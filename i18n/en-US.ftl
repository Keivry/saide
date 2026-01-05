## Application
app-title = SAide
app-starting = SAide starting...
app-shutdown = Shutdown signal received, closing application

## Configuration
config-video-backend = Video backend: {$backend}
config-max-video-size = Max video size: {$size}
config-max-fps = Max FPS: {$fps}
config-logging-level = Logging level: {$level}

## Device
device-serial = Device: {$serial}
device-offline = Device went offline - USB/ADB connection lost
device-orientation-changed = Device rotated to orientation: {$orientation}
device-ime-state-changed = Device IME state changed: {$state}

## Initialization
init-completed = Initialization completed successfully
init-error = Initialization error: {$error}
init-device-orientation = Initial device orientation: {$orientation}
init-video-rotation = Initial video rotation: {$rotation}

## Connection
connection-ready = ScrcpyConnection ready: {$width}x{$height}, device: {$serial} ({$name}), capture_orientation: {$orientation}
connection-cleanup = SAideApp dropping, cleaning up connection
connection-shutdown-failed = Failed to shutdown connection: {$error}
connection-cleanup-completed = SAideApp cleanup completed

## Video
video-resolution = Video resolution: {$width}x{$height}
video-rotated = Video rotated to {$rotation}
video-dimensions-changed = Video dimensions changed: {$old_width}x{$old_height} -> {$new_width}x{$new_height}

## Screen
screen-off-init = Screen turned off as per config
screen-off-init-failed = Failed to turn screen off on init: {$error}
screen-off-toolbar = Turning off screen from toolbar
screen-off-success = Screen OFF (press physical power button to wake up)
screen-off-failed = Failed to turn off screen: {$error}

## Mappings
mapping-add = Adding mapping: {$key} -> ({$x}, {$y})
mapping-add-screen = Add mapping: screen=({$screen_x},{$screen_y}) -> percent=({$percent_x},{$percent_y}) [device_orientation={$orientation}]
mapping-delete = Deleting mapping: {$key}
mapping-delete-screen = Delete mapping: {$key} at ({$x}, {$y})
mapping-saved = Mapping saved successfully
mapping-deleted = Mapping deleted successfully
mapping-save-failed = Failed to save config: {$error}
mapping-keyboard-not-init = Keyboard mapper not initialized
mapping-profiles-refreshed = Keyboard profiles refreshed: active={$active}, available={$available}
mapping-profile-set = Active profile set to: {$profile}
mapping-profile-disabled = Disable custom key mappings for this device/rotation.

## Input Events
input-skip-not-init = Skipping input processing - not initialized
input-keyboard-event = Processing keyboard event: key={$key}, modifiers={$modifiers}
input-keyboard-event-failed = Failed to handle keyboard event: {$error}
input-text-event-failed = Failed to handle text input event: {$error}
input-mouse-button = Processing mouse button event: {$button} at {$pos}
input-mouse-button-failed = Failed to handle mouse button event: {$error}
input-mouse-move-failed = Failed to handle mouse move event: {$error}
input-mouse-release-failed = Failed to handle mouse button release event: {$error}
input-mouse-wheel = Processing mouse wheel event: {$delta} at {$pos}
input-mouse-wheel-failed = Failed to handle wheel event: {$error}
input-mouse-wheel-success = Mouse wheel event at scrcpy video coords: ({$x}, {$y})
input-mouse-mapper-update-failed = Failed to update mouse mapper: {$error}
input-coords-convert-failed = Failed to convert screen coords to video coords
input-coords-converted = Converted screen ({$screen_x}, {$screen_y}) -> scrcpy video ({$scrcpy_x}, {$scrcpy_y})

## Device Monitor
monitor-skip-not-init = Skipping device monitor processing - not initialized
monitor-keyboard-unavailable = Keyboard mapper not available for profile refresh

## UI - Toolbar
toolbar-rotate = Rotate Video
toolbar-configure = Configure Mappings
toolbar-keyboard-mapping = Toggle Keyboard Mapping
toolbar-screen-off = Turn Off Screen
toolbar-screen-off-hint = (Press physical power button to wake up)

## UI - Audio Warning
audio-warning-title = Audio Not Available
audio-warning-close = ✖

## UI - Indicator
indicator-fps = FPS: {$fps}
indicator-latency = Latency: {$ms}ms
indicator-frames = Frames: {$total}
indicator-dropped = Dropped: {$dropped}
indicator-profile = Profile: {$profile}
indicator-orientation = Orientation: {$orientation}°
indicator-resolution = Resolution: {$width}x{$height}

## Background Tasks
background-task-cancel = SAideApp exiting, cancelling background tasks

## Stream
stream-stop = Stopping stream
stream-ready = Stream ready: {$width}x{$height}
stream-resolution-changed = Resolution changed: {$width}x{$height}
stream-failed = Stream failed: {$error}
stream-worker-cancel = Stream worker exiting due to cancellation
stream-worker-error = Stream worker error: {$error}
stream-worker-send-failed = Failed to send PlayerEvent::Failed: {$error}

## Audio
audio-thread-start = Audio thread spawned, entering read loop...
audio-thread-started = Audio thread started (Opus)
audio-thread-header = Audio thread: attempting to read header...
audio-thread-header-success = Audio thread: header read successful
audio-packets-processed = Audio: {$count} packets processed
audio-playback-error = Audio playback error: {$error}
audio-decode-error = Audio decode error: {$error}
audio-thread-error = Audio thread error: {$error}
audio-thread-cancel = Audio thread exiting due to cancellation
audio-thread-terminated = Audio thread terminated: {$error}

## Video Decode
video-decode-start = Starting video decode loop...
video-decode-cancel = Video decode loop exiting due to cancellation

## Ctrl-C Handler
ctrlc-received = Received Ctrl-C, shutting down...
ctrlc-handler-failed = Failed to set Ctrl-C handler: {$error}

## Platform Detection
platform-nvidia-proc = Detected NVIDIA driver via /proc
platform-nvidia-smi = Detected NVIDIA GPU via nvidia-smi
platform-nvidia-drm = Detected NVIDIA GPU via DRM device
platform-gpu-vendor = Found GPU vendor: 0x{$vendor} at {$path}
platform-intel = Detected Intel GPU
platform-amd = Detected AMD GPU

## Codec Probe
codec-probe-start = 🔍 Probing codec compatibility for device: {$serial}
codec-hw-encoder = Detected hardware encoder: {$encoder}
codec-default-encoder = Using system default encoder
codec-testing = Testing {$count} codec options...
codec-supported = ✅ Supported
codec-not-supported = ❌ Not supported
codec-validating = 🔄 Validating combined configuration...
codec-testing-config = Testing: {$config}
codec-combined-works = ✅ Combined config works!
codec-combined-failed = ❌ Combined config failed, falling back to None
codec-final-config = Final config: {$config}
codec-no-options = No options supported, using defaults
codec-test-options = Testing: video_codec_options={$options}
codec-connection-failed = Connection failed: {$error}
codec-packet-read-success = ✅ Successfully read video packet
codec-packet-read-failed = Failed to read packet: {$error}
codec-profiles-saved = Saved device profiles to {$path}
codec-skip-latency = Skipping 'latency' (requires Android 11+)
codec-skip-bframes = Skipping 'max-bframes' (requires Android 13+)
