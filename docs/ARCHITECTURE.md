# SAide Architecture

This document describes the current code structure of SAide as it exists in this repository.

## Runtime overview

SAide is a desktop scrcpy companion built around three pieces:

1. `src/main.rs` starts the application, loads configuration (falling back to defaults on error), verifies `adb`, initializes logging, and launches the egui/eframe desktop window with the WGPU renderer.
2. `src/core/` owns application lifecycle, UI state, device selection, stream startup, and the player/editor experience.
3. `3rd-party/scrcpy-rs`, `src/controller/`, `src/decoder/`, and `src/avsync/` implement reusable device communication, input injection, media decoding, and playback timing.

At runtime the app:

1. Loads config through `ConfigManager::new()`. If loading fails, a default config is used for the session and the user is notified via toast on the first UI frame.
2. Verifies `adb` is available. On failure, the error is captured as a `startup_error`.
3. Attempts to obtain the connected device serial. On failure, the error is captured as a `startup_error` (if none already set).
4. Launches the desktop UI unconditionally, passing any `startup_error` to `SAideApp`.
5. If `startup_error` is set, `SAideApp` enters `InitState::Failed` immediately and shows a blocking error dialog on the first frame; the user confirms and the app exits cleanly.
6. Otherwise, connects to the selected Android device, establishes scrcpy video / audio / control sockets, decodes media, renders video in egui, and forwards keyboard or mouse actions over the control channel.

## Module layout

```text
.
├── Cargo.toml              Workspace root (`saide` package + workspace members)
├── src/
│   ├── main.rs             Application entry point
│   ├── lib.rs              Public module exports for the desktop app crate
│   ├── constant.rs         Version strings, default paths, packet size guards
│   ├── error.rs            Top-level error types
│   ├── core/               App lifecycle and UI orchestration
│   │   ├── mod.rs          Re-exports `ui::{AppShell, SAideApp}`
│   │   ├── connection.rs   Connection service integration for the UI layer
│   │   ├── coords/         Mapping/view/scrcpy coordinate systems
│   │   ├── device_monitor.rs Startup-time and runtime device monitoring
│   │   ├── init.rs         Startup orchestration
│   │   ├── profile_manager.rs Profile CRUD and active-profile selection
│   │   ├── state.rs        Shared app/config/runtime state
│   │   ├── utils.rs        Nearest-mapping lookup and position extraction helpers
│   │   └── ui/             AppShell, player, dialogs, toolbar, editor
│   ├── config/             TOML-backed configuration structures and validation
│   ├── controller/         Input translation and control sending
│   ├── decoder/            Video and audio decode implementations
│   ├── avsync/             Audio/video timing coordination
│   ├── capture/            Screenshot and screen recording
│   ├── profiler/           Latency breakdown and rolling stats
│   ├── i18n/               Locale loading and source management
│   ├── shortcut/           Shortcut helpers backed by egui-command-binding
│   ├── modal/              UI modal primitives
│   ├── gpu/                GPU-type hints used by some optimizations
│   └── scrcpy_support.rs   Adapters around the reusable `scrcpy-rs` crate
├── 3rd-party/
│   ├── scrcpy-rs/          Reusable scrcpy protocol/runtime workspace crate
│   ├── egui-event/         Typed event bus used by AppShell/Dispatcher
│   └── egui-event-macros/  `#[derive(Event)]` / `#[derive(EventSet)]` proc macros
└── docs/
    ├── ARCHITECTURE.md
    └── configuration.md
```

## Startup and configuration flow

The application startup path is anchored in `src/main.rs` and `src/config/mod.rs`.

### Configuration loading

`ConfigManager::new()` uses this order:

1. Standard platform config path returned by `constant::config_dir()` (falls back to the system temp directory if the platform config directory cannot be resolved).
2. `./config.toml` in the current working directory if the standard file does not exist.
3. If neither exists, create a default config at the standard path.

### scrcpy server path resolution

`constant::resolve_scrcpy_server_path()` looks for `scrcpy-server-v3.3.3` in this order:

1. The application data directory.
2. The current working directory.
3. The legacy repository path `3rd-party/`.
4. If none exists, it still returns the current-directory candidate path string.

### Window and renderer setup

`src/main.rs` launches eframe with `eframe::Renderer::Wgpu`. The selected backend comes from `config.gpu.backend`, which currently supports only:

- `VULKAN`
- `OPENGL`

`config.gpu.vsync` controls whether the app requests `AutoVsync` or `AutoNoVsync` present mode.

Theme selection is applied by `src/core/ui/theme.rs`, and the optional `SAIDE_THEME` override accepts only `dark`, `light`, or `auto`.

## Core runtime responsibilities

### `src/core/`

This layer turns low-level protocol and decoding code into an interactive desktop app.

- `ui/` owns the visible application state, player, dialogs, and mapping editor.
- `device_monitor.rs` refreshes available ADB devices.
- `connection.rs` bridges the UI and the scrcpy connection lifecycle.
- `coords/` keeps mapping coordinates stable across resolution and rotation changes.

The root exports are `AppShell` and `SAideApp`, re-exported from both `src/core/mod.rs` and `src/lib.rs`.

### AppShell + Dispatcher frame loop

Phase 5 introduced an outer `AppShell` wrapper in `src/core/ui/shell.rs`.
`AppShell` implements `eframe::App`, while `SAideApp` now owns stateful UI logic
through `draw()` and `on_app_command()`.

The frame loop order is fixed:

1. `event_registry.update()` — rotate the double buffer so the previous frame's commands become readable
2. `dispatcher.dispatch(&mut state, &event_registry)` — apply buffered `AppCommand` events to `SAideApp`
3. `state.draw(ctx, frame, &mut event_registry)` — build UI and enqueue new commands for the next frame

This introduces an intentional one-frame delay for toolbar and shortcut commands,
which keeps the event flow borrow-checker friendly while remaining imperceptible
at normal frame rates.

### `Event` trait constraints

The `egui-event::Event` marker trait has a single bound: `Send + Sync + 'static`.
It does **not** require `Clone` or `Copy`.

Implications for this codebase:

- `CaptureEvent` and `DeviceMonitorEvent` satisfy the bound and use `#[derive(Event)]`.
- `InitEvent` contains `TcpStream` and `crossbeam_channel::Receiver<DeviceMonitorEvent>`,
  neither of which implement `Clone`. `InitEvent` therefore **cannot** derive `Event`
  and remains a plain Rust enum transferred exclusively over its dedicated crossbeam channel.
- The `macros` feature of the `egui-event` path dependency re-exports `egui-event-macros`,
  making `#[derive(Event)]` and `#[derive(EventSet)]` available without a separate
  `egui-event-macros` dependency entry in `Cargo.toml`.

### Commands and shortcuts

SAide uses `egui-command` v0.2 style command metadata. `AppCommand` is the shared
command enum for toolbar actions, editor actions, and global shortcuts. The
workspace also vendors `egui-command-binding`, whose `ShortcutManager` can fill
display-only shortcut hints into an `egui-command::CommandRegistry`.

### Coordinate systems

The coordinate modules separate three concerns:

- mapping-space coordinates stored in profiles (`0.0..=1.0` style positions)
- scrcpy/device-space coordinates used by the protocol
- visual/UI-space coordinates used by rendering and editor interaction

`capture_orientation` and display-side `video_rotation` use the same clockwise quarter-turn convention (`0/1/2/3 = 0°/90°/180°/270°`). `display_rotation` is the Android `Display.getRotation()` / `Surface.ROTATION_*` value, i.e. the screen-content rotation reported by Android rather than a direct physical turn direction. In locked-capture mode, `video_rotation` compensates the combined frame rotation (`capture_orientation + display_rotation`, modulo 4), while mapping/editor coordinates use the inverse transform so input stays aligned with the corrected on-screen image.

That separation is what allows profile mappings to survive device rotation and different stream resolutions.

## scrcpy connection architecture

`3rd-party/scrcpy-rs/src/connection.rs` implements the reusable handshake.

### Connection order

When a session starts, SAide:

1. Pushes the server jar to the device.
2. Sets up an ADB reverse tunnel.
3. Starts the scrcpy server process.
4. Accepts sockets in this order:
   - video
   - audio (if enabled)
   - control

If `send_device_meta` is enabled, the device name is read from the video stream. If `send_codec_meta` is enabled, SAide reads video codec metadata from the video stream and audio codec metadata from the audio stream.

### Audio availability

Before enabling audio, SAide checks the Android API level through ADB. If the device is older than Android 11 / API 30, audio is disabled automatically and the reason is stored as `AudioDisabledReason::UnsupportedAndroidVersion`.

### Socket tuning

The connection layer enables `TCP_NODELAY` on the scrcpy sockets. On Linux it also attempts `TCP_QUICKACK`. Read timeouts differ by channel:

- control channel: 2 seconds
- video/audio channels: 5 seconds

These settings exist to reduce latency while still surfacing disconnects promptly.

## Protocol coverage in this repository

The protocol code lives under `3rd-party/scrcpy-rs/src/protocol/`.

- `control.rs` serializes the control messages that SAide actually sends.
- `video.rs` parses the frame metadata and payload wrapper used by the video stream.
- `audio.rs` parses the audio packet wrapper used by the current audio path.

The repository does **not** implement the full scrcpy feature surface. For the exact coverage and wire-format notes, see [PROTOCOL.md](../3rd-party/scrcpy-rs/PROTOCOL.md).

## Decoder pipeline

The decoder layer is split by backend.

### Video

- `decoder/auto.rs` chooses and falls back between decoder implementations.
- Linux paths include NVDEC and VAAPI, then software H.264 fallback.
- Windows includes D3D11VA and NVDEC support in the codebase, with software fallback when hardware decoding is unavailable.

### Audio

- `decoder/audio/opus.rs` handles the current Opus decode path.
- `decoder/audio/player.rs` feeds decoded audio into CPAL.

### Rendering

SAide uses egui + WGPU for presentation. The player code renders decoded frames into the UI rather than running a separate native video window.

## Mapping system

The mapping configuration lives in `src/config/mapping/`.

### Data model

- `MappingsConfig` stores the global toggle key, initial enabled state, notification preference, and all profiles.
- Each `Profile` is bound to a `device_serial` and a `rotation`.
- `KeyMapping` serializes as a list of mapping items in TOML.
- `MappingAction` is tagged by `action` and supports tap, swipe, drag-related touch events, scroll, Android key events, text, and a few special actions such as back/home/menu/power.

### Why profiles are rotation-aware

Profiles match on both device serial and rotation. That lets SAide keep separate mappings for portrait and landscape layouts without guessing how to transform every game HUD.

## Latency and synchronization

Two modules matter here:

- `src/avsync/clock.rs` handles A/V timing coordination.
- `src/profiler/latency.rs` tracks capture, receive, decode, upload, and display timestamps so the UI can report latency breakdowns.

The profiler is code-backed and self-contained; no extra design document is required to understand its stages.

## Examples and release automation

### Examples

The repository includes runnable examples under `examples/`:

- `test_connection.rs`
- `test_audio.rs`
- `audio_diagnostic.rs`
- `render_avsync.rs`
- `probe_codec.rs`
- `test_protocol.rs`
- `test_auto_decoder.rs`
- `test_audio_native.rs`
- `test_i18n.rs`
- `test_planar_interleave.rs`
- `test_vulkan_import.rs`

### Release

The CI workflow (`.github/workflows/release.yml`) triggers on `v*` tags and `workflow_dispatch`, and publishes `windows-x64` and `linux-glibc-x64` artifacts to GitHub Releases.

## Related documents

- [configuration.md](configuration.md): config file structure, defaults, and validation ranges
- [3rd-party/scrcpy-rs/PROTOCOL.md](../3rd-party/scrcpy-rs/PROTOCOL.md): wire-format details and current protocol coverage
