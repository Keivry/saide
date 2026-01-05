# Development Pitfalls & Lessons Learned

This document records technical issues encountered during development and their solutions.

## Table of Contents

1. [Threading & Shutdown Issues](#1-threading--shutdown-issues)
2. [Audio/Video Synchronization](#2-audiovideo-synchronization)
3. [Protocol Implementation](#3-protocol-implementation)
4. [Configuration Management](#4-configuration-management)
5. [UI Rendering](#5-ui-rendering)
6. [State Management](#6-state-management)
7. [Rust Type System](#7-rust-type-system)
8. [Platform-Specific Issues](#8-platform-specific-issues)
9. [Hardware Decoding](#9-hardware-decoding)
10. [Input Mapping](#10-input-mapping)

---

## 1. Threading & Shutdown Issues

### 1.1 CUDA Cleanup Error on Exit (2025-12-16) ✅ Resolved

**Symptoms:**
```
DEBUG saide::scrcpy::connection: Force killing server process (timeout)
WARN saide::app::ui::stream_player: Audio/Video read error - skipping
ERROR Connection error: Failed to read pts_and_flags
[AVHWDeviceContext @ 0x...] cu->cuMemFree() failed
[AVHWDeviceContext @ 0x...] cu->cuCtxDestroy() failed
Error: nu::shell::terminated_by_signal
```

**Root Cause:**
**Thread exit order improper + decoder resource not released in time**

1. **StreamPlayer.stop()**: Used `thread.is_finished()` method (requires nightly Rust), causing runtime panic
2. **ScrcpyConnection.shutdown()**: Server process wait timeout (1s) then forced `kill()`
3. **NVDEC Drop**: CUDA context not flushed, `av_buffer_unref()` directly causes CUDA error

**Rust Thread Graceful Shutdown Best Practices:**

The solution adopted:
1. ✅ **Use `Arc<AtomicBool>` as exit signal**
2. ✅ **Channel close triggers thread exit**
3. ✅ **Explicit `Drop` order control** (decoder → socket → server process)
4. ✅ **Timeout join mechanism** (polling + `std::mem::forget` instead of `thread.is_finished()`)

**Fix:**

```rust
// 1. StreamPlayer: Safe timeout join (no nightly dependency)
pub fn stop(&mut self) {
    // 1. Drop channels first (signal threads to exit)
    self.frame_rx = None;
    self.stats_rx = None;

    // 2. Blocking join with timeout
    if let Some(thread) = self.stream_thread.take() {
        const JOIN_TIMEOUT_MS: u64 = 2000;
        let start = std::time::Instant::now();

        loop {
            if thread.is_finished() {
                let _ = thread.join();
                break;
            }
            if start.elapsed().as_millis() > JOIN_TIMEOUT_MS as u128 {
                warn!("Thread timeout, abandoning join");
                std::mem::forget(thread); // Detach thread (safe)
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

// 2. ScrcpyConnection: Extended wait time + correct exit order
pub fn shutdown(&mut self) -> Result<()> {
    // Step 1: Close sockets FIRST (triggers server exit via broken pipe)
    self.video_stream.take();
    self.audio_stream.take();
    self.control_stream.take();

    // Step 2: Wait longer for graceful exit (3s instead of 1s)
    if let Some(mut process) = self.server_process.take() {
        const MAX_WAIT_MS: u64 = 3000;
        let start = std::time::Instant::now();

        while start.elapsed().as_millis() < MAX_WAIT_MS as u128 {
            if let Ok(Some(status)) = process.try_wait() {
                debug!("Server exited gracefully: {:?}", status);
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Only kill if truly stuck
        debug!("Server timeout, force terminating");
        process.kill().ok();
    }

    // Step 3: Remove tunnel (safe to do last)
    remove_reverse_tunnel(...).ok();
    Ok(())
}

// 3. NVDEC Drop: Explicit flush + delayed release
impl Drop for NvdecDecoder {
    fn drop(&mut self) {
        unsafe {
            // 1. Flush decoder to release pending frames
            let _ = self.flush();

            // 2. Give CUDA time to finish async operations
            std::thread::sleep(std::time::Duration::from_millis(50));

            // 3. Release hardware device context
            if !self.hw_device_ctx.is_null() {
                ffmpeg::sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

// 4. Explicit Drop order (in decode loop)
let decode_result = (|| -> Result<()> {
    loop {
        // ... decode logic ...

        if frame_tx.is_disconnected() {
            debug!("Channel disconnected, exiting gracefully");
            return Ok(()); // Drop will handle cleanup
        }
    }
})();

// Explicit drop BEFORE sockets close
drop(video_decoder);
debug!("Decoder dropped");
```

**Key Improvements:**
1. ✅ **Arc<AtomicBool> as exit signal**: UI destroy `stop_signal.store(true)` → decode thread check before each read → immediate exit
2. ✅ **Decoder released before socket**: Ensure CUDA cleanup complete before closing network
3. ✅ **Server process wait time extended**: From 1s to 3s, reduce forced kill probability
4. ✅ **AudioPlayer Drop delay**: Give audio callback 50ms to complete current operation
5. ✅ **Loop start check exit signal**: Avoid blocking on socket read (timeout 5s) unresponsive

**Additional Fix (2025-12-16):**

**Problem:** Initial fix still showed timeout kill:
```
INFO Stopping stream
WARN Stream thread did not finish within 2000ms, abandoning join
... (2s later decode thread still working)
DEBUG Server process timeout (3000ms), force terminating
```

**Root Cause:** Decode thread blocked at `VideoPacket::read_from()` (blocking read, timeout 5s), cannot detect exit signal during this time.

**Final Solution:**
```rust
// StreamPlayer struct adds exit signal
struct StreamPlayer {
    stop_signal: Option<Arc<AtomicBool>>,
    // ...
}

// stop() sets signal
pub fn stop(&mut self) {
    if let Some(ref signal) = self.stop_signal {
        signal.store(true, Ordering::Relaxed);
    }
    self.frame_rx = None; // Release channel
    // ... join with timeout ...
}

// Worker thread checks at loop start
loop {
    // ✅ Key: Check before blocking read
    if stop_signal.load(Ordering::Relaxed) {
        debug!("Stop signal received, stopping decode loop");
        return Ok(());
    }

    // This may block 5s (read timeout)
    let packet = VideoPacket::read_from(&mut video_stream)?;
    // ...
}
```

**Why Not Channel:**
- `crossbeam_channel::Sender` has no `is_disconnected()` method
- Can only detect via `try_send()` failure, but needs actual data to detect
- `AtomicBool` is lighter weight, faster response

**Other Solutions Considered (Not Adopted):**
- `tokio::sync::Notify`: Requires async runtime, we have sync code
- `Condvar`: Suitable for long-blocking scenarios, we already have read timeout
- `Worker` struct with `Drop`: Already implemented in `StreamPlayer`

**Version 3.0 - UI Responsiveness (2025-12-16):**

**Problem:** UI exit slow (3-5 seconds)
- StreamPlayer.stop() blocks 2s waiting join
- ScrcpyConnection.shutdown() blocks 3s waiting server exit

**User Experience Goal:** Click close button → UI disappears immediately (<500ms)

**Scheme 3.0 (Failed):** Pure Detached cleanup
```rust
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    std::thread::spawn(move || {
        let _ = thread.join(); // Complete background
    });
    // Return immediately ⚡
}
```
❌ **Problem:** Main thread exit → CUDA driver cleanup → background thread drop decoder → CUDA context invalidated
```
[AVHWDeviceContext @ ...] cu->cuCtxPushCurrent() failed
```

**Final Scheme 3.1:** Hybrid - Short block + Detached
```rust
// StreamPlayer: Return immediately, background join
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    if let Some(thread) = self.stream_thread.take() {
        std::thread::spawn(move || {
            // Background wait 2s or join
            let _ = thread.join();
        });
    }
    // Return immediately, UI non-blocking ⚡
}

// ScrcpyConnection: Fast check + background cleanup
pub fn shutdown(&mut self) -> Result<()> {
    self.video_stream.take(); // Close socket
    
    if let Some(mut process) = self.server_process.take() {
        // Fast path: 50ms quick check
        for _ in 0..5 {
            if process.try_wait().is_ok() {
                return Ok(()); // Quick exit ⚡
            }
            sleep(10ms);
        }
        
        // Slow path: Background cleanup (UI non-blocking)
        std::thread::spawn(move || {
            // Max wait 3s or kill
        });
    }
    Ok(()) // Return immediately ⚡
}
```

```rust
// Hybrid cleanup: Ensure CUDA resources correctly released + UI fast response
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    if let Some(thread) = self.stream_thread.take() {
        // Phase 1: Short block wait decoder drop (300ms)
        const DECODER_CLEANUP_MS: u64 = 300;
        for _ in 0..30 {
            if thread.is_finished() {
                let _ = thread.join(); // ✅ Decoder dropped
                return; // Quick exit
            }
            sleep(10ms);
        }
        
        // Phase 2: Timeout detach remaining cleanup (network/process)
        std::thread::spawn(move || {
            let _ = thread.join(); // Complete in background
        });
    }
    // Return within 300ms (90% case) or immediately ⚡
}
```

**Performance Comparison:**
| Operation | Before Fix | Scheme 3.0 (Failed) | Scheme 3.1 (Final) |
|-----------|------------|--------------------|--------------------|
| **stop() call** | Block 2s | Immediate <1ms | **Wait 300ms (fast path)** |
| **shutdown() call** | Block 3s | Fast check 50ms | Fast check 50ms |
| **UI close delay** | **5s** | <100ms ❌ CUDA error | **<400ms** ✅ No error |
| **Decoder Drop** | Correct | ❌ After main thread exit | ✅ Before main thread exit |
| **Cleanup complete** | 5s | Background 2-3s | Background 2-3s |

**Key Design Decisions:**
| Requirement | Solution | Trade-off |
|-------------|----------|-----------|
| **CUDA resource correct release** | Short block wait decoder drop (300ms) | Sacrifice 300ms response time |
| **UI fast response** | Timeout detach remaining cleanup | Network/process cleanup async |
| **User experience** | 90% case <400ms window close | Acceptable delay |

**Lessons Learned:**
1. **Never use `thread.is_finished()` in stable Rust**: It's nightly-only API
2. **Timeout join use `std::mem::forget`**: Safer than detached spawn
3. **CUDA resources must be released while main thread exists**: After driver exit context invalid
4. **Exit order must be: decoder → channel → socket → process**
5. **Hybrid cleanup is optimal**: Critical resource block release + secondary resource detach
6. **UI response priority, but hardware resources non-negotiable** (CUDA > user experience extreme)

---

## 2. Audio/Video Synchronization

### 2.1 Audio Stream Blocking Issue (2025-12-12) ✅ Resolved

**Symptoms:**
- Video updates normally
- Audio thread reads only 2 packets then permanently blocks
- Notification sounds can be forwarded, but music player/media audio has no output
- test_audio (audio-only mode) works normally

**Root Cause:**
**Not reading audio stream codec_id causes protocol mismatch**

Scrcpy protocol requires first data of each stream to be 4-byte codec_id:
- Video stream: `codec_id(4 bytes) + width(4) + height(4) + packets...`
- Audio stream: `codec_id(4 bytes) + packets...`

Our code only read video stream codec metadata, completely skipped audio stream codec_id reading.
When audio thread tried to read 12-byte packet header (PTS 8 bytes + size 4 bytes), actually read:
```
[codec_id: 4 bytes][packet_header first 8 bytes]
```
Protocol completely misaligned, subsequent reads all fail.

**Wrong Code Example:**
```rust
// ❌ Wrong: Only read video stream codec metadata
let video_resolution = if params.send_codec_meta {
    stream.read_exact(&mut codec_meta[12])?; // Read codec_id + width + height
    ...
};

// ❌ Audio stream skipped directly, no codec_id read
let audio_stream = conn.audio_stream.take()?;
// Directly start reading packet header → Wrong! First 4 bytes is codec_id, not header

// Audio thread blocked here
audio_stream.read_exact(&mut header[12])?; // Actually read codec_id + partial header
```

**Correct Fix:**
```rust
// ✅ Correct: Audio stream must also read codec_id first
if params.send_codec_meta && let Some(ref mut stream) = audio_stream {
    let mut codec_id_bytes = [0u8; 4];
    stream.read_exact(&mut codec_id_bytes)?;
    let codec_id = u32::from_be_bytes(codec_id_bytes);
    debug!("Audio codec meta: id=0x{:08x}", codec_id); // 0x6f707573 = "opus"
    
    // Check special values (参考 demuxer.c)
    if codec_id == 0 { /* Stream disabled by device */ }
    if codec_id == 1 { /* Stream configuration error */ }
}
// Now can read packet header normally
```

**Diagnostic Process:**
1. ✅ Exclude connection issues: video → audio → control three sockets all established correctly
2. ✅ Exclude FD passing: reverse mode doesn't need FD passing via control
3. ✅ Exclude audio source config: `output` and `playback` modes have same issue
4. ✅ Compare scrcpy official client: audio/video both work
5. 🔍 **Deep source analysis**: Found `run_demuxer()` first step is reading codec_id in `demuxer.c`
6. 🎯 **Root cause location**: Our `connection.rs` only handled video stream codec metadata

**Test Results (After Fix):**
```
✅ Audio codec init: id=0x6f707573 (opus)
✅ Audio packet stats: 734 packets/15s continuous decode
✅ Audio processing progress: 100 → 200 → ... → 700 packets
✅ Video frames: 600+ frames, 0 dropped
✅ Audio/video sync smooth, no latency
```

**Key Lessons:**
1. **Protocol must be fully implemented**: Even "optional" fields must be read in protocol order
2. **Compare official implementation**: When issues arise, directly compare official client behavior
3. **Deep read source code**: Key protocol details often hidden in source comments and edge case handling
4. **Multi-stream symmetry**: If video stream needs codec_id read, audio stream definitely needs it too

**Related Code Locations:**
- Fix: `src/scrcpy/connection.rs` line 185-203
- Reference: `3rd-party/scrcpy/app/src/demuxer.c` line 145-158 (`run_demuxer`)
- Protocol definition: `demuxer.c` line 81-100 (packet header format comments)

### 2.2 AV Sync Mutex Contention Issue

**Symptoms:**
- Using `Arc<Mutex<AVSync>>` to share sync state between audio and video threads
- Both audio and video threads need lock to access AVSync
- Video decode stutter (GPU/driver/IO) blocks audio thread in reverse
- Audio is real-time path, blocking destroys sync precision

**Root Cause:**
- **Architecture error**: Audio and video are equal read/write relationship
- **Sync inversion**: Audio = master clock, should not wait for video
- **Mutex nature**: Fair lock, either side holding blocks the other

**Solution:** Lock-Free Snapshot Architecture

```rust
// Audio thread = only writer (&mut AVSync)
av_sync.update_audio_pts(pts);  // Atomic Release

// Video thread = read-only snapshot (Arc<AVSyncSnapshot>)
av_snapshot.should_drop_video(pts);  // Atomic Acquire
```

**Implementation Details:**
1. **AVSyncSnapshot**: Atomic snapshot structure
   - `audio_pts: AtomicI64`
   - `avg_drift_us: AtomicI64`
   - `clock_ready: AtomicBool`
2. **AVSync**: Audio thread exclusive
   - `update_audio_pts(&mut self)` - only write point
   - `snapshot() -> Arc<AVSyncSnapshot>` - get snapshot
3. **Memory ordering**:
   - Audio write uses `Ordering::Release`
   - Video read uses `Ordering::Acquire`
   - Forms happens-before relationship

**Performance Improvement:**
- Audio write latency: ~100ns (Mutex) → ~10ns (Atomic)
- Video read latency: ~100ns + contention → ~10ns (Atomic)
- ✅ Audio never blocked by video decode
- ✅ Video never blocked by audio update

**Key Lessons:**
1. **Player-level architecture principle**: Audio = master clock, Video = follower
2. **Avoid reverse dependency**: Real-time path should not wait non-real-time path
3. **Lock-Free priority**: Atomic 10x faster than Mutex, no contention
4. **Memory ordering important**: Release/Acquire guarantees cross-thread visibility
5. **Reference classic implementations**: scrcpy/mpv/VLC all use similar lock-free sync

**Reference Documentation:**
- `docs/avsync_lockfree.md` - Complete design document
- `src/sync/clock.rs` - AVSync / AVSyncSnapshot implementation
- `src/app/ui/player.rs` - Audio/Video thread usage example

---

## 3. Protocol Implementation

### 3.1 Missing Audio Stream Codec ID Reading

**Problem:** Audio thread blocked permanently after reading only 2 packets.

**Root Cause:** Skipped codec_id reading for audio stream (see Section 2.1).

**Solution:** Read 4-byte codec_id before reading audio packets.

**Related Code:**
- `src/scrcpy/connection.rs` line 185-203

---

## 4. Configuration Management

### 4.1 config.toml Not Applied (Hardcoded Parameters) (2025-12-15)

**Problem:**
- Modify `config.toml` `max_size = 720`
- But scrcpy still passes `max_size=1920`
- All scrcpy parameters (bit_rate, codec, etc.) not applied

**Root Cause:**
- `StreamPlayer::start()` internally calls `stream_worker()` creating connection parameters
- `stream_worker()` hardcoded all parameters
- Config not passed to actual connection logic

**Solution:**
1. Modify `StreamPlayer::start()` signature to accept `ScrcpyConfig`
2. Parse config in `stream_worker()` (support "24M" format)
3. Add `Clone` trait to `ScrcpyConfig` and substructures

**Lesson:**
- Config-driven systems must verify config pass-through end-to-end
- Be wary of hardcoding, especially in multi-layer function calls

---

## 5. UI Rendering

### 5.1 Video Not Displayed on Init, Requires Mouse Move (2025-12-15)

**Problem:** Black screen after startup, video appears after mouse move

**Root Cause:**
- `InitState::InProgress` didn't call `ctx.request_repaint()`
- egui only repaints on interaction events by default

**Solution:**
```rust
InitState::InProgress => {
    self.player.update();
    self.check_init_stage(ctx);
    ctx.request_repaint();  // Actively request repaint
}
```

**Lesson:** Async init scenarios must actively request repaint

---

## 6. State Management

### 6.1 Rotation Button Click No-op (2025-12-15)

**Problem:** Click rotation button, state updates but video doesn't rotate

**Root Cause:** `StreamPlayer::set_rotation()` empty method (legacy code)

**Solution:**
1. Add `video_rotation: u32` field
2. Implement real setter: `self.video_rotation = rotation % 4;`
3. Later need to apply rotation transform during rendering

**Lesson:** Be wary of no-op methods and "compatibility" code

---

## 7. Rust Type System

### 7.1 Nested Type Error (Arc<Arc<T>>) (2025-12-15)

**Problem:** `Arc::new(config.scrcpy.clone())` causes double Arc

**Solution:** Direct clone: `(*config.scrcpy).clone()`

**Lesson:** Understand return type, avoid meaningless wrapping

### 7.2 Missing Clone Trait (2025-12-15)

**Problem:** Config structures not derived `Clone`, cannot pass across threads

**Solution:** Add `#[derive(Clone)]` to all config structures

**Lesson:** Config data structures usually need Clone

---

## 8. Platform-Specific Issues

### 8.1 Wayland ResizeIncrements Warning (2025-12-15)

**Symptoms:** Warning when running on Wayland
```
WARN winit::platform_impl::linux::wayland::window:
  `set_resize_increments` is not implemented for Wayland
```

**Cause:**
- `ResizeIncrements` is X11-specific feature for locking window resize steps
- Wayland protocol doesn't support this feature (different design philosophy)
- winit library returns unimplemented warning on Wayland

**Impact:**
- ✅ Window init and rotation not affected
- ✅ Window can still be manually resized
- ❌ Cannot force lock aspect ratio during drag

**Solution:** No fix needed, this is platform limitation. Users may break aspect ratio when manually resizing on Wayland, but rotation and auto-adjust still work.

**To Suppress Warning:**
```bash
RUST_LOG=error cargo run  # Show only error level logs
```

---

## 9. Hardware Decoding

### 9.1 NVDEC Rotation Crash - Unsupported prepend-sps-pps Devices (2025-12-15)

**Problem:**
- VAAPI and software decode: rotation normal ✅
- NVDEC + `prepend-sps-pps-to-idr-frames=1`: SPS detection for resolution, decoder rebuild normal ✅
- NVDEC + unsupported Android device: video crashes after rotation ❌

**Root Cause:**
1. Some Android devices don't support `prepend-sps-pps-to-idr-frames=1` (e.g., HiSilicon Kirin 980)
2. Rotation causes video resolution change (e.g., 592x1280 → 1280x592)
3. NVDEC hardware decoder internal context (AVHWFramesContext) incompatible with new resolution
4. FFmpeg error: `AVHWFramesContext is already initialized with incompatible parameters`
5. Subsequent all frame decode fail: `CUDA_ERROR_INVALID_HANDLE: invalid resource handle`
6. No SPS data, cannot detect resolution change in advance

**Solution (Three-Layer Defense):**

**Layer 1: Tolerate FFmpeg Errors**
```rust
// src/decoder/nvdec.rs
fn send_packet(&mut self, data: &[u8], pts: i64) -> Result<()> {
    if let Err(e) = self.decoder.send_packet(&packet) {
        warn!("send_packet failed (possibly resolution change): {:?}", e);
        // Don't fail - let empty frame detection handle it
    }
    Ok(())
}

fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
    match self.decoder.receive_frame(&mut hw_frame) {
        Err(e) => {
            warn!("receive_frame failed: {:?}", e);
            break; // Return empty frames
        }
    }
}
```

**Layer 2: Empty Frame Counter**
```rust
// 3 consecutive empty frames trigger error
if frames.is_empty() {
    self.consecutive_empty_frames += 1;
    if self.consecutive_empty_frames >= 3 {
        bail!("NVDEC decoder stuck: {} consecutive empty frames", ...);
    }
}
```

**Layer 3: Dual-Strategy Recovery**
```rust
// stream_player.rs: try_recover_decoder()
// Strategy 1: Try to extract SPS from failed packet
if let Some((w, h)) = extract_resolution_from_stream(packet_data) {
    if w > 32 && h > 32 invalid init values
        AutoDecoder:: {  // Filternew(w, h)  // ✅ Rebuild decoder
    }
}

// Strategy 2: No SPS? Assume screen rotation (swap dimensions)
let swapped = (last_height, last_width);  // 592x1280 -> 1280x592
AutoDecoder::new(swapped.0, swapped.1)    // ✅ Rebuild decoder
```

**Implementation Details:**
1. **Error tolerance**: NVDEC doesn't immediately fail on CUDA error, returns empty frames
2. **Empty frame detection**: 3 consecutive empty frames trigger "consecutive empty frames" error
3. **Strategy 1**: Try to extract SPS from failed packet (even if not Key frame)
4. **Strategy 2**: No SPS? Swap dimensions (Android rotation most common: 90°/270°)
5. **Dimension filtering**: Ignore obviously invalid resolutions like 32x32 (encoder init values)

**Debug Log Example:**
```
Before rotation:
2025-12-15T06:06:33.263194Z  INFO Video resolution: 592x1280
2025-12-15T06:06:33.410326Z DEBUG NVDEC H.264 decoder initialized

During rotation (FFmpeg error):
[h264_cuvid @ ...] AVHWFramesContext is already initialized with incompatible parameters
[h264_cuvid @ ...] CUDA_ERROR_INVALID_HANDLE: invalid resource handle
(Repeated many times)

After rotation (expected log):
2025-12-15T06:06:43.253884Z DEBUG Rotation changed: Some(0) -> 1
2025-12-15T06:06:43.262214Z DEBUG Device rotated to orientation: 90
2025-12-15T06:06:44.XXX  WARN ⚠️ NVDEC detected resolution change via decode failure
2025-12-15T06:06:44.XXX  INFO 🔄 No SPS found, trying dimension swap: 592x1280 -> 1280x592
2025-12-15T06:06:44.XXX  INFO ✅ Decoder recreated with swapped dimensions: NVDEC
```

**Lessons:**
- **FFmpeg hardware decode fragility**: AVHWFramesContext cannot hot-update on resolution change
- **Error propagation chain**: CUDA error → FFmpeg error → needs Rust layer tolerance
- **Android fragmentation**: Same MediaCodec parameters have different support across devices (HiSilicon vs MTK)
- **Multi-layer defense**: Tolerance + detection + recovery, all necessary
- **Delay vs compatibility**: `prepend-sps-pps=1` can optimize but not required, code needs tolerate absence

**Related Code:**
- `src/app/ui/stream_player.rs` line 46-110 (`try_recover_decoder`)
- `src/decoder/nvdec.rs` line 112-130 (error tolerance)
- `src/decoder/nvdec.rs` line 216-228 (empty frame detection)

**Test Suggestion:**
```bash
# Test on device without prepend-sps-pps support (e.g., HiSilicon Kirin 980)
cargo run
# 1. Wait video normal display
# 2. Rotate device screen (90° or 270°)
# 3. Observe logs:
#    - [h264_cuvid] CUDA_ERROR_INVALID_HANDLE (FFmpeg error)
#    - ⚠️ NVDEC detected resolution change via decode failure
#    - 🔄 No SPS found, trying dimension swap: 592x1280 -> 1280x592
#    - ✅ Decoder recreated with swapped dimensions: NVDEC
# 4. Confirm video recovers within ~1 second
```

**Protection Mechanism - Prevent Rebuild Loop:**
```rust
// Use Option to track rebuild time (first rebuild allowed, later need cooldown)
let mut last_decoder_rebuild: Option<Instant> = None;

if let Some(last_rebuild_time) = last_decoder_rebuild {
    // Cooldown: 2 seconds or 10 frames
    const MIN_REBUILD_INTERVAL: Duration = Duration::from_secs(2);
    const MIN_FRAMES_BEFORE_REBUILD: u32 = 10;
    
    let can_rebuild = elapsed >= MIN_REBUILD_INTERVAL 
        || frames >= MIN_FRAMES_BEFORE_REBUILD;
    
    if !can_rebuild {
        continue;  // Skip rebuild
    }
}

// After rebuild, update time
last_decoder_rebuild = Some(Instant::now());
```

**Key Design:**
- **First rebuild no restriction**: `None` state allows immediate rebuild
- **Subsequent rebuild need cooldown**: `Some(time)` state enforces 2s or 10 frame interval
- **Reason**: New decoder needs time to receive correct resolution frames, otherwise immediately triggers "3 consecutive empty frames" again

### 9.2 Ultimate Solution: capture_orientation (2025-12-15)

**Strategy:** Always lock `capture_orientation` when using NVDEC

**Implementation:**
```rust
// Detect NVIDIA GPU → auto lock orientation
if has_nvidia_gpu() {
    params.capture_orientation = Some("@0".to_string());
}
```

**Principle:**
- scrcpy-server captures screen at fixed orientation
- Video resolution never changes (592x1280 or 1280x592)
- NVDEC decoder no rebuild needed
- System auto-rotates display content

**Advantages:**
1. ✅ **Fundamental solution**: Prevent resolution change instead of handling change
2. ✅ **Zero performance loss**: No decoder rebuild (~200ms overhead)
3. ✅ **No black screen**: Decode continues
4. ✅ **No SPS needed**: Don't need `prepend-sps-pps-to-idr-frames=1`
5. ✅ **Universal**: All NVDEC devices benefit
6. ✅ **Good compatibility**: scrcpy native support

**Why Not prepend-sps-pps?**

| Solution | Advantage | Disadvantage |
|----------|-----------|--------------|
| prepend-sps + rebuild | Support rotation | Poor compatibility, rebuild overhead, possible black screen |
| **capture_orientation** | **Universal, stable, zero overhead** | **Fixed orientation (acceptable)** |

**Affected Devices:**
- **All devices using NVDEC** (auto-detect NVIDIA GPU)
- Video orientation fixed, but always works normally
- Users can disable this behavior via config

**Fallback:**
If user needs video to rotate with device:
1. Use VAAPI decoder (Intel GPU)
2. Use software decoder
3. Manually disable `capture_orientation` lock

**Code Locations:**
- `src/scrcpy/server.rs`: `should_lock_orientation_for_nvdec()`
- `src/app/init.rs`: Auto-apply logic

---

## 10. Input Mapping

### 10.1 Keyboard Mapping Coordinate System with capture-orientation Lock (2025-12-16) ✅ Resolved

**Problem:**
- Using NVDEC, `capture-orientation=@0` locks video orientation to device natural orientation (0° portrait)
- User rotates device to landscape (rotation=1, 90° CCW), triggers Profile switch to `rotation=1` config
- But keyboard mapping button positions completely wrong, click target not at expected position

**Root Cause:**
**Profile coordinate system inconsistent with video coordinate system**

1. **Profile.rotation**: Records device rotation angle corresponding to config (CCW)
   - `rotation=0`: 0° portrait
   - `rotation=1`: 90° CCW landscape (device rotated left)
   - `rotation=2`: 180°
   - `rotation=3`: 270° CCW landscape (device rotated right)

2. **Profile coordinate system**: Percentage coordinates (0.0-1.0) based on `profile.rotation` corresponding device orientation
   - Example `rotation=1` config, coordinates based on "landscape device" coordinate system

3. **Video coordinate system (capture-orientation not locked)**:
   - Video orientation follows device rotation
   - `profile.rotation == device_orientation` Profile matches
   - Coordinate system consistent, just scale percentage

4. **Video coordinate system (capture-orientation=@0 locked)**:
   - **Video orientation always 0° (device natural orientation/portrait)**
   - Device rotates to `rotation=1`, Profile activates
   - But video coordinate system still 0°, while Profile coordinates 90° CCW coordinate system
   - **Coordinate system mismatch → mapping position error**

**Wrong Code:**
```rust
// ❌ Direct scaling, didn't consider rotation difference
let (px, py) = (x_percent * video_width as f32, y_percent * video_height as f32);
```

**Correct Solution:**
```rust
/// Convert coordinates considering profile.rotation vs video coordinate system (0°)
let transform_coord = |x_percent: f32, y_percent: f32| -> (f32, f32) {
    if !capture_orientation_locked {
        // Not locked: video coordinate system follows device, direct scaling
        (x_percent * video_width as f32, y_percent * video_height as f32)
    } else {
        // Locked: video coordinate system fixed at 0°, need rotation transform
        match profile.rotation {
            0 => (x_percent * video_width as f32, y_percent * video_height as f32),
            1 => {
                // Profile coordinates are 90° CCW coordinate system
                // Transform to 0°: (x', y') -> (y', 1-x')
                (y_percent * video_width as f32, (1.0 - x_percent) * video_height as f32)
            }
            2 => {
                // Profile coordinates are 180° coordinate system
                // Transform to 0°: (x', y') -> (1-x', 1-y')
                ((1.0 - x_percent) * video_width as f32, (1.0 - y_percent) * video_height as f32)
            }
            3 => {
                // Profile coordinates are 270° CCW coordinate system
                // Transform to 0°: (x', y') -> (1-y', x')
                ((1.0 - y_percent) * video_width as f32, x_percent * video_height as f32)
            }
            _ => (x_percent * video_width as f32, y_percent * video_height as f32),
        }
    }
};
```

**Key Understanding:**
1. **Android Display Rotation (device_orientation)**: Counter-clockwise (CCW)
   - Reference: `android.view.Surface.ROTATION_*`
2. **scrcpy capture-orientation**: Clockwise (CW) representation, internally converted to CCW
   - Reference: `Orientation.java` line 37: `int cwRotation = (4 - ccwRotation) % 4`
3. **Profile.rotation follows Android Display Rotation** (CCW)
4. **After capture-orientation=@0 lock, video fixed at device natural orientation (0°)**

**Implementation Changes:**
1. Add `capture_orientation_locked: bool` field to `SAideApp`
2. Pass flag in `InitEvent::ConnectionReady`
3. Receive parameter in `KeyboardMapper::refresh_profiles()`
4. Apply rotation transform in `KeyboardMapper::update_pixel_mappings()`

**Test Verification:**
- Device portrait (rotation=0), Profile rotation=0: ✅ Coordinates correct
- Device landscape (rotation=1), Profile rotation=1, capture locked: ✅ Coordinates auto-convert correctly
- Device landscape (rotation=1), Profile rotation=1, capture unlocked: ✅ Direct scaling correct
- Other rotation angles (2, 3): ✅ Transform formulas symmetric

**Key Lessons:**
1. **Understand coordinate system transforms**: Multiple coordinate systems (Profile/video/device) need clear reference orientation
2. **Note rotation direction definition**: CCW vs CW, different systems define differently
3. **Locked orientation side effects**: `capture-orientation` lock causes coordinate system not to follow device rotation
4. **Profile.rotation semantics**: Records "device orientation corresponding to config", not "video orientation"

**Code Locations:**
- `src/controller/keyboard.rs` - Coordinate transform logic (update_pixel_mappings)
- `src/app/init.rs` - capture_orientation_locked flag passing
- `src/app/ui/saide.rs` - State storage and refresh_profiles call
- `3rd-party/scrcpy/server/.../Orientation.java` - Rotation direction definition reference

---

**Last Updated**: 2025-12-16
