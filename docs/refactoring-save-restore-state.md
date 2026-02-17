# Save/Restore State Refactoring Proposal

**Issue**: [#592 - Refactor save and restore state](https://github.com/rmstdope/neser/issues/592)

**Author**: GitHub Copilot  
**Date**: 2026-02-17

## Executive Summary

After analyzing the save/restore state functionality and exploring key questions about the architecture, this proposal recommends a **simple, pragmatic refactoring**: move state struct definitions from `savestate.rs` to live alongside their component implementations.

**Key Finding**: Separate state structs are necessary (components contain non-serializable types), but they don't need to be centralized in `savestate.rs`.

**Recommendation**: **Option 1 - Move State Structs** (2-3 hours, very low risk)
- Move each state struct definition to its component file
- Update imports in `savestate.rs`
- No behavioral changes, just reorganization

**Why Not Traits?**: The original proposal was over-engineered. Traits add complexity without enough benefit. The current pattern works well; state structs just need to be in the right place.

---

## Current State Analysis

### Architecture Overview

The current save/restore state implementation follows these patterns:

1. **State Structs (Centralized)**: All state structs (CpuState, PpuState, ApuState, etc.) are defined in `src/console/savestate.rs`
   - **Why separate structs?** Component structs (like `Cpu`) contain non-serializable types (`Rc<RefCell<>>`, references) that can't be directly serialized
   - **Why centralized?** Historical choice - could be distributed alongside components
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

## Key Questions & Answers

### Q1: Why do we need separate State structs?

**Answer**: The component structs contain non-serializable types that prevent direct serialization.

**Example**:
```rust
// The Cpu struct cannot be serialized directly
pub struct Cpu {
    a: u8,
    x: u8,
    // ❌ These types don't implement Serialize
    bus: Rc<RefCell<Bus>>,
    ppu: Rc<RefCell<Ppu>>,
    apu: Rc<RefCell<Apu>>,
    master_clock: MasterClock,
    // ... more fields
}
```

**Alternatives explored**:
1. **Derive Serialize on main structs** - Not feasible due to `Rc`, `RefCell`, and cross-references
2. **Use custom Serialize impl** - Possible but complex, would need to skip non-data fields
3. **Separate state structs** - Current approach, clean separation of data vs references

**Conclusion**: Separate state structs are necessary for clean serialization.

### Q2: Do state structs need to be centralized in savestate.rs?

**Answer**: No! They could be defined alongside their components.

**Current pattern** (centralized):
```rust
// src/console/savestate.rs
pub struct CpuState { /* ... */ }
pub struct PpuState { /* ... */ }

// src/cpu/cpu.rs
use crate::console::CpuState;
impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
}
```

**Alternative pattern** (distributed):
```rust
// src/cpu/cpu.rs
#[derive(Serialize, Deserialize)]
pub struct CpuState { /* ... */ }

pub struct Cpu { /* ... */ }

impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
}

// src/console/savestate.rs
use crate::cpu::CpuState;
use crate::ppu::PpuState;

pub struct SaveState {
    pub cpu: CpuState,
    pub ppu: PpuState,
    // ...
}
```

**Pros of distributed approach**:
- ✅ State struct right next to component struct
- ✅ Less context switching when modifying
- ✅ Better encapsulation
- ✅ Each module owns its state definition

**Cons of distributed approach**:
- ⚠️ Serialization format more distributed
- ⚠️ Harder to see full save-state format at once

**Conclusion**: Distributing state structs alongside components is likely better! This addresses the original concern.

### Q3: Can we simplify further?

**Explored approaches**:

1. **Directly serialize component fields** - Not feasible due to non-serializable types
2. **Use serde's skip attribute** - Would make restore complex, need to track skipped fields
3. **Builder pattern** - Adds complexity without clear benefit
4. **Current approach** - Actually reasonably simple given constraints

**Conclusion**: The current pattern of separate state structs with capture/restore methods is a good solution. The main improvement is to move state structs next to their components.

---

## Identified Issues (Revised)

### 1. State Structs Separated from Components

**Problem**: State structs live in `savestate.rs` while the logic to populate/restore them lives with each component.

**Impact**:
- Split context when modifying state
- Need to edit multiple files for state changes
- Potential for struct/implementation mismatch

**Solution**: Move state structs to live alongside their components.

### 2. No Enforced Interface (Low Priority)

**Problem**: There's no trait requiring components to implement state management.

**Impact**:
- Easy to forget implementing state for new components
- Hard to ensure consistency across implementations

**Note**: This is less critical than initially thought. The pattern is clear and tests would catch missing implementations.

### 3. Manual Hierarchical Orchestration (Not Really an Issue)

**Current pattern**: Each parent component manually orchestrates child state.

**Example from PPU**:
```rust
pub fn capture_state(&self) -> PpuState {
    let bg_state = self.background.capture_state();
    let sprites_state = self.sprites.capture_state();
    PpuState {
        // ... map fields from bg_state and sprites_state
    }
}
```

**Assessment**: This is actually reasonable and explicit. It's not broken, just verbose. The hierarchical pattern makes the structure clear.

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

## Revised Proposed Solutions

### Option 1: Move State Structs to Components (⭐ RECOMMENDED - Simple & Effective)

**Concept**: Keep the current pattern but move state struct definitions alongside their components.

**Changes**:
```rust
// BEFORE: State in savestate.rs
// src/console/savestate.rs
pub struct CpuState { pub a: u8, /* ... */ }

// src/cpu/cpu.rs  
use crate::console::CpuState;
impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
}

// AFTER: State alongside component
// src/cpu/cpu.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState { pub a: u8, /* ... */ }

pub struct Cpu { /* ... */ }

impl Cpu {
    pub fn capture_state(&self) -> CpuState { /* ... */ }
    pub fn restore_state(&mut self, state: &CpuState) { /* ... */ }
}

// src/console/savestate.rs (just aggregates)
use crate::cpu::CpuState;
use crate::ppu::PpuState;
// ...

pub struct SaveState {
    pub version: u32,
    pub cpu: CpuState,
    pub ppu: PpuState,
    // ...
}
```

#### Pros:
- ✅ State struct right next to component - single file to edit
- ✅ Better encapsulation - each module owns its state
- ✅ Minimal code changes - just move struct definitions
- ✅ No breaking changes to serialization format
- ✅ Addresses the main concern from the issue
- ✅ Simple to implement (few hours)

#### Cons:
- ⚠️ SaveState format slightly more distributed (but still aggregated in one place)
- ⚠️ Need to update imports

**Effort**: 2-3 hours  
**Risk**: Very Low

---

### Option 2: Add Trait + Move State Structs

**Concept**: Move state structs AND add a trait for consistency.

```rust
// src/cpu/cpu.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState { /* ... */ }

impl Stateful for Cpu {
    type State = CpuState;
    fn capture_state(&self) -> CpuState { /* ... */ }
    fn restore_state(&mut self, state: &CpuState) { /* ... */ }
}
```

#### Pros:
- ✅ All benefits of Option 1
- ✅ Compile-time enforcement via trait
- ✅ Better discoverability

#### Cons:
- ⚠️ More work to implement
- ⚠️ Adds complexity (trait bounds, associated types)
- ⚠️ Benefit over Option 1 is marginal

**Effort**: 6-10 hours  
**Risk**: Low-Medium

---

### Option 3: Keep Current (Minimal Changes)

**Concept**: Keep everything as-is, just add documentation.

#### Pros:
- ✅ Zero effort
- ✅ Zero risk
- ✅ Already works

#### Cons:
- ❌ Doesn't address the original concern about separation

**Effort**: 1 hour (documentation only)  
**Risk**: None

---

## Recommendation: Option 1

After exploring the questions raised, **Option 1 (Move State Structs)** is the clear winner:

1. **Directly addresses the concern**: State structs next to their components
2. **Simple**: Just move struct definitions, update imports
3. **Low risk**: No behavioral changes, just reorganization
4. **Minimal effort**: 2-3 hours
5. **No complexity added**: No traits, no new patterns

The original proposal (Option 3 from old version) was over-engineered. The trait adds complexity without enough benefit. Simply moving the state structs alongside their components solves the main problem elegantly.

---

## Implementation Plan (Option 1)

### Phase 1: Move State Structs (2-3 hours)

For each component:

1. **Move state struct definition** from `src/console/savestate.rs` to component file
2. **Update imports** in `savestate.rs` to use component's exported state
3. **Run tests** to ensure serialization still works
4. **Commit** each component separately

**Example for CPU**:

```rust
// STEP 1: Move definition to src/cpu/cpu.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    // ... all fields
}

// STEP 2: Update src/console/savestate.rs
use crate::cpu::CpuState;  // Import instead of define

pub struct SaveState {
    pub cpu: CpuState,  // Use imported type
    // ...
}
```

**Order of migration** (one at a time):
1. CPU (simplest)
2. Bus
3. APU sub-components (Envelope, Pulse, Triangle, Noise, DMC)
4. APU
5. PPU sub-components (Background, Sprites)  
6. PPU

**After each move**: Run tests, commit with message like "Move CpuState to cpu.rs"

### Phase 2: Documentation (30 minutes)

1. Update module docs to explain the pattern
2. Add comment in savestate.rs explaining it aggregates component states

**Total Effort**: 2-3 hours  
**Risk**: Very Low

---

## Alternative: Keep Current Implementation

If even this simple reorganization isn't worth the effort, the current implementation works fine!

### What's Good About Current Implementation:
- ✅ Clear separation: state definitions vs. behavior
- ✅ Hierarchical pattern is explicit and understandable
- ✅ Tests prove it works reliably
- ✅ Localized implementation logic

### Minimal Improvements (if keeping current):
1. **Documentation**: Add comments linking state structs to their implementations
2. **Checklist**: Create a checklist for adding new stateful components  
3. **Integration Test**: Add test that serializes/deserializes full state

Example documentation improvement:
```rust
/// CPU register and internal state.
///
/// Implementation: `src/cpu/cpu.rs::Cpu::capture_state()` and `restore_state()`
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    // ...
}
```

---

## Decision Criteria

### Choose Option 1 (Move State Structs) if:
- ✅ You value having state definitions close to implementations
- ✅ The 2-3 hour investment is acceptable
- ✅ You want simpler, more maintainable code organization
- ✅ You agree the current split is a bit awkward

### Choose Option 2 (Add Trait) if:
- ✅ All of the above, AND
- ✅ You want compile-time enforcement
- ✅ The 6-10 hour investment is acceptable
- ✅ You value consistency and discoverability

### Keep Current if:
- ✅ The current implementation works well enough
- ✅ Even a few hours isn't worth it right now
- ✅ You prefer stability over improvement
- ✅ Other priorities are more important

---

## My Revised Assessment

After exploring the navigator's questions, my assessment has changed:

**Original Proposal**: Over-engineered. Trait-based approach adds complexity without commensurate benefit.

**Revised Recommendation**: **Option 1 (Move State Structs)** - Simple, addresses the concern, low effort.

**Key Insight**: The current pattern isn't broken, it's just that state structs are in the wrong place. Moving them alongside their components is a simple, effective improvement.

**Why Not Traits?**: 
- The current pattern is clear and explicit
- Tests would catch missing implementations
- Traits add complexity (bounds, associated types) for marginal benefit
- Keep it simple

---

## Next Steps

### If Approved (Option 1):
1. ✅ Review and approve this revised proposal
2. 🚀 Move state structs one component at a time
3. 🧪 Test after each move
4. 📚 Update documentation

### If Declined:
1. ✅ Keep current implementation
2. 📝 Optionally add documentation improvements
3. ✅ Close issue with rationale

---

## References

- **Issue**: [#592 - Refactor save and restore state](https://github.com/rmstdope/neser/issues/592)
- **Related Pattern**: `MapperStateSnapshot` trait in `src/cartridge/mapper.rs`
- **Current Implementation**: `src/console/savestate.rs` and component files

---

## Appendix: Code Examples

### Current Pattern (Before)

```rust
// src/console/savestate.rs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    // ... many fields
}

// src/cpu/cpu.rs
use crate::console::CpuState;

pub struct Cpu { /* ... */ }

impl Cpu {
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            // ...
        }
    }
    
    pub fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        // ...
    }
}
```

### Proposed Pattern (Option 1 - After)

```rust
// src/cpu/cpu.rs
use serde::{Serialize, Deserialize};

/// CPU state for save/restore
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    // ... many fields
}

/// NES 6502 CPU
pub struct Cpu { /* ... */ }

impl Cpu {
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            // ...
        }
    }
    
    pub fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        // ...
    }
}

// src/console/savestate.rs
use crate::cpu::CpuState;  // Import from component
use crate::ppu::PpuState;
// ...

/// Complete emulator save state
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveState {
    pub version: u32,
    pub cpu: CpuState,
    pub ppu: PpuState,
    // ...
}
```

**Key Change**: State struct moved from `savestate.rs` to `cpu.rs`, `savestate.rs` just imports and aggregates.

---

**End of Proposal (Revised)**
---

**End of Proposal**
