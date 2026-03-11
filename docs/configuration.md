# SAide Configuration Guide

SAide uses a TOML configuration file to customize behavior. The configuration file is loaded from:

1. **Platform default**:
   - Linux: `~/.config/saide/config.toml`
   - macOS: `~/Library/Application Support/saide/config.toml`
   - Windows: `%APPDATA%\saide\config.toml`
2. **Fallback**: `./config.toml` (current directory)

## Configuration Sections

### [general] - General Settings

```toml
[general]
keyboard_enabled = true      # Enable keyboard input mapping
mouse_enabled = true         # Enable mouse input handling
auto_hide_toolbar = false    # Use a floating edge-reveal toolbar instead of a docked sidebar
init_timeout = 15            # Connection initialization timeout (seconds)
indicator = true             # Show on-screen indicator
indicator_position = "bottom-left"  # Indicator position: top-left/top-right/bottom-left/bottom-right
window_width = 1280          # Default window width (pixels)
window_height = 720          # Default window height (pixels)
smart_window_resize = true   # Scale oversized video windows down to fit the screen
bind_address = "127.0.0.1"   # Network bind address for scrcpy server
scrcpy_server = "scrcpy-server-v3.3.3"  # Override local scrcpy-server path if needed
```

### [scrcpy.video] - Video Stream Settings

```toml
[scrcpy.video]
bit_rate = "8M"              # Video bitrate (e.g., "8M" = 8 Mbps)
min_fps = 5                  # Idle refresh rate when no new frame/input arrives
max_fps = 60                 # Maximum frame rate (fps)
max_size = 1920              # Maximum resolution (pixels, longer dimension)
codec = "h264"               # Video codec: h264 (default)
# encoder = "OMX.qcom.video.encoder.avc"  # Optional: specify hardware encoder
# capture_orientation = 0    # Lock orientation: 0=portrait, 1=landscape, 2=portrait180, 3=landscape180
```

### [scrcpy.audio] - Audio Stream Settings

```toml
[scrcpy.audio]
enabled = true               # Enable audio streaming (requires Android 11+)
codec = "opus"               # Audio codec: opus (default)
source = "playback"          # Audio source: playback (device audio output)

# Latency tuning (advanced)
buffer_frames = 64           # CPAL buffer size in frames
                             # Lower = less latency (64 ≈ 1.3ms @ 48kHz)
                             # Higher = fewer glitches (128 ≈ 2.7ms, 256 ≈ 5.3ms)
                             # Range: 32-16384

ring_capacity = 5760         # Internal ring buffer capacity (samples)
                             # Higher = more buffering, fewer glitches
                             # Lower = less latency
                             # Range: 1024-65536
```

### [scrcpy.options] - Scrcpy Behavior

```toml
[scrcpy.options]
turn_screen_off = true       # Turn off device screen during mirroring
stay_awake = true            # Prevent device from sleeping
```

### [gpu] - GPU Rendering

```toml
[gpu]
vsync = false                # Enable VSync (disable for lowest latency)
backend = "VULKAN"           # GPU backend: VULKAN (default) or OPENGL
hwdecode = true              # Enable hardware decode auto-detection/fallback
```

If `general.scrcpy_server` is not overridden, SAide looks for `scrcpy-server-v3.3.3` in the application data directory first, then the current working directory, and finally the legacy `3rd-party/` path used by older example setups.

### [input] - Input Control Settings

```toml
[input]
long_press_ms = 300          # Long press duration (milliseconds)
                             # Time to hold mouse button before triggering long-press
                             # Range: 50-2000ms

drag_threshold_px = 5.0      # Drag detection threshold (pixels)
                             # Minimum mouse movement to distinguish drag from click
                             # Range: 1.0-50.0px

drag_interval_ms = 8         # Drag update interval (milliseconds)
                             # Interval for sending touch move events during dragging
                             # Lower = smoother drag (8ms ≈ 120fps)
                             # Range: 1-100ms
```

### [mappings] - Keyboard Mapping

```toml
[mappings]
toggle = "F10"               # Hotkey to toggle keyboard mappings on/off
initial_state = true         # Enable mappings on startup
show_notification = true     # Show notification when toggling mappings

[[mappings.profiles]]
name = "Profile Name"        # Profile name for this device
device_serial = "ABC123"     # Device serial number (from `adb devices`)
rotation = 0                 # Screen rotation: 0-3 (0=portrait, 1=landscape)

[[mappings.profiles.mappings]]
key = "W"                    # Keyboard key to map
action = "Tap"               # Action type: Tap, Swipe, etc.
x = 0.5                      # X coordinate (0.0-1.0, normalized to screen width)
y = 0.3                      # Y coordinate (0.0-1.0, normalized to screen height)
```

### [logging] - Logging Configuration

```toml
[logging]
level = "info"               # Log level: error/warn/info/debug/trace
```

## Configuration Validation

SAide validates configuration values on load. Invalid values will produce error messages:

### Input Control Ranges

| Parameter | Valid Range | Default | Description |
|-----------|-------------|---------|-------------|
| `long_press_ms` | 50-2000 | 300 | Too low = accidental long-press; too high = sluggish |
| `drag_threshold_px` | 1.0-50.0 | 5.0 | Too low = accidental drag; too high = hard to trigger |
| `drag_interval_ms` | 1-100 | 8 | Too low = CPU overhead; too high = choppy drag |

### Audio Ranges

| Parameter | Valid Range | Default | Description |
|-----------|-------------|---------|-------------|
| `buffer_frames` | 32-16384 | 64 | Lower = less latency; higher = fewer glitches |
| `ring_capacity` | 1024-65536 | 5760 | Must be >= 3× buffer_frames for stability |

### Video Ranges

| Parameter | Valid Range | Default | Description |
|-----------|-------------|---------|-------------|
| `min_fps` | 1-240 and `<= max_fps` | 5 | Idle UI refresh rate when no new video frame or input activity exists |
| `max_fps` | 1-240 | 60 | Active refresh rate while streaming new frames or handling input |

## Example: Low-Latency Configuration

For competitive gaming or real-time control:

```toml
[scrcpy.video]
bit_rate = "12M"
min_fps = 5
max_fps = 120
max_size = 1920

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

## Example: Stable/High-Quality Configuration

For video recording or unstable network:

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

## Troubleshooting

### Audio Glitches/Dropouts

**Symptoms**: Crackling, popping, or audio cuts out intermittently

**Solutions**:
1. Increase `buffer_frames` (64 → 128 → 256)
2. Increase `ring_capacity` (5760 → 11520)
3. Check USB cable quality
4. Close other audio applications

### Mouse Feels Sluggish

**Symptoms**: Long-press triggers too easily, dragging feels delayed

**Solutions**:
1. Decrease `long_press_ms` (300 → 200)
2. Decrease `drag_threshold_px` (5.0 → 3.0)
3. Decrease `drag_interval_ms` (8 → 4)

### Accidental Long-Press/Drag

**Symptoms**: Clicks sometimes register as long-press or drag

**Solutions**:
1. Increase `long_press_ms` (300 → 500)
2. Increase `drag_threshold_px` (5.0 → 10.0)

### High CPU Usage During Drag

**Symptoms**: CPU spikes when dragging mouse

**Solutions**:
1. Increase `drag_interval_ms` (8 → 16 or 32)
2. Reduce frame rate (`max_fps = 30`)

## Configuration File Location

To find your active configuration file path:

```bash
# Linux/macOS
ls ~/.config/saide/config.toml

# Windows
dir %APPDATA%\saide\config.toml
```

If the file doesn't exist, SAide will create it with default values on first run.
