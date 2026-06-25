# Architecture Diagrams: Current vs Proposed

> **Implementation status (#2834 / I2.1):** The `Stateful` trait described below is
> **implemented** in [`src/platform/save_state.rs`](../src/platform/save_state.rs) (not the
> hypothetical `src/console/savestate.rs` used in the diagrams). The shipped design:
> - `Stateful` is a component-level trait with an associated `State: Serialize + DeserializeOwned`
>   and an **infallible** `restore_state`; fallible validation (format version, NES mapper / SNES
>   ROM identity, memory-region sizes) lives at the console boundary.
> - A shared, console-agnostic `SaveStateError` plus generic `to_bytes` / `from_bytes` and a
>   `check_version` helper replace the four hand-rolled per-console error/serialize/version blocks.
> - All four cores assemble their top-level save-state through the trait: NES `Cpu`/`Ppu`/`Apu`/`Bus`,
>   GB `Sm83`, GBA `Arm7tdmi`, and SNES `Cpu`. Console-specific errors (NES `MapperMismatch`, SNES
>   `RomMismatch`) remain in slim per-console enums that convert `From<SaveStateError>`.
> - The on-disk format is unchanged (still JSON); a postcard migration is tracked separately (I2.2).

## Current Architecture

```
┌─────────────────────────────────────────────────────────┐
│         src/console/savestate.rs                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │ CpuState, PpuState, ApuState, BusState, etc.    │   │
│  │ (State Structs - Centralized)                   │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
                           │
                           │ defined here, used elsewhere
                           ▼
┌─────────────────────────────────────────────────────────┐
│                  Component Files                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  src/cpu/    │  │  src/ppu/    │  │  src/apu/    │  │
│  │  cpu.rs      │  │  ppu.rs      │  │  apu.rs      │  │
│  │              │  │              │  │              │  │
│  │ impl Cpu {   │  │ impl Ppu {   │  │ impl Apu {   │  │
│  │   pub fn     │  │   pub fn     │  │   pub fn     │  │
│  │   capture_   │  │   capture_   │  │   capture_   │  │
│  │   state()    │  │   state()    │  │   state()    │  │
│  │              │  │              │  │              │  │
│  │   pub fn     │  │   pub fn     │  │   pub fn     │  │
│  │   restore_   │  │   restore_   │  │   restore_   │  │
│  │   state()    │  │   state()    │  │   state()    │  │
│  │ }            │  │ }            │  │ }            │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  (Implementations - Distributed, Not Enforced)          │
└─────────────────────────────────────────────────────────┘

Issues:
❌ No compile-time enforcement
❌ Easy to forget implementing for new components
❌ State structs far from implementations
❌ Manual orchestration required
```

## Proposed Architecture (Hybrid Approach)

```
┌─────────────────────────────────────────────────────────┐
│         src/console/savestate.rs                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │ pub trait Stateful {                            │   │
│  │     type State: Serialize + Deserialize;        │   │
│  │     fn capture_state(&self) -> Self::State;     │   │
│  │     fn restore_state(&mut self, &Self::State);  │   │
│  │ }                                               │   │
│  └─────────────────────────────────────────────────┘   │
│  ┌─────────────────────────────────────────────────┐   │
│  │ CpuState, PpuState, ApuState, BusState, etc.    │   │
│  │ (State Structs - Centralized)                   │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
                           │
                           │ enforced via trait
                           ▼
┌─────────────────────────────────────────────────────────┐
│                  Component Files                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  src/cpu/    │  │  src/ppu/    │  │  src/apu/    │  │
│  │  cpu.rs      │  │  ppu.rs      │  │  apu.rs      │  │
│  │              │  │              │  │              │  │
│  │ impl         │  │ impl         │  │ impl         │  │
│  │ Stateful     │  │ Stateful     │  │ Stateful     │  │
│  │ for Cpu {    │  │ for Ppu {    │  │ for Apu {    │  │
│  │   type State │  │   type State │  │   type State │  │
│  │   = CpuState;│  │   = PpuState;│  │   = ApuState;│  │
│  │              │  │              │  │              │  │
│  │   fn         │  │   fn         │  │   fn         │  │
│  │   capture_   │  │   capture_   │  │   capture_   │  │
│  │   state() {  │  │   state() {  │  │   state() {  │  │
│  │     // ...   │  │     // ...   │  │     // ...   │  │
│  │   }          │  │   }          │  │   }          │  │
│  │              │  │              │  │              │  │
│  │   fn         │  │   fn         │  │   fn         │  │
│  │   restore_   │  │   restore_   │  │   restore_   │  │
│  │   state() {  │  │   state() {  │  │   state() {  │  │
│  │     // ...   │  │     // ...   │  │     // ...   │  │
│  │   }          │  │   }          │  │   }          │  │
│  │ }            │  │ }            │  │ }            │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  (Trait Implementations - Distributed, ENFORCED)        │
└─────────────────────────────────────────────────────────┘

Benefits:
✅ Compile-time enforcement via trait
✅ Discoverable (can list all Stateful implementors)
✅ Type-safe with associated types
✅ Implementations still near components
✅ State structs centralized for serialization
```

## Comparison: Feature by Feature

| Feature | Current | Proposed |
|---------|---------|----------|
| **State Structs** | Centralized ✅ | Centralized ✅ |
| **Implementations** | Distributed ✅ | Distributed ✅ |
| **Compile-time Safety** | ❌ No trait | ✅ Trait enforced |
| **Discoverability** | ⚠️ Manual search | ✅ Via trait |
| **Type Safety** | ⚠️ Convention | ✅ Associated types |
| **Consistency** | ⚠️ Manual | ✅ Enforced |
| **Ease of Adding New Component** | ⚠️ Easy to forget | ✅ Compiler reminds |
| **Code Location** | ✅ Near component | ✅ Near component |
| **Serialization Format** | ✅ Centralized | ✅ Centralized |

## Example: Adding a New Component

### Current Approach

```rust
// 1. Define state struct in savestate.rs
pub struct NewComponentState {
    pub field1: u8,
    pub field2: u16,
}

// 2. Remember to add capture_state() in component file
// ❌ Easy to forget! No compiler error if you don't
impl NewComponent {
    pub fn capture_state(&self) -> NewComponentState {
        // ...
    }
    
    pub fn restore_state(&mut self, state: &NewComponentState) {
        // ...
    }
}

// 3. Remember to call it from parent component
// ❌ Easy to forget! No compiler error if you don't
```

### Proposed Approach

```rust
// 1. Define state struct in savestate.rs (same as before)
pub struct NewComponentState {
    pub field1: u8,
    pub field2: u16,
}

// 2. Implement Stateful trait in component file
// ✅ Compiler enforces this!
impl Stateful for NewComponent {
    type State = NewComponentState;
    
    fn capture_state(&self) -> NewComponentState {
        // ✅ Must implement or won't compile
    }
    
    fn restore_state(&mut self, state: &NewComponentState) {
        // ✅ Must implement or won't compile
    }
}

// 3. Call from parent component
// ✅ Type-safe, discoverable through trait
```

## Migration Path

```
Phase 1: Add Trait (No Breaking Changes)
┌──────────────────────────────────────────┐
│ Add Stateful trait to savestate.rs       │
│ ✓ Backwards compatible                   │
│ ✓ Existing code still works              │
│ ✓ Can use trait for new components       │
└──────────────────────────────────────────┘
              ↓
Phase 2: Migrate Core Components (One by One)
┌──────────────────────────────────────────┐
│ For each component:                      │
│   1. Add impl Stateful for Component     │
│   2. Move existing methods into impl     │
│   3. Run tests                           │
│   4. Commit                              │
│ ✓ Incremental                            │
│ ✓ Can rollback any step                  │
└──────────────────────────────────────────┘
              ↓
Phase 3: Migrate Input Controllers
┌──────────────────────────────────────────┐
│ Same process for Joypad, Arkanoid, etc. │
│ ✓ Well-isolated components               │
└──────────────────────────────────────────┘
              ↓
Phase 4: Documentation
┌──────────────────────────────────────────┐
│ Document pattern for future contributors │
│ Add examples and guidelines              │
└──────────────────────────────────────────┘
```

## Risk Mitigation

```
Low Risk Factors:
✅ Trait is purely additive in Phase 1
✅ Existing code continues to work during migration
✅ Each component migrated independently
✅ Tests run after each component
✅ Can rollback individual components if issues
✅ Similar to existing MapperStateSnapshot pattern

Medium Risk Factors:
⚠️ Need to verify trait bounds work correctly
⚠️ May need to adjust generic code
⚠️ Integration with ControllerDevice trait

Mitigation Strategy:
1. Start with simplest component (CPU)
2. Test thoroughly before moving to next
3. Have fallback plan for each phase
4. Keep existing methods during transition
5. Remove old code only after full verification
```

## Alignment with Existing Patterns

The proposed approach is **consistent** with existing patterns in the codebase:

```rust
// Mappers already use a trait-based pattern!
pub trait MapperStateSnapshot {
    fn prg_ram_snapshot(&self) -> Vec<u8>;
    fn chr_ram_snapshot(&self) -> Vec<u8>;
    fn registers_snapshot(&self) -> Vec<u8>;
    fn restore_prg_ram(&mut self, data: &[u8]);
    fn restore_chr_ram(&mut self, data: &[u8]);
    fn restore_registers(&mut self, data: &[u8]);
}

// Proposed Stateful trait follows similar pattern
pub trait Stateful {
    type State: Serialize + Deserialize;
    fn capture_state(&self) -> Self::State;
    fn restore_state(&mut self, state: &Self::State);
}
```

This shows the codebase already embraces trait-based patterns for state management!

---

**Conclusion**: The hybrid approach provides the best balance of safety, maintainability, and minimal disruption while aligning with existing architectural patterns in the codebase.
