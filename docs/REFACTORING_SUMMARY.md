# Save/Restore State Refactoring - Summary (Final)

**Issue**: [#592](https://github.com/rmstdope/neser/issues/592)  
**Status**: Approved Approach Finalized  
**Date**: 2026-02-17 (Updated after navigator approval)

## Final Approved Approach

**Remove `savestate.rs` entirely** - handle all serialization in components and NES struct.

### What This Means

**No more centralized SaveState struct!** Instead:
1. Each component defines its own state struct (e.g., `CpuState` in `cpu.rs`)
2. NES struct serializes directly to/from bytes using serde_json
3. No intermediate aggregator struct needed

### Architecture

```rust
// Each component owns its state
// src/cpu/cpu.rs
#[derive(Serialize, Deserialize)]
pub struct CpuState { /* ... */ }

// NES handles serialization directly
// src/console/nes.rs
impl Nes {
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let state = serde_json::json!({
            "version": 8,
            "cpu": self.cpu.capture_state(),
            "ppu": self.ppu.borrow().capture_state(),
            // ...
        });
        serde_json::to_vec(&state)
    }
}
```

### Benefits

- ✅ Simpler - no separate module
- ✅ Better encapsulation - components own their state
- ✅ Clearer ownership - state lives with implementation
- ✅ Less indirection - direct bytes serialization

### Changes Required

1. Move state structs to component files (CpuState → cpu.rs, etc.)
2. Remove `savestate.rs` completely
3. Update NES to serialize/deserialize directly to/from bytes
4. Update callers (wasm, sdl_frontend, main) to use bytes API
5. Move SaveStateError and SAVESTATE_VERSION to nes.rs

### Implementation Estimate

**3-4 hours** for complete refactoring:
- Move structs: 1 hour
- Update NES serialization: 1 hour  
- Update callers: 30 min
- Testing: 1-1.5 hours

---

## Evolution of This Proposal

1. **Original**: Add traits for compile-time enforcement (over-engineered)
2. **Revised**: Just move structs to components, keep savestate.rs (simpler)
3. **Final (Approved)**: Remove savestate.rs entirely (simplest)

The navigator's iterative feedback led to progressively simpler solutions!
