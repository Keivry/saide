# SAide Architecture Overview

## Project Overview

SAide is a Rust-based Android device remote control application inspired by scrcpy. It provides
high-performance video streaming, audio capture, and low-latency input control from Android devices
to a desktop UI built with egui.

### Key Features

- **High-Performance Video Streaming**: H.264/H.265/AV1 video decoding with hardware acceleration (NVDEC, VAAPI, D3D11VA)
- **Low-Latency Input**: Direct scrcpy control channel for keyboard and mouse input (40-90ms reduction vs ADB)
- **Audio Capture**: Opus audio streaming from Android 11+ devices
- **Keyboard Mapping**: Custom key mappings with coordinate systems supporting screen rotation
- **Cross-Platform**: Linux primary, with architecture designed for Windows/macOS expansion

---

## System Architecture

```text
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
│  │  │  │ H264Decoder  │ │  NvdecDecoder│ │ VAAPI / D3D11VA        │ │  ││
│  │  │  │  (Software)  │ │  (NVIDIA)    │ │ (Intel/AMD)            │ │  ││
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

```text
src/
├── main.rs                 # Application entry point
├── lib.rs                 # Library root
├── error.rs               # Unified error types
├── constant.rs            # Constants and default values
│
├── core/                  # Application layer
│   ├── mod.rs
│   ├── init.rs            # Initialization logic
│   ├── state.rs           # App/config/UI state structures
│   ├── profile_manager.rs # Mapping profile management
│   ├── connection.rs      # Connection management service
│   ├── device_monitor.rs  # Device monitoring service
│   ├── utils.rs           # Utility functions
│   ├── coords/            # Coordinate system management
│   └── ui/                # UI components
│       ├── mod.rs
│       ├── app.rs         # Main application state and event loop
│       ├── editor.rs      # Mapping editor
│       ├── dialog.rs      # Dialog components
│       ├── function.rs    # Profile/mapping actions
│       ├── player.rs      # Video/audio rendering
│       ├── toolbar.rs     # Toolbar controls
│       └── indicator.rs   # Status indicators
│
├── config/                # Configuration management
│   ├── mod.rs
│   ├── log.rs             # Logging configuration
│   ├── scrcpy.rs          # Scrcpy-specific config
│   └── mapping.rs         # Key mapping configuration
│
├── controller/            # Input control layer
│   ├── mod.rs
│   ├── adb.rs             # ADB shell interface
│   ├── control_sender.rs  # Control message sending
│   ├── keyboard.rs        # Keyboard mapping
│   └── mouse.rs           # Mouse input handling
│
├── scrcpy/                # Scrcpy protocol implementation
│   ├── mod.rs
│   ├── connection.rs      # Connection management
│   ├── server.rs          # Server startup
│   ├── codec_probe.rs     # Codec detection
│   ├── hwcodec.rs         # Hardware codec support
│   └── protocol/          # Protocol message handling
│       ├── mod.rs
│       ├── control.rs     # Control messages (PC→Device)
│       ├── video.rs       # Video packet parsing
│       └── audio.rs       # Audio packet parsing
│
├── decoder/               # Media decoding
│   ├── mod.rs
│   ├── auto.rs            # Cascade fallback decoder selection
│   ├── error.rs           # Decoder error types
│   ├── h264.rs            # Software H.264 decoder
│   ├── h264_parser.rs     # H.264 NAL parser
│   ├── nvdec.rs           # NVIDIA NVDEC decoder (cross-platform)
│   ├── vaapi.rs           # Linux VAAPI decoder (Intel/AMD)
│   ├── d3d11va.rs         # Windows D3D11VA decoder (Intel/AMD/NVIDIA)
│   ├── nv12_render.rs     # NV12 rendering pipeline
│   ├── rgba_render.rs     # RGBA rendering pipeline
│   └── audio/             # Audio decoders
│       ├── mod.rs
│       ├── error.rs       # Audio error types
│       ├── opus.rs        # Opus decoder
│       └── player.rs      # Audio playback (cpal)
│
├── avsync/                # Audio-video synchronization
│   ├── mod.rs
│   └── clock.rs           # AV sync clock (lock-free)
│
├── profiler/              # Performance profiling
│   ├── mod.rs
│   └── latency.rs         # Latency profiler
│
├── i18n/                  # Internationalization
│   ├── mod.rs
│   ├── manager.rs         # i18n manager
│   ├── source.rs          # i18n source trait
│   ├── embedded.rs        # Embedded resources
│   └── fs_source.rs       # Filesystem source
│
└── gpu/                   # GPU detection (legacy - used for optimization hints only)
    └── mod.rs             # GPU type detection
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

**Source**: `src/core/ui/player.rs`

### 2. ScrcpyConnection

Manages the three-way TCP connection to the Android device.

**Connections:**

1. **Video Stream**: H.264/H.265/AV1 encoded video
2. **Audio Stream**: Opus encoded audio (optional)
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

| System              | Purpose                     | Transformations                                |
| ------------------- | --------------------------- | ---------------------------------------------- |
| **MappingCoordSys** | User-defined key mappings   | Percentage → Pixel based on device orientation |
| **ScrcpyCoordSys**  | Scrcpy protocol coordinates | Video resolution, capture orientation          |
| **VisualCoordSys**  | UI display coordinates      | Video rect, window size, visual rotation       |

**Source**: `src/core/coords/`

### 5. Lock-Free AV Sync

Atomic snapshot architecture for audio/video synchronization.

**Architecture:**

- **Audio Thread**: Only writer (`&mut AVSync`) → updates PTS
- **Video Thread**: Only reader (`Arc<AVSyncSnapshot>`) → reads PTS snapshot

**Performance:**

- Audio write: ~10ns (vs ~100ns with Mutex)
- Video read: ~10ns (vs ~100ns + contention with Mutex)
- Zero contention between threads

**Source**: `src/avsync/clock.rs`

---

### 6. Command & Shortcut System

SAide uses a two-crate command system to decouple keyboard shortcuts from application logic.

**Crates**:

| Crate | Responsibility |
|---|---|
| `egui-command` | Pure command identity (`CommandId`, `CommandSpec`, `CommandState`) — no egui dependency |
| `egui-command-binding` | egui-aware dispatch: `ShortcutManager<C>`, `ShortcutMap<C>`, `ShortcutScope<C>` |

**`AppCommand` Enum** (`src/core/ui/mod.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppCommand {
    ShowHelp,              // F1
    ShowProfileSelection,  // F6
    PrevProfile,           // F7
    NextProfile,           // F8
    ShowRenameDialog,      // F2 (editor scope)
    ShowCreateDialog,      // F3 (editor scope)
    ShowDeleteDialog,      // F4 (editor scope)
    ShowSaveAsDialog,      // F5 (editor scope)
    CloseEditor,           // Esc (editor scope)
}
```

`AppCommand` must be `Copy + Hash` to be used as a `ShortcutMap` key. Commands that need to carry data (e.g., a file path or mapping position) use `PendingCommand` instead, which is stored separately in application state.

**`PendingCommand` Enum** — dialog-result carriers:

```rust
pub enum PendingCommand {
    RenameProfile, CreateProfile, SaveProfileAs, DeleteProfile, SwitchProfile,
    AddMapping(MappingPos), DeleteMapping(Key), ProbeCodec,
}
```

**Shortcut Registration** (`src/core/ui/mod.rs`):

```rust
// Global shortcuts (always active)
lazy_static! {
    pub static ref GLOBAL_SHORTCUTS: Arc<RwLock<ShortcutMap<AppCommand>>> =
        Arc::new(RwLock::new(shortcut_map! {
            "F1" => AppCommand::ShowHelp,
            "F6" => AppCommand::ShowProfileSelection,
            "F7" => AppCommand::PrevProfile,
            "F8" => AppCommand::NextProfile,
        }));
    pub static ref SHORTCUT_MANAGER: ShortcutManager<AppCommand> =
        ShortcutManager::new(GLOBAL_SHORTCUTS.clone());
}
```

Editor-specific shortcuts (F2–F5, Esc) are registered as a `ShortcutScope` pushed onto the `SHORTCUT_MANAGER` stack when the editor panel is active, and popped on close.

**Dispatch Flow**:

```text
egui frame input
       │
       ▼
ShortcutManager::dispatch(ctx)
       │  lookup order:
       │  1. Extra scope (if provided, always consuming)
       │  2. Scope stack top-down (stop at consuming scope)
       │  3. Global ShortcutMap
       │
       ▼
Vec<AppCommand>   ───►   app.rs process_commands()
                               │
                   ┌───────────┴───────────┐
                   ▼                       ▼
           Direct action           Set PendingCommand
           (e.g., prev profile)    (opens dialog; result
                                    processed next frame)
```

**Key Design Properties**:

- `ShortcutManager` does not block; consumed keys are removed from egui's input queue in the same frame
- `SHORTCUT_MANAGER` is a `lazy_static` singleton — no per-frame allocation
- Scopes enable context-sensitive shortcuts without global state mutation

**Source**: `crates/egui-command/`, `crates/egui-command-binding/`, `src/shortcut/`, `src/core/ui/mod.rs`

---

## Data Flow

### Video Streaming Path

```text
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

```text
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
[general]
keyboard_enabled = true
mouse_enabled = true
init_timeout = 15
window_width = 1280
window_height = 720
smart_window_resize = true
bind_address = "127.0.0.1"
scrcpy_server = "scrcpy-server-v3.3.3"

[scrcpy.video]
bit_rate = "8M"
max_fps = 60
max_size = 1920
codec = "h264"

[scrcpy.audio]
enabled = true
codec = "opus"
source = "playback"
buffer_frames = 64
ring_capacity = 5760

[scrcpy.options]
stay_awake = true
turn_screen_off = false

[gpu]
backend = "VULKAN"
vsync = false
hwdecode = true

[input]
long_press_ms = 300
drag_threshold_px = 5.0
drag_interval_ms = 8
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

| Dependency      | Version | Purpose                           |
| --------------- | ------- | --------------------------------- |
| eframe          | 0.33    | UI framework (egui + wgpu)        |
| egui            | 0.33    | Immediate mode GUI                |
| wgpu            | 27      | GPU abstraction                   |
| tokio           | 1.x     | Async runtime for network I/O     |
| ffmpeg-next     | 8       | Media decoding (FFmpeg bindings)  |
| cpal            | 0.17    | Audio playback                    |
| opus            | 0.3     | Opus codec (direct libopus)       |
| fluent-bundle   | 0.16    | i18n localization (Fluent)        |
| tracing         | 0.1     | Structured logging                |
| serde           | 1.0     | Serialization/deserialization     |
| toml            | 1       | Configuration file format         |

### Build Dependencies

| Dependency                      | Purpose                 |
| ------------------------------- | ----------------------- |
| cargo                           | Build system            |
| clang                           | C library binding       |
| pkg-config                      | Library detection       |
| FFmpeg development headers      | Media codec support     |
| VAAPI/NVDEC development headers | Hardware decode support |

---

## Decoder Selection Strategy

SAide uses a **cascade fallback** approach for video decoder selection, eliminating dependency on GPU detection.

### Cascade Fallback Algorithm

**Linux**:
1. Try NVDEC (NVIDIA hardware decoder)
2. Fallback to VAAPI (Intel/AMD hardware decoder)
3. Fallback to Software H.264 decoder

**Windows**:
1. Try NVDEC (NVIDIA hardware decoder)
2. Fallback to D3D11VA (DirectX 11 hardware decoder - Intel/AMD/NVIDIA)
3. Fallback to Software H.264 decoder

**Key Benefits**:
- **Multi-GPU Support**: Works with integrated + discrete GPU setups (e.g., Intel iGPU + NVIDIA dGPU)
- **Self-Detection**: FFmpeg decoders internally validate hardware availability
- **Robust**: Works on unknown/misconfigured GPU systems
- **Platform Agnostic**: Same strategy across Linux/Windows (only decoder order differs)

### Codec Profile Testing

Device codec compatibility is tested using cascade approach:

1. Test NVDEC profile (`profile=65536`)
2. Fallback to Baseline profile (`profile=66`)
3. Use first profile that succeeds

Results are cached in `device_profiles.toml` to avoid repeated testing.

**Source**: `src/decoder/auto.rs`, `src/scrcpy/codec_probe.rs`

---

## Platform Considerations

### Linux (Primary)

- **Video**: NVDEC (NVIDIA), VAAPI (Intel/AMD), Software fallback
- **Audio**: PulseAudio, ALSA via cpal
- **Display**: X11 and Wayland support via winit
- **ADB**: Standard Android SDK tools

### Windows (Experimental - v0.3)

- **Video**: NVDEC (NVIDIA), D3D11VA (Intel/AMD/NVIDIA), Software fallback
- **Audio**: WASAPI via cpal
- **Display**: DirectComposition
- **ADB**: Windows SDK

### macOS (Planned)

- **Video**: VideoToolbox, Metal
- **Audio**: CoreAudio via cpal
- **Display**: CAMetalLayer
- **ADB**: iOS not supported (Android only)

---

## Performance Characteristics

### Latency Breakdown

| Component      | Typical Latency | Optimization                       |
| -------------- | --------------- | ---------------------------------- |
| Android Encode | 15-35ms         | Hardware encoder, Baseline profile |
| Network (USB)  | 1-3ms           | TCP_NODELAY                        |
| PC Decode      | 5-15ms          | Hardware decode (VAAPI/NVDEC)      |
| Render         | 5-10ms          | NV12 zero-copy                     |
| **Total**      | **30-60ms**     | Near scrcpy performance            |

### Resource Usage

| Resource | Typical Usage           | Notes                         |
| -------- | ----------------------- | ----------------------------- |
| CPU      | 5-15% (hardware decode) | Higher with software decode   |
| GPU      | 5-10% (VAAPI/NVDEC)     | Video decode and rendering    |
| Memory   | 50-100MB                | Frame buffers, codec contexts |
| Network  | 2-10 Mbps               | Depends on bit_rate setting   |

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

```text
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
