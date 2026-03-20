## Application
app-title = SAide

## UI - Toolbar
toolbar-toggle-keyboard-mapping = Toggle Keyboard Mapping
toolbar-mapping-visualization = Toggle Mapping Visualization
toolbar-editor = Mapping Editor
toolbar-rotate = Rotate Video
toolbar-screenshot = Take Screenshot
toolbar-recording = Toggle Recording
toolbar-screen-off = Screen Off
toolbar-pin-toolbar = Pin Toolbar
toolbar-float-toolbar = Float Toolbar
notification-recording-started = Recording started
notification-recording-stopped = Recording saved: {$path}
notification-screenshot-saved = Screenshot saved: {$path}
notification-capture-error = Capture error: {$error}

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

## UI - Indicator Panel
indicator-panel-resolution = Resolution:
indicator-panel-capture-orientation = Capture Orientation:
indicator-panel-video-rotation = Video Rotation:
indicator-panel-display-rotation = Display Rotation:
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
editor-dialog-new-profile-title = Create New Profile
editor-dialog-new-profile-placeholder = Profile name
editor-dialog-save-profile-as-title = Save Profile As
editor-dialog-save-profile-as-placeholder = New Profile Name
editor-dialog-switch-profile-title = Select Profile to Activate
editor-dialog-error-profile-exists-title = Profile Already Exists
editor-dialog-error-profile-exists-message = Profile "{$profile_name}" already exists.

## UI - Common Dialog Buttons
dialog-button-confirm = Confirm
dialog-button-cancel = Cancel

## UI - Help
help-title = Help
help-message =
    Available shortcuts:
    
    F1  - Show this help            
    F2  - Rename profile (*)        
    F3  - New profile (*)           
    F4  - Delete current profile (*)
    F5  - Save as (*)               
    F6  - Switch profile            
    F7  - Previous profile          
    F8  - Next profile              

    {"*"} - ONLY IN EDITOR MODE

## UI - Notifications
notification-no-active-profile = No active profile
notification-no-profiles = No profiles available
notification-create-profile-failed = Failed to create profile
notification-create-profile-failed-with-reason = Failed to create profile: { $reason }
notification-delete-profile-failed = Failed to delete profile
notification-rename-profile-failed = Failed to rename profile
notification-rename-profile-failed-with-reason = Failed to rename profile: { $reason }
notification-save-profile-as-failed = Failed to save profile as
notification-save-profile-as-failed-with-reason = Failed to save profile as: { $reason }
notification-switch-profile-failed = Failed to switch profile
notification-switch-profile-success = Switched to profile "{ $profile_name }"
notification-no-profile-to-switch = No other profile available to switch
notification-add-mapping-failed = Failed to add mapping
notification-delete-mapping-failed = Failed to delete mapping
notification-save-config-failed = Failed to save config
notification-profile-error-not-found = Profile not found
notification-profile-error-name-conflict = Profile name already exists
notification-profile-error-invalid-format = Profile name cannot be empty

# UI - Codec Compatibility Detection
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

## Startup errors
startup-fatal-error-title = SAide — Startup Error
notification-config-load-failed = Configuration failed to load, using defaults: { $error }
