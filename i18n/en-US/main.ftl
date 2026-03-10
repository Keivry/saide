## Application
app-title = SAide

## UI - Toolbar
toolbar-rotate = Rotate Video
toolbar-configure = Configure Mapping
toolbar-editor = Mapping Editor
toolbar-create-profile = Create Mapping Profile
toolbar-delete-profile = Delete Mapping Profile
toolbar-keyboard-mapping = Toggle Keyboard Mapping
toolbar-mapping-visualization = Show Mapping Visualization
toolbar-screen-off = Screen Off
toolbar-screen-off-hint = (Press physical power button to wake)

## UI - Audio Warning
audio-warning-title = Audio Unavailable
audio-warning-unsupported-android = Audio capture requires Android 11+ (API 30+). Device API level: {$api_level}.
audio-warning-close = ✖

## UI - Player
player-status-idle = No Device
player-status-connecting = Connecting...
player-status-loading = Loading...
player-error-title = ⚠️ Stream Error
player-error-message = An error occurred during streaming
player-error-restart = Please restart the application
player-error-details = Details: {$error}

## UI - Indicator
indicator-fps = FPS: {$fps}
indicator-latency = Latency: {$ms}ms
indicator-frames = Frames: {$total}
indicator-dropped = Dropped: {$dropped}
indicator-profile = Profile: {$profile}
indicator-orientation = Orientation: {$orientation}°
indicator-resolution = Resolution: {$width}x{$height}

## UI - Indicator Panel
indicator-panel-resolution = Resolution:
indicator-panel-capture-orientation = Capture Orientation:
indicator-panel-video-rotation = Video Rotation:
indicator-panel-device-rotation = Device Rotation:
indicator-panel-fps = FPS:
indicator-panel-frames = Frames (Dropped/Total):
indicator-panel-latency-avg = Latency (Avg):
indicator-panel-latency-p95 = Latency (P95):
indicator-panel-decode = Decode:
indicator-panel-gpu-upload = GPU Upload:
indicator-panel-profile = Profile:
indicator-panel-profile-none = N/A

## UI - Mapping Editor
editor-title = Mapping Editor
editor-profile-label = Profile:
editor-profile-none = No Profile
editor-instruction-add = Left click - Add mapping
editor-instruction-delete = Right click - Delete mapping
editor-instruction-help = F1 - Show help
editor-instruction-exit = Press ESC to exit

## UI - Mapping Editor Dialogs
editor-dialog-create-title = Add Mapping
editor-dialog-create-message =
    Position: ({$x}, {$y})
    
    Press any key or ESC to cancel...
editor-dialog-delete-title = Delete Mapping
editor-dialog-delete-message = {$key}: ({$x}, {$y})?
editor-dialog-delete-profile-title = Delete Profile
editor-dialog-delete-profile-message = Delete profile "{$name}"?
editor-dialog-rename-title = Rename Profile
editor-dialog-rename-placeholder = New Name
editor-dialog-new-title = Create New Profile
editor-dialog-new-placeholder = Profile name
editor-dialog-saveas-title = Save Profile As
editor-dialog-saveas-placeholder = New Profile Name
editor-dialog-switch-title = Select Profile to Activate
editor-dialog-error-profile-exists-title = Profile Already Exists
editor-dialog-error-profile-exists-message = Profile "{$profile_name}" already exists.
help-title = Help
help-message =
    Available shortcuts:
    
    F1  - Show this help        
    F2  - Rename profile        
    F3  - New profile           
    F4  - Delete current profile
    F5  - Save as               
    F6  - Switch profile        
    F7  - Previous profile      
    F8  - Next profile          

    ESC - Close editor          

## UI - Notifications
notification-no-active-profile = No active profile
notification-no-profiles = No profiles available
notification-create-profile-failed = Failed to create profile
notification-delete-profile-failed = Failed to delete profile
notification-rename-profile-failed = Failed to rename profile
notification-save-profile-as-failed = Failed to save profile as
notification-switch-profile-failed = Failed to switch profile
notification-add-mapping-failed = Failed to add mapping
notification-delete-mapping-failed = Failed to delete mapping
notification-save-config-failed = Failed to save config

## UI - Common Dialog Buttons
dialog-button-confirm = Confirm
dialog-button-cancel = Cancel

probe-codec-dialog-title = Detect Codec Compatibility
probe-codec-dialog-message = No codec profile found for device { $serial }. Run detection now? (~30-60s)
probe-codec-progress-title = Detecting Codec Compatibility
probe-codec-step-detecting-device = Reading device information...
probe-codec-step-detecting-encoder = Detecting hardware encoder...
probe-codec-step-testing-profile = Testing codec profile ({ $index }/{ $total }): { $name }
probe-codec-step-testing-option = Testing codec option ({ $index }/{ $total }): { $key }
probe-codec-step-validating = Validating combined configuration...
probe-codec-done-success = Detection complete
probe-codec-done-failed = Detection failed: { $error }
