# SAide Architecture Overview

## Project Overview

SAide is a Rust-based Android device remote control application inspired by scrcpy. It provides high-performance video streaming, audio capture, and low-latency input control from Android devices to a desktop UI built with egui.

### Key Features

- **High-Performance Video Streaming**: H.264/H.265/AV1 video decoding with hardware acceleration (VAAPI, NVDEC)
- **Low-Latency Input**: Direct scrcpy control channel for keyboard and mouse input (40-90ms reduction vs ADB)
- **Audio Capture**: Opus/AAC/FLAC audio streaming from Android 11+ devices
- **Keyboard Mapping**: Custom key mappings with coordinate systems supporting screen rotation
- **Cross-Platform**: Linux primary, with architecture designed for Windows/macOS expansion

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            SAide Application                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                         UI Layer (egui)                              ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               ││
│  │  │   SAideApp   │  │   Toolbar    │  │   Indicator  │               ││
│  │  │  (Main App)  │  │   Controls   │  │    Panel     │               ││
│  │  └──────────────┘  └──────────────┘  └──────────────┘               ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                      │
│                                    ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                     Controller Layer                                  ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               ││
│  │  │KeyboardMapper│  │  MouseMapper │  │ControlSender │               ││
│  │  └──────────────┘  └──────────────┘  └──────────────┘               ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                      │
│                                    ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                      Scrcpy Protocol Layer                            ││
│  │  ┌───────────────────────────────────────────────────────────────┐  ││
│  │  │                  ScrcpyConnection                              │  ││
│  │  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────────┐  ││
│  │  │  │ VideoStream │ │ AudioStream │ │    ControlStream        │  ││
│  │  │  └─────────────┘ └─────────────┘ └─────────────────────────┘  ││
│  │  └───────────────────────────────────────────────────────────────┘  ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               ││
│  │  │ControlSender │  │DeviceMsgRecv │  │  ServerMgr   │               ││
│  │  └──────────────┘  └──────────────┘  └──────────────┘               ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                      │
│                                    ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                      Decoder Layer                                    ││
│  │  ┌───────────────────────────────────────────────────────────────┐  ││
│  │  │                    VideoDecoder                                 │  ││
│  │  │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────────┐ │  ││
│  │  │  │ H264Decoder  │ │  NvdecDecoder│ │   VaapiDecoder         │ │  ││
│  │  │  │  (Software)  │ │  (NVIDIA)    │ │   (Intel)              │ │  ││
│  │  │  └──────────────┘ └──────────────┘ └────────────────────────┘ │  ││
│  │  └───────────────────────────────────────────────────────────────┘  ││
│  │  ┌───────────────────────────────────────────────────────────────┐  ││
│  │  │                    AudioDecoder                                 │  ││
│  │  │  ┌──────────────┐ ┌──────────────┐                            │  ││
│  │  │  │ OpusDecoder  │ │  AacDecoder  │                            │  ││
│  │  │  └──────────────┘ └──────────────┘                            │  ││
│  │  └───────────────────────────────────────────────────────────────┘  ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                    │                                      │
│                                    ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                      Renderer Layer                                   ││
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               ││
│  │  │ StreamPlayer │  │  CoordSys    │  │   Renderer   │               ││
│  │  └──────────────┘  └──────────────┘  └──────────────┘               ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ ADB Tunnel
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       Android Device (scrcpy-server)                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                   │
│  │ Video Encoder│  │ Audio Capture│  │ Input Injector│                   │
│  │  (H.264)     │  │   (Opus)     │  │              │                   │
│  └──────────────┘  └──────────────┘  └──────────────┘                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

### Core Modules

```
src/
├── main.rs                 # Application entry point
├── lib.rs                 # Library root
├── error.rs               # Unified error types
├── logging.rs             # Logging configuration
│
├── app/                   # Application layer
│   ├── mod.rs
│   ├── init.rs            # Initialization logic
│   ├── config.rs          # Configuration management
│   ├── coords.rs          # Coordinate system management
│   └── ui/                # UI components
│       ├── mod.rs
│       ├── saide.rs       # Main application state
│       ├── stream_player.rs # Video/audio rendering
│       ├── toolbar.rs     # Toolbar controls
│       ├── indicator.rs   # Status indicators
│       └── mapping.rs     # Key mapping configuration
│
├── controller/            # Input control layer
│   ├── mod.rs
│   ├── control_sender.rs  # Control message sending
│   ├── keyboard.rs        # Keyboard mapping
│   └── mouse.rs           # Mouse input handling
│
├── scrcpy/                # Scrcpy protocol implementation
│   ├── mod.rs
│   ├── connection.rs      # Connection management
│   ├── server.rs          # Server startup
│   └── protocol/          # Protocol message handling
│       ├── mod.rs
│       ├── control.rs     # Control messages (PC→Device)
│       ├── device.rs      # Device messages (Device→PC)
│       ├── video.rs       # Video packet parsing
│       └── audio.rs       # Audio packet parsing
│
├── decoder/               # Media decoding
│   ├── mod.rs
│   ├── video/             # Video decoders
│   │   ├── mod.rs
│   │   ├── h264.rs        # Software H.264 decoder
│   │   ├── nvdec.rs       # NVIDIA NVDEC decoder
│   │   └── vaapi.rs       # Intel VAAPI decoder
│   └── audio/             # Audio decoders
│       ├── mod.rs
│       ├── opus.rs        # Opus decoder
│       └── player.rs      # Audio playback (cpal)
│
├── sync/                  # Synchronization
│   └── clock.rs           # AV sync clock (lock-free)
│
└── utils/                 # Utilities
    └── ...
```

---

## Key Components

### 1. StreamPlayer

The central component for video and audio rendering.

**Responsibilities:**
- Manage video/audio decode threads
- Coordinate frame rendering with egui
- Handle device rotation and resolution changes
- Implement lock-free AV synchronization

**Key Features:**
- Hardware acceleration (VAAPI, NVDEC, Software)
- NV12 and RGBA rendering pipelines
- Dynamic resolution switching
- Frame dropping for AV sync

**Source**: `src/app/ui/stream_player.rs`

### 2. ScrcpyConnection

Manages the three-way TCP connection to the Android device.

**Connections:**
1. **Video Stream**: H.264/H.265/AV1 encoded video
2. **Audio Stream**: Opus/AAC/FLAC encoded audio (optional)
3. **Control Channel**: Bidirectional control messages

**Responsibilities:**
- ADB reverse tunnel setup
- Server process management
- Socket lifecycle (connect, shutdown)
- Codec metadata extraction

**Source**: `src/scrcpy/connection.rs`

### 3. ControlSender

Type-safe control message sender via scrcpy control channel.

**Supported Messages:**
- Inject keycode (keyboard input)
- Inject text (clipboard paste)
- Inject touch event (mouse/touch)
- Inject scroll event (mouse wheel)
- Set clipboard
- Screen power control

**Source**: `src/controller/control_sender.rs`

### 4. Coordinate Systems

Three coordinate systems for input mapping:

| System | Purpose | Transformations |
|--------|---------|-----------------|
| **MappingCoordSys** | User-defined key mappings | Percentage → Pixel based on device orientation |
| **ScrcpyCoordSys** | Scrcpy protocol coordinates | Video resolution, capture orientation |
| **VisualCoordSys** | UI display coordinates | Video rect, window size, visual rotation |

**Source**: `src/app/coords.rs`

### 5. Lock-Free AV Sync

Atomic snapshot architecture for audio/video synchronization.

**Architecture:**
- **Audio Thread**: Only writer (`&mut AVSync`) → updates PTS
- **Video Thread**: Only reader (`Arc<AVSyncSnapshot>`) → reads PTS snapshot

**Performance:**
- Audio write: ~10ns (vs ~100ns with Mutex)
- Video read: ~10ns (vs ~100ns + contention with Mutex)
- Zero contention between threads

**Source**: `src/sync/clock.rs`

---

## Data Flow

### Video Streaming Path

```
Android Device                    PC (SAide)
     │                                │
     │  H.264 NAL Units               │
     ├────────────────────────────────►│
     │                                │
     │                          ┌─────┴─────┐
     │                          │ Scrcpy    │
     │                          │ Connection│
     │                          └─────┬─────┘
     │                                │
     │                          ┌─────┴─────┐
     │                          │ Video     │
     │                          │ Decoder   │
     │                          │(NVDEC/    │
     │                          │ VAAPI)    │
     │                          └─────┬─────┘
     │                                │
     │                          ┌─────┴─────┐
     │                          │  Texture  │
     │                          │  Upload   │
     │                          └─────┬─────┘
     │                                │
     │                          ┌─────┴─────┐
     │                          │   egui    │
     │                          │ Render    │
     │                          └───────────┘
```

### Input Control Path

```
User Input                    SAide                          Android
    │                           │                               │
    │ Mouse/Keyboard Event      │                               │
    ├───────────────────────────►│                               │
    │                           │                               │
    │                     ┌─────┴─────┐                         │
    │                     │ Coordinate│                         │
    │                     │ Transform │                         │
    │                     └─────┬─────┘                         │
    │                           │                               │
    │                     ┌─────┴─────┐                         │
    │                     │  Control  │                         │
    │                     │  Sender   │                         │
    │                     └─────┬─────┘                         │
    │                           │                               │
    │                           │ Control Message               │
    │                           ├──────────────────────────────►│
    │                           │                               │
    │                           │                         ┌─────┴─────┐
    │                           │                         │ Input     │
    │                           │                         │ Injector  │
    │                           │                         └───────────┘
```

---

## Configuration System

### Configuration File

```toml
[scrcpy]
serial = "device_serial"
max_size = 1920
bit_rate = "8M"
max_fps = 60
video_codec = "h264"
audio = true
audio_codec = "opus"
tunnel_forward = false
stay_awake = true
turn_screen_off = false

[scrcpy.options]
prepend_sps_pps = true
capture_orientation = "@0"

[keyboard]
enabled = true
profile = "Default"

[mouse]
enabled = true
mapping_file = "keymap.toml"
```

### Profile System

Keyboard mappings organized by device orientation:

```toml
[profiles.0]  # Portrait (0°)
[profiles.0.mappings]
"F1" = { action = "tap", x = 0.5, y = 0.5 }

[profiles.1]  # Landscape (90° CCW)
[profiles.1.mappings]
"F1" = { action = "tap", x = 0.3, y = 0.7 }
```

---

## Dependencies

### Core Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| tokio | 1.x | Async runtime for network I/O |
| egui | 0.x | UI framework |
| capnp | 3.x | Message serialization |
| ffmpeg-next | 5.x | Media decoding |
| cpal | 0.15 | Audio playback |
| rustyline | 10.x | CLI interface |

### Build Dependencies

| Dependency | Purpose |
|------------|---------|
| cargo | Build system |
| clang | C library binding |
| pkg-config | Library detection |
| FFmpeg development headers | Media codec support |
| VAAPI/NVDEC development headers | Hardware decode support |

---

## Platform Considerations

### Linux (Primary)

- **Video**: VAAPI (Intel), NVDEC (NVIDIA), Software fallback
- **Audio**: PulseAudio, ALSA via cpal
- **Display**: X11 and Wayland support via winit
- **ADB**: Standard Android SDK tools

### Windows (Future)

- **Video**: D3D11 (DirectX), WGPU
- **Audio**: WASAPI via cpal
- **Display**: DirectComposition
- **ADB**: Windows SDK

### macOS (Future)

- **Video**: VideoToolbox, Metal
- **Audio**: CoreAudio via cpal
- **Display**: CAMetalLayer
- **ADB**: iOS not supported (Android only)

---

## Performance Characteristics

### Latency Breakdown

| Component | Typical Latency | Optimization |
|-----------|-----------------|--------------|
| Android Encode | 15-35ms | Hardware encoder, Baseline profile |
| Network (USB) | 1-3ms | TCP_NODELAY |
| PC Decode | 5-15ms | Hardware decode (VAAPI/NVDEC) |
| Render | 5-10ms | NV12 zero-copy |
| **Total** | **30-60ms** | Near scrcpy performance |

### Resource Usage

| Resource | Typical Usage | Notes |
|----------|---------------|-------|
| CPU | 5-15% (hardware decode) | Higher with software decode |
| GPU | 5-10% (VAAPI/NVDEC) | Video decode and rendering |
| Memory | 50-100MB | Frame buffers, codec contexts |
| Network | 2-10 Mbps | Depends on bit_rate setting |

---

## Error Handling

### Error Categories

```rust
pub enum SaideError {
    Connection(ConnectionError),
    Protocol(ProtocolError),
    Decode(DecodeError),
    IO(IOError),
    Audio(AudioError),
    Config(ConfigError),
   Cancelled,      // User cancelled, no error logging
    ConnectionLost, // Connection dropped, expected during shutdown
    Unknown,
}
```

### Error Handling Strategy

- **Cancelled**: Silent handling, no logging
- **ConnectionLost**: Info level, expected during normal shutdown
- **Decode/Protocol**: Warning level, may be recoverable
- **IO/Config**: Error level, requires attention

**Source**: `src/error.rs`

---

## Threading Model

### Thread Layout

```
Main Thread (egui)
    │
    ├── Stream Worker Thread
    │   ├── Video Decode Thread
    │   ├── Audio Decode Thread
    │   └── Render Thread (called from egui)
    │
    ├── Control Thread (tokio runtime)
    │   ├── Control Message Send
    │   └── Device Message Receive
    │
    └── Audio Playback Thread (cpal)
```

### Synchronization

- **AV Sync**: Lock-free atomic snapshots (no Mutex)
- **Frame Delivery**: Bounded channel (capacity: 1)
- **State Access**: Thread-safe interior mutability (Arc, AtomicBool)

---

## Build Variants

### Development

```bash
cargo build
```

### Release (Hardware Acceleration)

```bash
cargo build --release
```

### Software Decode Only

```bash
cargo build --no-default-features --features software_decode
```

---

## Related Documentation

- [Protocol Specification](SCRCPY_PROTOCOL.md) - Scrcpy protocol details
- [Development Guide](DEVELOPMENT.md) - Setting up development environment
- [Pitfalls & Lessons](pitfalls.md) - Known issues and solutions
- [Task Tracker](TODO.md) - Project progress tracking
