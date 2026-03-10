# Event Queue & Global Shortcuts - Manual Verification Guide

**Date**: 2026-02-02  
**Branch**: `windows`  
**Scope**: Verify event queue prevents duplicate dialogs and global shortcuts work correctly

---

## Prerequisites

```bash
cd /home/keivry/项目/Rust/saide
cargo build --release
./target/release/saide
```

**Required**: Android device connected via ADB with USB debugging enabled

---

## Test 1: Event Queue with Rapid Key Presses

**Objective**: Verify multiple events queued in single frame are processed sequentially

### Test 1.1: Multiple Dialog Events (Editor Open)

**Steps**:
1. Launch SAide and wait for device connection
2. Press `M` to open the mapping editor
3. **Rapidly** press `F1` then `F6` (within ~50ms)
4. Observe dialog behavior

**Expected Result**:
- ✅ Help dialog appears first
- ✅ After closing help dialog, Switch Profile dialog appears automatically
- ✅ No dialogs are lost or skipped

**Failure Symptoms** (if queue not working):
- ❌ Only Switch Profile dialog appears (F1 event lost)
- ❌ Both dialogs try to render simultaneously (UI corruption)

### Test 1.2: Event Queue Cleared on Editor Close

**Steps**:
1. Press `M` to open the mapping editor
2. Rapidly press `F1`, `F2`, `F3` (within ~50ms)
3. **Immediately** press `Esc` to close the editor (before any dialog appears)
4. Wait 2 seconds

**Expected Result**:
- ✅ No dialogs appear after closing the editor
- ✅ Event queue was cleared when the editor closed

**Failure Symptoms**:
- ❌ Dialogs appear after the editor is closed
- ❌ Application crashes

---

## Test 2: Global Shortcuts Outside the Editor

**Objective**: Verify F1/F6/F7/F8 work globally, while F2-F5 require the editor to be open

### Test 2.1: Global Shortcuts (Editor Closed)

**Steps**:
1. Ensure the editor is **closed** (press `Esc` if needed)
2. Press `F1`
3. Close dialog
4. Press `F6`
5. Close dialog
6. Press `F7`
7. Wait 200ms, observe indicator
8. Press `F8`
9. Wait 200ms, observe indicator

**Expected Result**:
- ✅ `F1`: Help dialog appears
- ✅ `F6`: Switch Profile dialog appears
- ✅ `F7`: Profile indicator changes to previous profile (no dialog)
- ✅ `F8`: Profile indicator changes to next profile (no dialog)

**Failure Symptoms**:
- ❌ Any of the above keys do nothing
- ❌ Keys only work when the editor is open

### Test 2.2: Editor-Only Shortcuts (Editor Closed)

**Steps**:
1. Ensure the editor is **closed**
2. Press `F2` (Rename Profile)
3. Wait 500ms
4. Press `F3` (New Profile)
5. Wait 500ms
6. Press `F4` (Delete Profile)
7. Wait 500ms
8. Press `F5` (Save As Profile)
9. Wait 500ms

**Expected Result**:
- ✅ **No dialogs appear** for any of F2-F5
- ✅ No error messages in terminal

**Failure Symptoms**:
- ❌ Dialogs appear when the editor is closed
- ❌ Errors in terminal about the editor not being available

### Test 2.3: Editor-Only Shortcuts (Editor Open)

**Steps**:
1. Press `M` to open the mapping editor
2. Press `F2`
3. Close dialog (press `Esc`)
4. Press `F3`
5. Close dialog
6. Press `F4`
7. Close dialog
8. Press `F5`

**Expected Result**:
- ✅ `F2`: Rename Profile dialog appears (with current profile name)
- ✅ `F3`: New Profile dialog appears (empty name input)
- ✅ `F4`: Delete Profile dialog appears (with current profile name)
- ✅ `F5`: Save As Profile dialog appears (with current profile name)

**Failure Symptoms**:
- ❌ Any dialog fails to appear
- ❌ Wrong dialog appears for a key

---

## Test 3: Edge Cases

### Test 3.1: Spam Protection

**Steps**:
1. Press `M` to open the mapping editor
2. Rapidly press `F1` 10 times (as fast as possible)
3. Observe behavior

**Expected Result**:
- ✅ Only one Help dialog appears
- ✅ No queue buildup (only 1 event queued when dialog is open)
- ✅ Application remains responsive

**Failure Symptoms**:
- ❌ 10 dialogs queue up and appear sequentially after closing first one
- ❌ UI becomes unresponsive

### Test 3.2: Mixed Global and Editor Shortcuts

**Steps**:
1. Press `M` to open the mapping editor
2. Rapidly press `F1`, `F7`, `F6`, `F8` (within ~100ms)
3. Observe behavior

**Expected Result**:
- ✅ `F1` dialog appears first (queued)
- ✅ `F7` and `F8` execute immediately (profile switches, no dialogs)
- ✅ After closing F1 dialog, `F6` dialog appears

**Failure Symptoms**:
- ❌ F7/F8 are queued instead of executing immediately
- ❌ Profile doesn't switch when pressing F7/F8

---

## Test 4: Integration with Existing Features

### Test 4.1: Shortcuts During Active Dialog

**Steps**:
1. Press `M` to open the editor
2. Press `F1` to show Help dialog
3. While Help dialog is open, press `F6`
4. Wait 200ms (dialog still open)
5. Close Help dialog

**Expected Result**:
- ✅ `F6` is **not** queued (dialog.is_some() prevents queueing)
- ✅ After closing Help dialog, no Switch Profile dialog appears

**Failure Symptoms**:
- ❌ F6 is queued and appears after closing Help dialog
- ❌ Both dialogs render simultaneously

### Test 4.2: Device Rotation During Dialog

**Steps**:
1. Press `M` to open the editor
2. Press `F1` to show Help dialog
3. Rotate device (or trigger rotation via ADB)
4. Observe dialog behavior

**Expected Result**:
- ✅ Dialog remains visible and functional
- ✅ No crashes or UI corruption

**Failure Symptoms**:
- ❌ Dialog disappears on rotation
- ❌ Application crashes

---

## Verification Checklist

After completing all tests, verify:

- [ ] Event queue prevents duplicate dialogs (Test 1.1)
- [ ] Event queue clears on editor close (Test 1.2)
- [ ] Global shortcuts (F1, F6, F7, F8) work without the editor (Test 2.1)
- [ ] Editor-only shortcuts (F2-F5) require the editor (Test 2.2, 2.3)
- [ ] Spam protection works (Test 3.1)
- [ ] Direct actions (F5, F7, F8) execute immediately (Test 3.2)
- [ ] No queueing during active dialog (Test 4.1)
- [ ] Dialogs survive device rotation (Test 4.2)

---

## Debugging

If tests fail, enable debug logging:

```bash
RUST_LOG=debug ./target/release/saide 2>&1 | grep -E "(dialog|event|shortcut)"
```

**Key Log Patterns**:
- `handle_mapping_event`: Event received
- `show_dialog_for_event`: Dispatching event to dialog
- `process_dialog_result`: Dialog processing
- `pending_dialog_events`: Queue operations

---

## Known Limitations

1. **Frame Timing**: Events queued within same frame (~16ms @ 60fps) will be processed sequentially. Events in different frames may show dialogs immediately.
2. **No Priority**: Events processed FIFO. No way to prioritize certain dialogs.
3. **No Max Queue Size**: Theoretical unbounded queue (mitigated by spam protection).

---

## Revision History

- **2026-02-02**: Initial version (event queue + global shortcuts implementation)
