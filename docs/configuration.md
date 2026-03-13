# SAide Configuration Guide

SAide stores its runtime settings in a TOML file.

## Config file discovery order

Configuration is loaded in this order:

1. the standard platform config path (e.g. `~/.config/saide/config.toml` on Linux)
2. `./config.toml` in the current working directory if the standard file does not exist
3. if neither exists, write a default config to the standard path
4. if the platform config directory cannot be determined, fall back to the system temp directory

If you want to inspect the repository's sample file, see [`config.toml`](../config.toml).

## Sections and defaults

### `[general]`

```toml
[general]
keyboard_enabled = true
mouse_enabled = true
auto_hide_toolbar = false
init_timeout = 15
indicator = true
indicator_position = "top-left"
window_width = 1280
window_height = 720
smart_window_resize = true
bind_address = "127.0.0.1"
scrcpy_server = "<resolved path to scrcpy-server-v3.3.3>"
```

- `keyboard_enabled`: enable keyboard mapping logic
- `mouse_enabled`: enable mouse-to-touch / pointer handling
- `auto_hide_toolbar`: when `false`, the docked toolbar width is included in the initial window sizing path
- `init_timeout`: initialization timeout in seconds
- `indicator`: show the on-screen status indicator
- `indicator_position`: `top-left`, `top-right`, `bottom-left`, or `bottom-right`
- `smart_window_resize`: scale oversized content down using built-in resolution tiers
- `bind_address`: host bind address for the reverse-tunneled scrcpy connection, such as `127.0.0.1` or `[::1]`
- `scrcpy_server`: local path to the scrcpy server binary; if not set, SAide searches the app data directory, current working directory, and the legacy `3rd-party/` directory in that order

### scrcpy server lookup order

If `general.scrcpy_server` is not overridden, SAide searches for `scrcpy-server-v3.3.3` in this order:

1. application data directory
2. current working directory
3. legacy `3rd-party/` directory

### `[scrcpy.video]`

```toml
[scrcpy.video]
bit_rate = "8M"
min_fps = 5
max_fps = 60
max_size = 1920
codec = "h264"
# encoder = "OMX.qcom.video.encoder.avc"
# capture_orientation = 0
```

- `bit_rate`: scrcpy video bitrate string
- `min_fps`: idle refresh rate floor used by the current runtime
- `max_fps`: maximum streaming frame rate
- `max_size`: maximum long edge in pixels
- `codec`: default video codec string, currently `h264`
- `encoder`: optional explicit Android encoder name
- `capture_orientation`: optional lock value `0..=3`; if omitted, the stream follows device rotation

### `[scrcpy.audio]`

```toml
[scrcpy.audio]
enabled = true
codec = "opus"
source = "playback"
buffer_frames = 64
ring_capacity = 5760
```

- `enabled`: request audio capture
- `codec`: current default is `opus`
- `source`: current default is `playback`
- `buffer_frames`: output audio buffer size in frames
- `ring_capacity`: decoded audio ring buffer capacity in frames

Audio capture requires Android 11 / API 30 or newer. On older devices SAide automatically disables audio at connection time.

### `[scrcpy.options]`

```toml
[scrcpy.options]
turn_screen_off = true
stay_awake = true
```

- `turn_screen_off`: request that the device screen be turned off while mirroring
- `stay_awake`: request that the device stay awake during the session

### `[gpu]`

```toml
[gpu]
vsync = false
backend = "VULKAN"
hwdecode = true
```

- `vsync`: controls whether SAide requests a vsynced present mode
- `backend`: currently only `VULKAN` or `OPENGL`
- `hwdecode`: enable hardware decode attempts with fallback when possible

### `[input]`

```toml
[input]
long_press_ms = 300
drag_threshold_px = 5.0
drag_interval_ms = 8
```

- `long_press_ms`: long-press threshold in milliseconds
- `drag_threshold_px`: minimum pointer movement before a drag is recognized
- `drag_interval_ms`: interval for sending drag updates

### `[mappings]`

```toml
[mappings]
toggle = "F10"
initial_state = false
show_notification = true

[[mappings.profiles]]
name = "Portrait profile"
device_serial = "ABC123"
rotation = 0

[[mappings.profiles.mappings]]
key = "W"
action = "Tap"
x = 0.5
y = 0.3
```

This structure supports profile-based key/mouse mapping:

- the global toggle key default is `F10`
- each profile matches on `device_serial` and `rotation`
- each mapping item stores a host key plus a tagged action payload

Supported action tags currently include:

- `Tap`
- `Swipe`
- `TouchDown`
- `TouchMove`
- `TouchUp`
- `Scroll`
- `Key`
- `KeyCombo`
- `Text`
- `Back`
- `Home`
- `Menu`
- `Power`
- `Ignore`

### `[logging]`

```toml
[logging]
level = "info"
```

## Validation ranges

### General

| Key | Range |
| --- | --- |
| `general.init_timeout` | `1..=300` |
| `general.window_width` | `320..=7680` |
| `general.window_height` | `240..=4320` |

### Video

| Key | Range |
| --- | --- |
| `scrcpy.video.min_fps` | `1..=240` |
| `scrcpy.video.max_fps` | `1..=240` |
| `scrcpy.video.min_fps <= max_fps` | required |
| `scrcpy.video.max_size` | `100..=4096` |

### Input

| Key | Range |
| --- | --- |
| `input.long_press_ms` | `50..=2000` |
| `input.drag_threshold_px` | `1.0..=50.0` |
| `input.drag_interval_ms` | `1..=100` |

### Audio

| Key | Range |
| --- | --- |
| `scrcpy.audio.buffer_frames` | `32..=16384` |
| `scrcpy.audio.ring_capacity` | `1024..=65536` |

## Example profiles

### Low-latency leaning setup

```toml
[scrcpy.video]
bit_rate = "12M"
min_fps = 5
max_fps = 120
max_size = 1600

[scrcpy.audio]
enabled = true
buffer_frames = 64
ring_capacity = 3840

[gpu]
vsync = false

[input]
long_press_ms = 200
drag_threshold_px = 3.0
drag_interval_ms = 8
```

### Stability-first setup

```toml
[scrcpy.video]
bit_rate = "24M"
min_fps = 5
max_fps = 60
max_size = 2560

[scrcpy.audio]
enabled = true
buffer_frames = 256
ring_capacity = 11520

[gpu]
vsync = true

[input]
long_press_ms = 400
drag_threshold_px = 8.0
drag_interval_ms = 16
```

## Troubleshooting notes

### Audio crackling or dropouts

Try increasing:

- `scrcpy.audio.buffer_frames`
- `scrcpy.audio.ring_capacity`

### Input feels too sticky or too sensitive

Tune:

- `input.long_press_ms`
- `input.drag_threshold_px`
- `input.drag_interval_ms`

### High latency or excessive CPU usage

Common first adjustments:

- lower `scrcpy.video.max_fps`
- lower `scrcpy.video.max_size`
- disable `gpu.vsync`
- disable `gpu.hwdecode` only if the hardware path is unstable
