# Save/Restore State Refactoring Proposal

**Issue**: [#592 - Refactor save and restore state](https://github.com/rmstdope/neser/issues/592)

**Author**: GitHub Copilot  
**Date**: 2026-02-17

## Executive Summary

This document proposes a **hybrid trait-based approach** to refactor the save/restore state functionality. The proposal introduces a `Stateful` trait while keeping state structs centralized, providing better compile-time safety and consistency with minimal code disruption.

**Recommendation**: Adopt the hybrid approach (Option 3) as it provides the best balance of maintainability, safety, and minimal risk.

---

## Current State Analysis

### Architecture Overview

The current save/restore state implementation follows these patterns:

1. **State Structs (Centralized)**: All state structs (CpuState, PpuState, ApuState, etc.) are defined in `src/console/savestate.rs`
2. **Capture/Restore Methods (Distributed)**: Each component implements `capture_state()` and `restore_state()` methods near the component itself
3. **Hierarchical Orchestration**: Each component manually orchestrates its sub-components' state capture/restore
4. **Top-level Integration**: The NES struct coordinates all top-level components

### Components with State Management

#### Core Components (14 implementations found):

- **CPU** (`src/cpu/cpu.rs`)
- **PPU** (`src/ppu/ppu.rs`)
  - Background (`src/ppu/background.rs`)
  - Sprites (`src/ppu/sprites.rs`)
- **APU** (`src/apu/apu.rs`)
  - Pulse channels x2 (`src/apu/pulse.rs`)
  - Triangle (`src/apu/triangle.rs`)
  - Noise (`src/apu/noise.rs`)
  - DMC (`src/apu/dmc.rs`)
  - Envelope (`src/apu/envelope.rs`)
- **Bus** (`src/bus/bus.rs`)
- **Input Controllers**:
  - Joypad (`src/input/nes_joypad.rs`)
  - Arkanoid (`src/input/arkanoid_controller.rs`)
  - Zapper (`src/input/zapper.rs`)

#### Special Cases:

- **Mappers** use a different trait-based approach (`MapperStateSnapshot` trait in `src/cartridge/mapper.rs`)
- **Controllers** have both type-specific and generic trait methods

### Current Implementation Example

```rust
// State struct in savestate.rs
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    // ... more fields
}

// Implementation in cpu.rs
impl Cpu {
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            // ... populate all fields
        }
    }
    
    pub fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        // ... restore all fields
    }
}
```

---

## Identified Issues

### 1. Separation of Concerns

**Problem**: State structs live in `savestate.rs` while the logic to populate/restore them lives with each component.

**Impact**:
- Split context when modifying state
- Need to edit multiple files for state changes
- Potential for struct/implementation mismatch
- Harder to ensure all fields are properly handled

### 2. No Enforced Interface

**Problem**: There's no trait requiring components to implement state management.

**Impact**:
- Easy to forget implementing state for new components
- Hard to ensure consistency across implementations
- No compile-time guarantee of completeness
- Difficult to discover which components are stateful

### 3. Manual Hierarchical Orchestration

**Problem**: Each parent component must manually orchestrate child state.

**Example from PPU**:
```rust
pub fn capture_state(&self) -> PpuState {
    let bg_state = self.background.capture_state();      // Manual call
    let sprites_state = self.sprites.capture_state();    // Manual call
    PpuState {
        // ... manually map 50+ fields from bg_state and sprites_state
        bg_pattern_shift_lo: bg_state.bg_pattern_shift_lo,
        bg_pattern_shift_hi: bg_state.bg_pattern_shift_hi,
        // ... many more manual mappings
    }
}
```

**Impact**:
- Verbose and error-prone code
- Easy to miss fields during updates
- Duplication of field access logic

### 4. Inconsistent Patterns

**Problem**: Different patterns for different component types.

**Examples**:
- Most components use direct methods
- Mappers use a trait (`MapperStateSnapshot`)
- Controllers implement both specific and generic methods

**Impact**:
- Confusion about which pattern to use
- Harder to learn the codebase
- Inconsistent code style

---

## Proposed Solutions

### Option 1: Pure Trait-Based Hierarchical

**Concept**: Define a `Stateful` trait that all components implement:

```rust
pub trait Stateful {
    type State: Serialize + Deserialize;
    fn capture_state(&self) -> Self::State;
    fn restore_state(&mut self, state: &Self::State);
}
```

#### Pros:
- ✅ Enforces implementation at compile time
- ✅ Type-safe state associations
- ✅ Consistent interface across all components
- ✅ Easy to discover what components have state

#### Cons:
- ❌ Requires associated types for each component
- ❌ May complicate generic code
- ❌ State structs still somewhat distant from implementations

---

### Option 2: Keep Current Pattern, Add Documentation

**Concept**: Keep the current implementation but improve it with:
- Better documentation
- Naming conventions
- Helper macros for common patterns
- Integration tests

#### Pros:
- ✅ Minimal code changes
- ✅ No learning curve
- ✅ Working implementation
- ✅ Zero migration risk

#### Cons:
- ❌ Doesn't solve fundamental architectural issues
- ❌ No enforcement of consistency
- ❌ Technical debt remains
- ❌ Still easy to forget implementations

---

### Option 3: Hybrid Approach (⭐ RECOMMENDED)

**Concept**: Combine the best of both approaches:

1. Keep state structs in `savestate.rs` (centralized serialization format)
2. Introduce a `Stateful` trait for consistency and discoverability
3. Keep implementation near components (but enforce via trait)
4. Use consistent hierarchical call chains

#### Structure:

```rust
// In savestate.rs - all state types and the trait
pub trait Stateful {
    type State: Serialize + Deserialize;
    fn capture_state(&self) -> Self::State;
    fn restore_state(&mut self, state: &Self::State);
}

// In each component file - implement the trait
impl Stateful for Cpu {
    type State = CpuState;
    
    fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            // ... existing logic
        }
    }
    
    fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        // ... existing logic
    }
}
```

#### Pros:
- ✅ Enforces implementation via trait
- ✅ State structs remain centralized for serialization
- ✅ Implementation stays near component
- ✅ Type-safe and discoverable
- ✅ Minimal code changes (mostly adding trait impls)
- ✅ Consistent with existing mapper pattern
- ✅ Can migrate incrementally
- ✅ Backwards compatible during migration

#### Cons:
- ⚠️ Still some separation between struct and implementation
- ⚠️ Requires adding trait implementations to existing code
- ⚠️ Small one-time migration effort

---

## Recommendation: Adopt Option 3

**Rationale**:

1. **Cleaner Architecture**: Trait enforcement prevents missing implementations while maintaining clear structure
2. **Easier Maintenance**: Clear contract for state management makes future changes safer
3. **Minimal Disruption**: Existing code mostly stays the same, just gains trait implementations
4. **Better Discoverability**: Easy to find all stateful components via trait implementations
5. **Consistency**: Similar to existing mapper pattern, reducing cognitive load
6. **Incremental Migration**: Can migrate one component at a time without breaking anything
7. **Future-Proof**: New components must implement the trait, preventing oversight

---

## Implementation Plan

### Phase 1: Define Trait (Low Risk) ⏱️ 1-2 hours

**Tasks**:
1. Add `Stateful` trait to `src/console/savestate.rs`
2. Add comprehensive documentation with examples
3. Add unit tests for trait behavior
4. No breaking changes yet - purely additive

**Deliverables**:
```rust
/// Trait for components that support save-state capture and restoration.
///
/// Implement this trait for any component that needs to be included in
/// save-state snapshots. The trait ensures consistent state management
/// across all emulator components.
pub trait Stateful {
    /// The state type for this component.
    type State: Serialize + for<'de> Deserialize<'de>;
    
    /// Capture the current state of this component.
    fn capture_state(&self) -> Self::State;
    
    /// Restore this component's state from a snapshot.
    fn restore_state(&mut self, state: &Self::State);
}
```

**Risk**: None - purely additive change

---

### Phase 2: Migrate Core Components (Medium Risk) ⏱️ 4-6 hours

**Order of Migration**:
1. CPU (simplest, no sub-components)
2. Bus (moderate complexity)
3. APU channels (Pulse, Triangle, Noise, DMC, Envelope)
4. APU (aggregates channels)
5. PPU sub-components (Background, Sprites)
6. PPU (aggregates sub-components)

**For each component**:
1. Add `impl Stateful for Component`
2. Move existing `capture_state()` and `restore_state()` into trait impl
3. Run component-specific tests
4. Verify state serialization/deserialization works
5. Commit

**Example Migration**:
```rust
// Before
impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
    pub fn restore_state(&mut self, state: &CpuState) { /* ... */ }
}

// After
impl Stateful for Cpu {
    type State = CpuState;
    fn capture_state(&self) -> CpuState { /* ... */ }
    fn restore_state(&mut self, state: &CpuState) { /* ... */ }
}
```

**Risk**: Medium
- Could break compilation if trait bounds are incorrect
- Need to verify all tests still pass
- Mitigated by: incremental migration with tests after each component

---

### Phase 3: Migrate Input Controllers (Low Risk) ⏱️ 2-3 hours

**Components**:
1. Joypad
2. Arkanoid
3. Zapper

**Special Consideration**: These already implement `ControllerDevice` trait. May need to:
- Keep both trait implementations
- Or refactor `ControllerDevice` to use `Stateful`

**Risk**: Low
- Well-isolated components
- Good test coverage
- Can fall back if issues arise

---

### Phase 4: Documentation & Cleanup ⏱️ 2-3 hours

**Tasks**:
1. Update module documentation to explain the pattern
2. Add examples for future components
3. Document migration path for contributors
4. Add integration test that verifies all components implement `Stateful`
5. Update CONTRIBUTING.md if it exists

**Deliverables**:
- Updated documentation
- Pattern examples
- Checklist for adding new stateful components

**Risk**: None

---

## Total Effort Estimate

- **Phase 1**: 1-2 hours
- **Phase 2**: 4-6 hours
- **Phase 3**: 2-3 hours
- **Phase 4**: 2-3 hours

**Total**: 9-14 hours of development time

---

## Alternative: Keep Current Implementation

If the proposed refactoring seems too invasive or the effort isn't justified, the current implementation is actually **reasonably well-structured**:

### What's Good About Current Implementation:
- ✅ Clear separation: state definitions vs. behavior
- ✅ Hierarchical pattern is explicit and understandable
- ✅ Tests prove it works reliably
- ✅ Localized implementation logic

### Minimal Improvements (1-2 hours):
If choosing not to refactor, consider these low-effort improvements:

1. **Documentation**: Add comprehensive module docs explaining the pattern
2. **Checklist**: Create a checklist for adding new stateful components
3. **Integration Test**: Add test that serializes/deserializes full state
4. **Comments**: Add comments in `savestate.rs` linking to implementations

Example:
```rust
/// CPU register and internal state.
///
/// Captured and restored by: `src/cpu/cpu.rs::Cpu::capture_state()`
/// and `src/cpu/cpu.rs::Cpu::restore_state()`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    // ...
}
```

---

## Decision Criteria

### Choose Option 3 (Hybrid) if:
- ✅ You plan to add more stateful components in the future
- ✅ You want compile-time safety and enforcement
- ✅ The 9-14 hour investment is acceptable
- ✅ You value consistency and maintainability
- ✅ You want better discoverability

### Choose Option 2 (Keep Current) if:
- ✅ The current implementation works well enough
- ✅ Development resources are very limited
- ✅ The codebase is relatively stable (few new components planned)
- ✅ The team prefers minimal changes
- ✅ Risk avoidance is the priority

---

## Questions for Decision

Before proceeding, please consider:

1. **Migration Risk**: Is the 9-14 hour investment worth the architectural improvement?
2. **Team Capacity**: Is this the right time to do this refactoring?
3. **Future Plans**: Are many new stateful components planned?
4. **Current Pain**: Is the current pattern causing actual maintenance problems?
5. **Priority**: How does this compare to other technical debt items?

---

## My Assessment

As the analyzing agent, my assessment is:

**Current Implementation**: Functional but improvable  
**Proposed Refactoring**: Would provide meaningful benefits  
**Risk**: Low to Medium (mitigated by incremental approach)  
**Recommendation**: **Adopt Option 3** - the benefits outweigh the migration cost

However, **only proceed if you agree** that the improved maintainability, safety, and consistency are worth the development time investment.

---

## Next Steps

### If Approved (Option 3):
1. ✅ Review and approve this proposal
2. 📋 Create sub-issues for each phase (optional)
3. 🚀 Begin Phase 1: Define trait
4. 🧪 Migrate components incrementally with testing
5. 📚 Complete documentation

### If Declined (Option 2):
1. ✅ Review and approve keeping current implementation
2. 📝 Add documentation improvements
3. ✅ Close issue with rationale
4. 📋 Create follow-up issue if circumstances change

---

## References

- **Issue**: [#592 - Refactor save and restore state](https://github.com/rmstdope/neser/issues/592)
- **Related Pattern**: `MapperStateSnapshot` trait in `src/cartridge/mapper.rs`
- **Current Implementation**: `src/console/savestate.rs` and component files

---

## Appendix: Code Examples

### Current Pattern

```rust
// savestate.rs
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    // ...
}

// cpu.rs
impl Cpu {
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            // ...
        }
    }
}
```

### Proposed Pattern (Option 3)

```rust
// savestate.rs
pub trait Stateful {
    type State: Serialize + for<'de> Deserialize<'de>;
    fn capture_state(&self) -> Self::State;
    fn restore_state(&mut self, state: &Self::State);
}

pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    // ...
}

// cpu.rs
impl Stateful for Cpu {
    type State = CpuState;
    
    fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            // ...
        }
    }
    
    fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        // ...
    }
}
```

### Using the Trait

```rust
// Generic function that works with any Stateful component
fn test_state_roundtrip<T: Stateful>(component: &mut T) {
    let state = component.capture_state();
    component.restore_state(&state);
    let restored = component.capture_state();
    assert_eq!(
        serde_json::to_string(&state).unwrap(),
        serde_json::to_string(&restored).unwrap()
    );
}
```

---

**End of Proposal**
