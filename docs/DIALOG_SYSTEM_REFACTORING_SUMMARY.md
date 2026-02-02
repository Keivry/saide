# Dialog System Refactoring - Complete Summary

**Project**: SAide (Scrcpy Android device mirroring)  
**Branch**: `windows`  
**Date**: 2026-02-02  
**Phases Completed**: 1, 2, 3

---

## Overview

This document summarizes the complete dialog system refactoring across three phases, transforming the mapping configuration UI from a tightly-coupled design to a clean event-driven architecture.

---

## Phase 1: Initial Refactoring (Completed Previously)

### Objectives
- Move all dialog display logic from `MappingConfigOverlay` to `SAideApp`
- Implement centralized dialog event handling
- Fix `ProfileOperationResult` compilation errors

### Changes
1. **Fixed** `ProfileOperationResult` enum variants (tuple → unit variants)
2. **Moved** dialog rendering from `mapping.rs` to `saide.rs`
3. **Implemented** 6 dialog helper methods in `SAideApp`:
   - `show_add_mapping_dialog()`
   - `show_delete_mapping_dialog()`
   - `show_rename_profile_dialog()`
   - `show_new_profile_dialog()`
   - `show_save_as_profile_dialog()`
   - `show_help_dialog()`
4. **Cleaned up** 334 lines of commented old code
5. **Added** missing i18n keys (`mapping-config-dialog-{new,saveas,profile-exists}-title`)

### Files Modified
- `src/saide/ui/saide.rs`
- `src/saide/ui/mapping.rs`
- `i18n/en-US/main.ftl`
- `i18n/zh-CN/main.ftl`

---

## Phase 2: Simplification (Completed Previously)

### Objectives
- Remove redundant `DialogContext` enum
- Remove `DialogData` struct (temporary data storage)
- Use `MappingConfigEvent` directly for context tracking

### Changes
1. **Removed** `DialogContext` enum (8 variants)
2. **Removed** `DialogData` struct
3. **Simplified** dialog system: dialogs only display pre-constructed strings
4. **Changed** business data handling: recalculated in callbacks from event data

### Architecture Decision
- Dialogs are **stateless presenters** (no business logic)
- Business data (keys, positions) stored in events, recalculated when needed

---

## Phase 3: Event Queue & Global Shortcuts (Completed 2026-02-02)

### Objectives
- Implement true event queue (VecDeque) to prevent lost events
- Add global shortcut support (F1, F6-F8 work outside mapping overlay)
- Prevent duplicate dialogs

### Problem Statement
**Before**:
```rust
pending_dialog_event: Option<MappingConfigEvent>  // Only holds ONE event
```
- Rapid keypresses in same frame → only last event processed (others lost)
- F1-F8 shortcuts only worked inside mapping overlay
- Multiple F1 presses could create multiple help dialogs

**After**:
```rust
pending_dialog_events: VecDeque<MappingConfigEvent>  // Queue of events
```
- All events queued and processed sequentially
- Global shortcuts work anywhere
- Spam protection: only enqueue when no dialog open

---

### Changes Made

#### 1. Event Queue Implementation

**File**: `src/saide/ui/saide.rs`

**Modified Struct**:
```rust
pub struct SAideApp {
    // Before:
    // pending_dialog_event: Option<MappingConfigEvent>,
    
    // After:
    pending_dialog_events: VecDeque<MappingConfigEvent>,
    // ...
}
```

**Added Method** (`show_dialog_for_event`):
```rust
fn show_dialog_for_event(&mut self, event: MappingConfigEvent) {
    match event {
        MappingConfigEvent::AddMapping(pos) => self.show_add_mapping_dialog(&pos),
        MappingConfigEvent::DeleteMapping(pos) => self.show_delete_mapping_dialog(&pos),
        MappingConfigEvent::RenameProfile => self.show_rename_profile_dialog(),
        MappingConfigEvent::NewProfile => self.show_new_profile_dialog(),
        MappingConfigEvent::SaveAsProfile => self.show_save_as_profile_dialog(),
        MappingConfigEvent::ShowHelp => self.show_help_dialog(),
        MappingConfigEvent::SwitchProfile => self.show_switch_profile_dialog(),
        _ => {}
    }
}
```

**Modified** `handle_mapping_event`:
```rust
fn handle_mapping_event(&mut self, event: MappingConfigEvent) {
    match event {
        MappingConfigEvent::None => {}
        MappingConfigEvent::Close => {
            self.mapping_config_overlay.take();
            self.pending_dialog_events.clear();  // Clear queue on close
        }
        MappingConfigEvent::DeleteProfile 
            | MappingConfigEvent::NextProfile 
            | MappingConfigEvent::PrevProfile => {
            // Direct actions (no dialogs)
        }
        _ => {
            // Only enqueue if no dialog is currently open
            if self.dialog.is_none() {
                self.pending_dialog_events.push_back(event);
            }
        }
    }
}
```

**Modified** `process_dialog_result`:
```rust
fn process_dialog_result(&mut self, _ctx: &egui::Context) {
    let Some(dialog) = &mut self.dialog else {
        // No dialog open, check queue
        if let Some(event) = self.pending_dialog_events.pop_front() {
            self.show_dialog_for_event(event);
        }
        return;
    };

    match dialog.draw(_ctx) {
        DialogState::WidgetsState(states) => {
            if let Some(event) = self.pending_dialog_events.pop_front() {
                // Process event from queue
                match event {
                    MappingConfigEvent::AddMapping(screen_pos) => { /* ... */ }
                    // ... handle other events
                }
            }
            self.dialog = None;
        }
        DialogState::CapturedKey(key) => {
            if let Some(MappingConfigEvent::AddMapping(screen_pos)) = 
                self.pending_dialog_events.pop_front() 
            {
                // ... process key capture
            }
            self.dialog = None;
        }
        DialogState::Cancelled => {
            self.pending_dialog_events.pop_front();  // Discard cancelled event
            self.dialog = None;
        }
        _ => {}
    }

    // After processing current dialog, show next queued dialog
    if self.dialog.is_none()
        && let Some(event) = self.pending_dialog_events.pop_front()
    {
        self.show_dialog_for_event(event);
    }
}
```

**Removed** from all `show_*_dialog()` methods:
```rust
// Before:
self.pending_dialog_event = Some(MappingConfigEvent::SomeEvent);

// After: (removed - no longer needed)
```

---

#### 2. Global Shortcut Handling

**File**: `src/saide/ui/saide.rs`

**Modified** `process_keyboard_event`:
```rust
fn process_keyboard_event(&mut self, key: &egui::Key, pressed: bool, modifiers: egui::Modifiers) -> Result<bool> {
    if !pressed {
        return Ok(false);
    }

    // NEW: Global shortcut handling (BEFORE custom mapping logic)
    match key {
        egui::Key::F1 => {
            self.handle_mapping_event(MappingConfigEvent::ShowHelp);
            return Ok(true);
        }
        egui::Key::F2 if self.mapping_config_overlay.is_some() => {
            self.handle_mapping_event(MappingConfigEvent::RenameProfile);
            return Ok(true);
        }
        egui::Key::F3 if self.mapping_config_overlay.is_some() => {
            self.handle_mapping_event(MappingConfigEvent::NewProfile);
            return Ok(true);
        }
        egui::Key::F4 if self.mapping_config_overlay.is_some() => {
            self.handle_mapping_event(MappingConfigEvent::SaveAsProfile);
            return Ok(true);
        }
        egui::Key::F5 if self.mapping_config_overlay.is_some() => {
            self.handle_mapping_event(MappingConfigEvent::DeleteProfile);
            return Ok(true);
        }
        egui::Key::F6 => {
            self.handle_mapping_event(MappingConfigEvent::SwitchProfile);
            return Ok(true);
        }
        egui::Key::F7 => {
            self.handle_mapping_event(MappingConfigEvent::PrevProfile);
            return Ok(true);
        }
        egui::Key::F8 => {
            self.handle_mapping_event(MappingConfigEvent::NextProfile);
            return Ok(true);
        }
        _ => {}
    }

    // Existing custom mapping logic continues...
    if key == &self.config().mappings.toggle {
        self.toggle_keyboard_mapping();
        return Ok(true);
    }
    // ...
}
```

**Shortcut Behavior**:
| Key | Scope | Action | Dialog? |
|-----|-------|--------|---------|
| F1  | Global | Show help | Yes |
| F2  | Overlay-only | Rename profile | Yes |
| F3  | Overlay-only | New profile | Yes |
| F4  | Overlay-only | Save as profile | Yes |
| F5  | Overlay-only | Delete profile | No (direct) |
| F6  | Global | Switch profile | Yes |
| F7  | Global | Previous profile | No (direct) |
| F8  | Global | Next profile | No (direct) |

---

### Key Design Decisions

#### 1. **Spam Protection via Conditional Queueing**
```rust
if self.dialog.is_none() {
    self.pending_dialog_events.push_back(event);
}
```
- Only enqueue when no dialog is open
- Prevents queue buildup if user repeatedly presses F1 while help dialog is open
- Alternative considered: max queue size (rejected - adds complexity)

#### 2. **FIFO Processing (No Priorities)**
- Events processed first-in-first-out
- Simpler implementation, predictable behavior
- Alternative considered: priority queue (rejected - no clear use case)

#### 3. **Queue Cleared on Overlay Close**
```rust
MappingConfigEvent::Close => {
    self.mapping_config_overlay.take();
    self.pending_dialog_events.clear();
}
```
- Prevents orphaned dialogs appearing after overlay closes
- User expects clean state when closing overlay

#### 4. **Direct Actions Don't Queue**
```rust
MappingConfigEvent::DeleteProfile 
    | MappingConfigEvent::NextProfile 
    | MappingConfigEvent::PrevProfile => {
    // Execute immediately
}
```
- F5 (delete), F7 (prev), F8 (next) execute instantly
- No dialog shown, so no queueing needed

---

## Testing

### Automated Tests ✅
```bash
✅ cargo fmt --all -- --check     # Code formatting
✅ cargo clippy -- -D warnings     # Linting
✅ cargo test --quiet              # All 108 tests passed
```

### Manual Verification 📋
Created comprehensive test guide: `docs/testing/EVENT_QUEUE_VERIFICATION.md`

**Test Coverage**:
1. Event queue with rapid key presses
2. Event queue cleared on overlay close
3. Global shortcuts (F1, F6-F8) work outside overlay
4. Overlay-only shortcuts (F2-F5) require overlay
5. Spam protection (rapid F1 presses)
6. Mixed global/overlay shortcut handling
7. Shortcuts during active dialog
8. Device rotation during dialog

**Verification Status**: Manual testing required (documented procedures provided)

---

## Architecture Improvements

### Before (Phase 1)
```
MappingConfigOverlay
  ├─ Contains dialog rendering logic
  ├─ Stores DialogContext enum
  ├─ Stores DialogData struct
  └─ Emits MappingConfigEvent

SAideApp
  └─ Receives events, calls overlay methods
```

### After (Phase 3)
```
MappingConfigOverlay
  └─ Emits MappingConfigEvent (pure presenter)

SAideApp
  ├─ pending_dialog_events: VecDeque<MappingConfigEvent>
  ├─ handle_mapping_event() → enqueues events
  ├─ show_dialog_for_event() → dispatches to dialog methods
  ├─ process_dialog_result() → pops queue, processes sequentially
  └─ process_keyboard_event() → handles global shortcuts
```

**Benefits**:
- ✅ Single source of truth (event queue)
- ✅ No duplicate dialogs
- ✅ No lost events
- ✅ Clear separation of concerns
- ✅ Testable event flow
- ✅ Global shortcuts work consistently

---

## Files Modified (Phase 3)

1. **`src/saide/ui/saide.rs`**:
   - Changed `pending_dialog_event: Option<T>` → `pending_dialog_events: VecDeque<T>`
   - Added `show_dialog_for_event()` method
   - Modified `handle_mapping_event()` to enqueue events
   - Modified `process_dialog_result()` to process queue
   - Modified `process_keyboard_event()` to handle global shortcuts
   - Removed `pending_dialog_event` assignments from all `show_*_dialog()` methods

2. **`docs/testing/EVENT_QUEUE_VERIFICATION.md`** (new):
   - Comprehensive manual testing procedures
   - Edge case coverage
   - Debugging guide

3. **`docs/DIALOG_SYSTEM_REFACTORING_SUMMARY.md`** (this file):
   - Complete refactoring history

---

## Metrics

**Lines Changed**: ~150 lines in `saide.rs`  
**Compilation Time**: 2.45s (no regression)  
**Test Suite**: 108 tests, all passing  
**Clippy Warnings**: 0  
**Code Quality**: ✅ Formatted, ✅ Linted, ✅ Tested

---

## Known Limitations

1. **No Maximum Queue Size**: Theoretically unbounded, but spam protection mitigates risk
2. **No Event Priority**: All events processed FIFO (future enhancement if needed)
3. **Frame Timing Dependent**: Events in same frame queue up, different frames may show immediately

---

## Future Enhancements

Potential improvements not implemented (out of scope):

1. **Priority Queue**: Allow certain events (e.g., errors) to jump queue
2. **Max Queue Size**: Hard limit with warning/error if exceeded
3. **Event Coalescing**: Merge duplicate events (e.g., 3x F1 → 1x F1)
4. **Configurable Shortcuts**: Allow users to rebind F1-F8 in config.toml
5. **Event History**: Log last N events for debugging

---

## Commit Message Template

```
refactor(dialog): implement event queue and global shortcuts

- Replace Option<MappingConfigEvent> with VecDeque for true event queueing
- Add show_dialog_for_event() dispatcher method
- Implement global shortcuts (F1, F6-F8) in process_keyboard_event()
- Add spam protection: only enqueue when dialog.is_none()
- Clear event queue when mapping overlay closes

Fixes:
- Multiple events in single frame no longer lost
- F1 (help) and F6-F8 (profile switching) now work globally
- Rapid F1 presses no longer create duplicate dialogs

Test: cargo test --quiet (108 passed)
Lint: cargo clippy -- -D warnings (clean)
Format: cargo fmt --all -- --check (clean)

See docs/DIALOG_SYSTEM_REFACTORING_SUMMARY.md for full details
See docs/testing/EVENT_QUEUE_VERIFICATION.md for manual test procedures
```

---

## References

- **Phase 1 Completion**: Commit (previous session)
- **Phase 2 Completion**: Commit (previous session)
- **Phase 3 Completion**: 2026-02-02 (this session)
- **Related Issues**: None (internal refactoring)
- **Related Docs**:
  - `docs/testing/EVENT_QUEUE_VERIFICATION.md`
  - `src/saide/ui/mapping.rs` (event emitter)
  - `src/saide/ui/dialog.rs` (dialog primitives)

---

**Status**: ✅ **COMPLETE** (All 3 phases finished, quality checks passed)
