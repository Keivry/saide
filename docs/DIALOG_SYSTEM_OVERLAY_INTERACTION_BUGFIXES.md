# Dialog System - Overlay Interaction Bugfixes

**Date**: 2026-02-02  
**Issues**: 
1. F6 switch profile dialog shows but doesn't execute switch
2. Add/Delete mapping in overlay shows dialog but doesn't add/delete

**Severity**: High (broken functionality)

---

## Problem 1: F6 Switch Profile Dialog Not Executing

### Symptoms
- 按下 F6 显示切换 profile 对话框 ✅
- 在列表中选择 profile 后点击确认
- Profile **没有切换** ❌

### Root Cause

**File**: `src/core/ui/app.rs` 

**Issue in `handle_mapping_event()` (line ~626)**:
```rust
// Before fix:
MappingConfigEvent::SwitchProfile => {
    if self.mapping_config_overlay.is_some() {
        if self.dialog.is_none() {
            self.show_switch_profile_dialog();  // ❌ Dialog shown but event NOT queued
        }
    } else if self.dialog.is_none() {
        self.pending_dialog_events.push_back(event);
    }
}
```

**Issue in `process_dialog_result()` (line ~1439)**:
```rust
match dialog.draw(_ctx) {
    DialogState::WidgetsState(states) => {
        if let Some(event) = self.pending_dialog_events.pop_front() {  // ❌ Queue is empty!
            match event {
                MappingConfigEvent::SwitchProfile => {
                    // This code never executes because event wasn't queued
                    if let Some(WidgetState::ListSelection(idx)) = states.get("profile") {
                        keyboard_mapper.switch_to_profile(&profiles[*idx]);
                    }
                }
            }
        }
    }
}
```

**Root Cause**:
1. `show_switch_profile_dialog()` called directly without adding event to queue
2. When dialog confirms, `pop_front()` returns `None` (queue empty)
3. Switch logic at line 1468-1488 never executes
4. Dialog closes but nothing happens

---

## Problem 2: Add/Delete Mapping Not Working

### Symptoms
- 在 overlay 中鼠标左键/右键点击位置
- 显示"添加映射"/"删除映射"对话框 ✅
- 确认后映射**没有添加/删除** ❌

### Root Cause

**File**: `src/core/ui/app.rs` 

**Issue in `handle_mapping_event()` (line ~626)**:
```rust
// Before fix:
MappingConfigEvent::AddMapping(pos) => {
    if self.mapping_config_overlay.is_some() && self.dialog.is_none() {
        self.show_add_mapping_dialog(&pos);  // ❌ Dialog shown but event NOT queued
    }
}

MappingConfigEvent::DeleteMapping(pos) => {
    if self.mapping_config_overlay.is_some() && self.dialog.is_none() {
        self.show_delete_mapping_dialog(&pos);  // ❌ Dialog shown but event NOT queued
    }
}
```

**Issue in `process_dialog_result()` (line ~1517, ~1489)**:
```rust
DialogState::CapturedKey(key) => {
    if let Some(MappingConfigEvent::AddMapping(screen_pos)) =
        self.pending_dialog_events.pop_front()  // ❌ Queue is empty!
    {
        // This code never executes
        let percent_pos = /* convert screen_pos to mapping coords */;
        self.add_mapping(key, &percent_pos);
    }
}

DialogState::WidgetsState(states) => {
    if let Some(event) = self.pending_dialog_events.pop_front() {
        match event {
            MappingConfigEvent::DeleteMapping(screen_pos) => {
                // This code never executes
                let percent_pos = /* convert screen_pos */;
                self.delete_mapping(nearest_key);
            }
        }
    }
}
```

**Root Cause**:
1. Mouse click generates `AddMapping(pos)` or `DeleteMapping(pos)`
2. Event contains critical data: **click position (`pos`)**
3. Dialog shown but event **not queued**
4. When dialog confirms, `pop_front()` returns `None`
5. Can't retrieve `pos` → can't add/delete mapping

---

## Solution

### Fix: Always Queue Events Before Showing Dialogs

**File**: `src/core/ui/app.rs` (line ~626)

**After**:
```rust
MappingConfigEvent::AddMapping(pos) => {
    if self.mapping_config_overlay.is_some() && self.dialog.is_none() {
        self.pending_dialog_events.push_back(event.clone());  // ✅ Queue FIRST
        self.show_add_mapping_dialog(&pos);                   // Then show dialog
    }
}

MappingConfigEvent::DeleteMapping(pos) => {
    if self.mapping_config_overlay.is_some() && self.dialog.is_none() {
        self.pending_dialog_events.push_back(event.clone());  // ✅ Queue FIRST
        self.show_delete_mapping_dialog(&pos);                // Then show dialog
    }
}

MappingConfigEvent::SwitchProfile => {
    if self.mapping_config_overlay.is_some() {
        if self.dialog.is_none() {
            self.pending_dialog_events.push_back(event.clone());  // ✅ Queue FIRST
            self.show_switch_profile_dialog();                    // Then show dialog
        }
    } else if self.dialog.is_none() {
        self.pending_dialog_events.push_back(event);
    }
}
```

### Why This Works

**Event Flow (After Fix)**:
```
1. User clicks mouse / presses F6
   ↓
2. Event generated: AddMapping(Pos2{x: 100, y: 200})
   ↓
3. handle_mapping_event() receives event
   ↓
4. Event cloned and pushed to queue
   ↓
5. Dialog shown with position info
   ↓
6. User captures key / selects profile
   ↓
7. process_dialog_result() called
   ↓
8. pop_front() retrieves original event with position
   ↓
9. Extract position from event
   ↓
10. add_mapping(key, position) / switch_to_profile()
```

**Key Insight**: 
- Dialog only stores **user input** (key capture, text input, list selection)
- Event stores **context data** (position, profile name, etc.)
- **Both are needed** to complete the operation
- Queue preserves context until dialog confirms

---

## Testing

### Manual Verification

**Test 1: F6 Switch Profile**
```bash
cargo run --release
# Press M to open overlay
# Press F6
# Expected: ✅ Dialog shows profile list
# Select a different profile
# Click "Confirm"
# Expected: ✅ Profile switches, indicator updates
```

**Test 2: Add Mapping**
```bash
cargo run --release
# Press M to open overlay
# Left-click on screen position (e.g., top-left corner)
# Expected: ✅ Dialog shows "Press a key to map to (x, y)"
# Press W key
# Expected: ✅ Mapping added, W key appears at clicked position
```

**Test 3: Delete Mapping**
```bash
cargo run --release
# Press M to open overlay
# Ensure at least one mapping exists (from Test 2)
# Right-click on mapping circle
# Expected: ✅ Dialog shows "Delete mapping for W at (x, y)?"
# Click "Confirm"
# Expected: ✅ Mapping deleted, circle disappears
```

### Automated Tests

```bash
✅ cargo build                      # Passed
✅ cargo fmt --all -- --check        # Passed  
✅ cargo clippy -- -D warnings       # Passed (0 warnings)
✅ cargo test --quiet                # All 108 tests passed
```

---

## Impact

### Before
- ❌ F6 显示对话框但不切换 profile
- ❌ 鼠标点击显示对话框但不添加/删除映射
- ❌ 用户完成对话框操作后没有任何效果（confusing UX）

### After
- ✅ F6 正常切换 profile（对话框 → 选择 → 切换生效）
- ✅ 鼠标左键正常添加映射（点击 → 按键 → 映射创建）
- ✅ 鼠标右键正常删除映射（点击 → 确认 → 映射删除）
- ✅ 所有对话框操作都能正确执行

---

## Architecture Insight

### Design Pattern: Event Queue + Dialog Separation

**Why Queue Events Before Showing Dialogs?**

1. **Data Preservation**:
   - Events carry context (position, profile name, etc.)
   - Dialogs only capture user input (key press, text, selection)
   - Queue preserves context until dialog completes

2. **Decoupling**:
   - Event generation (mouse click, F6 press)
   - Dialog display (show UI)
   - Result processing (execute action)
   - Three stages can happen in different frames

3. **Consistency**:
   - Global shortcuts (F1, F6 without overlay) → queue event, process later
   - Overlay shortcuts (F6 with overlay) → queue event, show dialog immediately
   - Same processing logic (`process_dialog_result()`) handles both cases

**Anti-pattern (Before Fix)**:
```rust
// ❌ Show dialog without queueing event
self.show_add_mapping_dialog(&pos);
// Context lost! Can't retrieve `pos` later
```

**Correct Pattern (After Fix)**:
```rust
// ✅ Queue event THEN show dialog
self.pending_dialog_events.push_back(event.clone());
self.show_add_mapping_dialog(&pos);
// Context preserved in queue, dialog just shows UI
```

---

## Files Modified

1. **`src/core/ui/app.rs`**:
   - Line ~626: `handle_mapping_event()` - Queue events before showing dialogs
   
---

## Related

- **Phase 3 Implementation**: Event queue + global shortcuts
- **Previous Bugfix**: F1/F6 not showing when overlay closed
- **Design Decision**: Events carry context, dialogs capture input

---

## Commit Message

```
fix(overlay): queue events before showing dialogs for add/delete/switch

Problem 1: F6 switch profile dialog not executing
- Root cause: show_switch_profile_dialog() called without queueing event
- Fix: push_back(event.clone()) before showing dialog
- Impact: Profile switch now works when dialog confirms

Problem 2: Add/Delete mapping not working in overlay
- Root cause: AddMapping/DeleteMapping events not queued
- Fix: Queue event (with position data) before showing dialog
- Impact: Mouse clicks now correctly add/delete mappings

Architecture: Events carry context (position, profile), dialogs capture
user input (key, selection). Both needed → must queue before dialog.

Test: cargo test --quiet (108 passed)
Lint: cargo clippy -- -D warnings (clean)
Manual: F6 switches profile, mouse clicks add/delete mappings
```

---

**Status**: ✅ **FIXED** (Both issues resolved, all tests passing)
