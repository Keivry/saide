# Dialog System - Critical Bugfixes

**Date**: 2026-02-02  
**Issues**: 
1. F1 help dialog not showing when overlay is closed
2. Cancel button unresponsive in all dialogs

**Severity**: High (broken functionality)

---

## Problem 1: F1 Help Dialog Not Showing (Overlay Closed)

### Symptoms
- 按下 F1 时，overlay 关闭状态下不显示帮助对话框
- 打开 overlay 后才会显示之前按的 F1 对应的帮助对话框

### Root Cause

**File**: `src/saide/ui/saide.rs` (line ~1636)

**Before**:
```rust
if self.mapping_config_overlay.is_some() {
    self.process_mapping_config_events(ctx);
} else if self.dialog.is_none() {
    self.process_input_events(ctx);
}
```

**Issue**: 
1. F1 pressed → `process_keyboard_event()` → `handle_mapping_event(ShowHelp)` → event queued
2. Overlay closed → `process_mapping_config_events()` NOT called
3. `process_dialog_result()` NOT called (it's inside `process_mapping_config_events()`)
4. Event stays in queue forever, never processed
5. User opens overlay → `process_mapping_config_events()` called → dialog finally shows

### Solution

**After**:
```rust
if self.mapping_config_overlay.is_some() {
    self.process_mapping_config_events(ctx);
} else {
    self.process_dialog_result(ctx);
    if self.dialog.is_none() {
        self.process_input_events(ctx);
    }
}
```

**Rationale**: 
- Always process dialogs, regardless of overlay state
- Global shortcuts (F1, F6-F8) work without overlay
- Dialog processing must happen every frame to handle queued events

---

## Problem 2: Cancel Button Unresponsive

### Symptoms
- 点击对话框的"取消"按钮没有任何反应
- 只能通过 ESC 键关闭对话框
- 影响所有对话框（帮助、重命名、新建配置等）

### Root Cause

**File**: `src/saide/ui/dialog.rs` (line ~332)

**Before**:
```rust
let confirm_enabled = self.body.validate();
let mut confirm = ButtonState::Confirm == self.draw_buttons(ui, confirm_enabled);

// ... key input handling ...

// If confirmed, collect widget states
if confirm {
    state = DialogState::WidgetsState(self.body.state());
}
```

**Issue**: 
1. `draw_buttons()` returns `ButtonState::Cancelled` when cancel button clicked
2. Code only checks `ButtonState::Confirm`, ignores `Cancelled`
3. `state` remains `DialogState::None` when cancel clicked
4. Dialog doesn't close, no state change

### Solution

**After**:
```rust
let confirm_enabled = self.body.validate();
let button_state = self.draw_buttons(ui, confirm_enabled);
let mut confirm = button_state == ButtonState::Confirm;

// ... key input handling ...

if button_state == ButtonState::Cancelled {
    state = DialogState::Cancelled;
} else if confirm {
    state = DialogState::WidgetsState(self.body.state());
}
```

**Rationale**: 
- Capture `button_state` before it's lost
- Explicitly check for `Cancelled` button click
- Set `state = DialogState::Cancelled` to trigger dialog close
- Matches existing ESC key behavior

---

## Testing

### Manual Verification

**Test 1: F1 Help Dialog (Overlay Closed)**
```bash
cargo run --release
# Wait for device connection
# Do NOT press M (overlay should be closed)
# Press F1
# Expected: ✅ Help dialog appears immediately
```

**Test 2: Cancel Button**
```bash
cargo run --release
# Press M to open overlay
# Press F1 to show help dialog
# Click "Cancel" button
# Expected: ✅ Dialog closes
```

**Test 3: F6 Switch Profile (Overlay Closed)**
```bash
cargo run --release
# Press F6 (overlay closed)
# Expected: ✅ Switch profile dialog appears
# Click "Cancel"
# Expected: ✅ Dialog closes
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
- ❌ F1/F6/F7/F8 只在 overlay 打开时有效（与全局快捷键设计矛盾）
- ❌ 所有对话框的取消按钮不工作（用户体验差）

### After
- ✅ F1/F6/F7/F8 在任何状态下都能正常工作（真正的全局快捷键）
- ✅ 取消按钮正常工作（与 ESC 键行为一致）
- ✅ 对话框处理与 overlay 状态解耦

---

## Files Modified

1. **`src/saide/ui/saide.rs`**:
   - Line ~1636: Always call `process_dialog_result()` when overlay closed
   
2. **`src/saide/ui/dialog.rs`**:
   - Line ~331-377: Handle `ButtonState::Cancelled` properly

---

## Related

- **Phase 3 Implementation**: Event queue + global shortcuts
- **Previous Hotfix**: Log spam when overlay closed
- **Design Decision**: Dialogs are global, not tied to overlay state

---

## Commit Message

```
fix(dialog): enable global shortcuts and cancel button

Problem 1: F1/F6 dialogs not showing when overlay closed
- Root cause: process_dialog_result() only called when overlay open
- Fix: Always process dialogs, check overlay state separately
- Impact: Global shortcuts now truly global

Problem 2: Cancel button unresponsive in all dialogs
- Root cause: ButtonState::Cancelled not checked in draw()
- Fix: Capture button_state and handle Cancelled explicitly
- Impact: Cancel button now works like ESC key

Test: cargo test --quiet (108 passed)
Lint: cargo clippy -- -D warnings (clean)
Manual: F1 shows help when overlay closed, cancel button closes dialogs
```

---

**Status**: ✅ **FIXED** (Both issues resolved, all tests passing)
