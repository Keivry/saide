# Dialog System - Event Queue Double-Pop Bug

**Date**: 2026-02-02  
**Issue**: F6 switch profile dialog shows but doesn't execute when overlay is closed

**Severity**: High (broken functionality)

---

## Problem: F6 Switch Profile Not Working (Overlay Closed)

### Symptoms
- 按 F6 时 overlay 关闭
- 对话框正确显示 profile 列表 ✅
- 选择 profile 并点击"确认"
- Profile **没有切换** ❌
- 影响：F6 在 overlay 打开时正常工作，关闭时不工作

### Root Cause

**File**: `src/core/ui/app.rs` 

**Issue in `process_dialog_result()` (line ~1429-1439)**:

```rust
// ❌ Bug: Double pop from same queue
fn process_dialog_result(&mut self, _ctx: &egui::Context) {
    let Some(dialog) = &mut self.dialog else {
        if let Some(event) = self.pending_dialog_events.pop_front() {  // ← First pop
            self.show_dialog_for_event(event);  // Show dialog
        }
        return;
    };

    match dialog.draw(_ctx) {
        DialogState::WidgetsState(states) => {
            if let Some(event) = self.pending_dialog_events.pop_front() {  // ← Second pop
                match event {
                    MappingConfigEvent::SwitchProfile => {
                        // This code never executes because queue is empty!
                        keyboard_mapper.switch_to_profile(&profiles[*idx]);
                    }
                }
            }
        }
    }
}
```

### Event Flow (Before Fix)

```
Frame 1: F6 pressed (overlay closed)
  ↓
handle_mapping_event(): push_back(SwitchProfile)  // Queue: [SwitchProfile]
  ↓
process_dialog_result(): dialog is None
  ↓
Line 1431: event = pop_front() → SwitchProfile    // Queue: [] ❌ Empty!
  ↓
Line 1432: show_dialog_for_event(SwitchProfile)   // Dialog shown
  ↓
self.dialog = Some(...)

Frame 2+: Dialog displayed, waiting for user input
  ↓
(User selects profile and clicks Confirm)
  ↓

Frame N: User confirmed
  ↓
process_dialog_result(): dialog is Some
  ↓
dialog.draw() returns WidgetsState with selected index
  ↓
Line 1439: event = pop_front() → None             // Queue is empty! ❌
  ↓
SwitchProfile case doesn't match (event is None)
  ↓
Profile switch code never executes ❌
```

**Root Cause Analysis**:

1. **First pop (line 1431)**: Event removed to show dialog
2. **Dialog created**: Now blocking on user input
3. **Second pop (line 1439)**: Queue already empty, can't retrieve event
4. **Missing context**: Can't execute switch logic without event

**Why This Worked in Overlay**:

When overlay is open, `handle_mapping_event()` calls:
```rust
self.pending_dialog_events.push_back(event.clone());
self.show_switch_profile_dialog();  // ← Dialog shown immediately
```

Dialog shown **before** `process_dialog_result()` runs, so:
- Line 1431 doesn't execute (dialog already exists)
- Line 1439 pops successfully (event still in queue)

**Why This Failed Without Overlay**:

When overlay is closed, `handle_mapping_event()` only queues:
```rust
self.pending_dialog_events.push_back(event);
// No show_switch_profile_dialog() call!
```

Dialog shown **by** `process_dialog_result()` line 1431-1432:
- Line 1431 pops event (queue becomes empty)
- Line 1439 pops again (but queue is empty!)

---

## Solution

### Fix: Use `front()` Instead of `pop_front()` for Dialog Display

**File**: `src/core/ui/app.rs` (line ~1429-1434)

**After**:
```rust
fn process_dialog_result(&mut self, _ctx: &egui::Context) {
    let Some(dialog) = &mut self.dialog else {
        if let Some(event) = self.pending_dialog_events.front() {  // ✅ Peek, don't pop
            self.show_dialog_for_event(event.clone());              // Clone for display
        }
        return;
    };

    match dialog.draw(_ctx) {
        DialogState::WidgetsState(states) => {
            if let Some(event) = self.pending_dialog_events.pop_front() {  // ✅ Pop on confirm
                match event {
                    MappingConfigEvent::SwitchProfile => {
                        // Now this executes! Event available in queue
                        keyboard_mapper.switch_to_profile(&profiles[*idx]);
                    }
                }
            }
        }
    }
}
```

### Event Flow (After Fix)

```
Frame 1: F6 pressed (overlay closed)
  ↓
handle_mapping_event(): push_back(SwitchProfile)  // Queue: [SwitchProfile]
  ↓
process_dialog_result(): dialog is None
  ↓
Line 1431: event = front() → SwitchProfile        // Queue: [SwitchProfile] ✅ Still there!
  ↓
Line 1432: show_dialog_for_event(event.clone())   // Dialog shown
  ↓
self.dialog = Some(...)

Frame 2+: Dialog displayed, waiting for user input
  ↓
(User selects profile and clicks Confirm)
  ↓

Frame N: User confirmed
  ↓
process_dialog_result(): dialog is Some
  ↓
dialog.draw() returns WidgetsState with selected index
  ↓
Line 1439: event = pop_front() → SwitchProfile    // Queue: [] ✅ Event retrieved!
  ↓
SwitchProfile case matches!
  ↓
keyboard_mapper.switch_to_profile(&profiles[*idx]) ✅
  ↓
indicator.update_active_profile() ✅
  ↓
config_manager.save() ✅
```

---

## Why This Design?

### Queue Semantics

**Show Dialog Phase**:
- **Purpose**: Display UI to user
- **Action**: Peek at event to know what dialog to show
- **Reason**: Event still needed later for processing result

**Confirm Dialog Phase**:
- **Purpose**: Execute action with user input
- **Action**: Pop event from queue (consume it)
- **Reason**: Event no longer needed after processing

### Analogy

Think of the queue as a **TODO list**:

```
❌ Bad (Double Pop):
1. Read task: "Switch profile"
2. Cross out task (pop)
3. Show dialog to user
4. User confirms
5. Try to read task again → Empty! Can't remember what to do

✅ Good (Peek + Pop):
1. Read task: "Switch profile" (peek)
2. Show dialog to user
3. User confirms
4. Cross out task (pop) AND execute it
```

---

## Testing

### Manual Verification

**Test 1: F6 Without Overlay (Main Bug)**
```bash
cargo run --release
# Wait for device connection
# Do NOT press M (overlay should be closed)
# Press F6
# Expected: ✅ Switch profile dialog appears
# Select a different profile
# Click "Confirm"
# Expected: ✅ Profile switches, indicator updates
```

**Test 2: F6 With Overlay (Regression Check)**
```bash
cargo run --release
# Press M to open overlay
# Press F6
# Expected: ✅ Switch profile dialog appears
# Select a different profile
# Click "Confirm"
# Expected: ✅ Profile switches (same as before fix)
```

**Test 3: F1 Help Dialog (Regression Check)**
```bash
cargo run --release
# Press F1 (overlay closed)
# Expected: ✅ Help dialog appears
# Click "Cancel"
# Expected: ✅ Dialog closes
```

### Automated Tests

```bash
✅ cargo build                      # Passed
✅ cargo fmt --all -- --check       # Passed
✅ cargo clippy -- -D warnings      # Passed (0 warnings)
✅ cargo test --quiet               # All 108 tests passed
```

---

## Impact

### Before
- ❌ F6 在 overlay 关闭时显示对话框但不执行切换
- ❌ 用户选择 profile 后没有任何反应（confusing UX）
- ✅ F6 在 overlay 打开时正常工作（因为显示对话框的路径不同）

### After
- ✅ F6 在任何状态下都正常工作（overlay 打开/关闭都能切换 profile）
- ✅ 用户选择 profile 后立即切换并保存配置
- ✅ 行为一致性：F1/F6/F7/F8 都是真正的全局快捷键

---

## Architecture Insight

### Event Queue Design Pattern

**Purpose**: Preserve event context across multiple frames

**Key Principle**: **Peek for display, pop for processing**

```rust
// ✅ Correct Pattern
if dialog.is_none() {
    if let Some(event) = queue.front() {      // Peek (non-destructive)
        show_dialog_for(event.clone());       // Display phase
    }
}

if dialog.is_some() {
    match dialog.draw() {
        Confirmed => {
            if let Some(event) = queue.pop_front() {  // Pop (destructive)
                process(event);                       // Processing phase
            }
        }
    }
}
```

**Anti-pattern**:
```rust
// ❌ Wrong: Pop twice from same queue
if dialog.is_none() {
    if let Some(event) = queue.pop_front() {  // Pop 1
        show_dialog_for(event);
    }
}

if dialog.is_some() {
    if let Some(event) = queue.pop_front() {  // Pop 2 → None!
        process(event);  // Never executes
    }
}
```

---

## Related Issues

- **Previous Fix**: F6 显示对话框但不切换（overlay 打开时）
  - **Solution**: Queue event before showing dialog
- **This Fix**: F6 显示对话框但不切换（overlay 关闭时）
  - **Solution**: Peek (don't pop) when showing dialog

**Common Theme**: Event queue lifetime management

---

## Files Modified

1. **`src/core/ui/app.rs`**:
   - Line ~1431: Change `pop_front()` to `front()` + `clone()`

---

## Commit Message

```
fix(dialog): use peek instead of pop when showing dialog from queue

Problem: F6 switch profile not working when overlay closed
- Root cause: Double-pop from event queue in process_dialog_result()
  - First pop (line 1431): Remove event to show dialog
  - Second pop (line 1439): Queue empty, can't retrieve event
  - Result: Dialog shown but action never executes

Solution: Use front() + clone() for dialog display phase
- Peek event without removing (front + clone)
- Pop event only on confirm (pop_front on WidgetsState)
- Ensures event available for both display and processing

Why overlay worked but non-overlay failed:
- Overlay: Dialog shown by handle_mapping_event() before process_dialog_result()
  → Line 1431 skipped, line 1439 pops successfully
- Non-overlay: Dialog shown by process_dialog_result() line 1431
  → Line 1431 pops, line 1439 finds empty queue

Changes:
- src/core/ui/app.rs: Change pop_front() to front() + clone()
  when showing dialog (line 1431), keep pop_front() for confirm (line 1439)

Test: cargo test --quiet (108 passed)
Lint: cargo clippy -- -D warnings (clean)
Manual: F6 switches profile with/without overlay
```

---

**Status**: ✅ **FIXED** (F6 now works in all scenarios)
