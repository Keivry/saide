# Dialog System Refactoring - Hotfix

**Date**: 2026-02-02  
**Issue**: Excessive warning logs when mapping overlay is closed  
**Severity**: Minor (log spam)

---

## Problem

当 `MappingConfigOverlay` 未开启时，每帧都输出以下警告：

```
2026-02-02T03:22:37.680615Z  WARN saide::saide::ui::saide: Mapping config overlay not available, skipping mapping config events
```

**Root Cause**: `process_mapping_config_events()` 在 `update()` 中无条件调用，即使 overlay 为 `None`，导致每帧（~60fps）都触发 warn! 日志。

---

## Solution

### Change 1: Guard `process_mapping_config_events()` Call

**File**: `src/saide/ui/saide.rs` (line ~1638)

**Before**:
```rust
// Process mapping configuration window
self.process_mapping_config_events(ctx);

// When mapping config window is open, skip normal input processing
if self.mapping_config_overlay.is_none() && self.dialog.is_none() {
    // Handle input events
    self.process_input_events(ctx);
}
```

**After**:
```rust
if self.mapping_config_overlay.is_some() {
    self.process_mapping_config_events(ctx);
} else if self.dialog.is_none() {
    self.process_input_events(ctx);
}
```

**Rationale**: Only call `process_mapping_config_events()` when overlay exists. Simplifies control flow and eliminates unnecessary method calls.

### Change 2: Remove Redundant Warning

**File**: `src/saide/ui/saide.rs` (line ~570)

**Before**:
```rust
let Some(mapping_config_window) = &mut self.mapping_config_overlay else {
    warn!("Mapping config overlay not available, skipping mapping config events");
    return;
};
```

**After**:
```rust
let Some(mapping_config_window) = &mut self.mapping_config_overlay else {
    return;
};
```

**Rationale**: Caller now guarantees overlay exists, so this branch is only reached if:
1. Overlay becomes `None` between the check and this line (impossible in single-threaded egui)
2. Future refactoring introduces a bug (early return without warning is acceptable)

Kept the keyboard_mapper warning since it's a genuine error condition.

---

## Impact

**Before**: ~60 WARN logs per second when overlay closed  
**After**: 0 WARN logs (clean terminal output)

**Performance**: Negligible improvement (avoided ~60 unnecessary method calls per second)

**Code Quality**:
- ✅ Clearer control flow (if-else instead of double-check)
- ✅ Fewer logs = easier debugging
- ✅ Matches existing pattern for dialog handling

---

## Testing

```bash
✅ cargo build                      # Passed
✅ cargo fmt --all -- --check        # Passed  
✅ cargo clippy -- -D warnings       # Passed
✅ cargo test --quiet                # All 108 tests passed
```

**Manual Verification**:
1. Launch `cargo run --release`
2. Connect device
3. Do NOT press `M` (keep overlay closed)
4. Wait 5 seconds, check terminal
5. Expected: No "Mapping config overlay not available" warnings
6. Press `M` to open overlay
7. Expected: Overlay works normally

---

## Related Issues

None (discovered during Phase 3 implementation)

---

## Commit Message

```
fix(dialog): eliminate log spam when overlay is closed

- Only call process_mapping_config_events() when overlay exists
- Remove redundant warning log (caller guarantees overlay.is_some())
- Simplify control flow: if overlay → process events, else → process input

Before: ~60 WARN logs/second when overlay closed
After: Clean terminal output

Test: cargo test --quiet (108 passed)
Lint: cargo clippy -- -D warnings (clean)
```

---

**Status**: ✅ Fixed and verified
