# Save/Restore State Refactoring - Summary (Revised)

**Issue**: [#592](https://github.com/rmstdope/neser/issues/592)  
**Status**: Proposal Revised - Awaiting Decision  
**Date**: 2026-02-17 (Updated after navigator feedback)

## Quick Summary

After exploring key questions from the navigator, I've revised the proposal to be **much simpler and more pragmatic**.

### 📊 Key Questions Answered

**Q1: Do we need separate State structs?**  
**A**: Yes - component structs contain non-serializable types (`Rc<RefCell<>>`), so we need separate state structs for serialization.

**Q2: Do state structs need to be centralized in savestate.rs?**  
**A**: No! They can (and should) be defined alongside their components for better encapsulation.

**Q3: Can we avoid state structs entirely?**  
**A**: Not really - the current pattern is actually quite good given the constraints.

### ✅ Revised Recommendation: Move State Structs (Option 1)

**Simple change**: Move state struct definitions from `savestate.rs` to component files.

**Example**:
```rust
// Before: State in savestate.rs, impl in cpu.rs (split)
// After: Both in cpu.rs (together)

// src/cpu/cpu.rs
#[derive(Serialize, Deserialize)]
pub struct CpuState { /* ... */ }

impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
}

// src/console/savestate.rs (just imports and aggregates)
use crate::cpu::CpuState;
pub struct SaveState {
    pub cpu: CpuState,
    // ...
}
```

### 📈 Benefits

- ✅ State struct next to implementation (single file to edit)
- ✅ Better encapsulation
- ✅ Minimal effort (2-3 hours)
- ✅ Very low risk (just moving code)
- ✅ No behavioral changes

### ⏱️ Effort Estimate

**2-3 hours** total:
1. Move state structs one component at a time
2. Update imports in savestate.rs
3. Test after each move

### 🤔 Why Not Traits?

The original proposal was **over-engineered**. Adding a trait would:
- Add complexity without commensurate benefit
- Require 6-10 hours instead of 2-3
- Introduce trait bounds and associated types
- Not significantly improve the code

The current pattern is fine - state structs just need to be in the right place.

### 📖 Full Details

See the complete revised proposal:

**[docs/refactoring-save-restore-state.md](./refactoring-save-restore-state.md)**

## Decision Required

**Option 1: Move State Structs** (Recommended)
- Simple, pragmatic improvement
- 2-3 hours investment
- Very low risk

**Option 2: Keep Current**
- Works fine as-is
- Zero investment
- Zero risk

## Response Format

Please respond with:

1. **"Approve Option 1"** - I'll move the state structs to components
2. **"Keep current"** - I'll close the issue with rationale
3. **"Changes needed: [details]"** - I'll revise further

---

**What changed from original proposal:**
- ✅ Explored whether we need state structs (yes, we do)
- ✅ Explored whether they need to be centralized (no, they don't)
- ✅ Dropped the trait-based approach (too complex for the benefit)
- ✅ Simplified to just moving struct definitions
- ✅ Reduced effort from 9-14 hours to 2-3 hours
