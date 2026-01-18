# NES Emulator Refactoring Opportunities

This document provides a comprehensive review of the codebase, identifying potential improvements, refactoring opportunities, and code smells organized by category.

---

## Table of Contents

1. [Ownership & Borrowing Clarity](#1-ownership--borrowing-clarity)
2. [Error Handling Quality](#2-error-handling-quality)
3. [Public API Design](#3-public-api-design)
4. [Trait Usage](#4-trait-usage)
5. [Enums Instead of Flags](#5-enums-instead-of-flags)
6. [Performance Hot Spots](#6-performance-hot-spots)
7. [Interior Mutability Patterns](#7-interior-mutability-patterns)
8. [Module Structure](#8-module-structure)
9. [Testing Quality](#9-testing-quality)
10. [Idiomatic Rust](#10-idiomatic-rust)

---

## 1. Ownership & Borrowing Clarity

### Current Issues Found

#### 1.1 Heavy Use of `Rc<RefCell<T>>` Pattern

**Location:** `src/nes.rs`, `src/cpu/cpu.rs`, `src/mem_controller.rs`

The codebase extensively uses `Rc<RefCell<T>>` for shared ownership between components:

```rust
// src/nes.rs
pub struct Nes {
    pub ppu: Rc<RefCell<ppu::Ppu>>,
    pub apu: Rc<RefCell<apu::Apu>>,
    pub memory: Rc<RefCell<mem_controller::MemController>>,
    // ...
}
```

**Why This Is a Smell:**
- Runtime borrow checking adds overhead
- Panic potential if borrow rules are violated at runtime
- Makes code harder to reason about

**Proposed Refactoring:**
- Consider an arena-based allocation pattern where the `Nes` struct owns all components directly
- Use explicit method parameters to pass references instead of shared ownership
- Investigate if the PPU and APU can take callbacks instead of holding references

#### 1.2 Cartridge Sharing Pattern

**Location:** `src/mem_controller.rs`, `src/ppu/ppu.rs`

```rust
// src/ppu/ppu.rs
cartridge: Option<Rc<RefCell<Cartridge>>>,
```

Both the MemController and PPU hold `Rc<RefCell<Cartridge>>` references.

**Proposed Refactoring:**
- Consider making the PPU take a trait object for CHR memory access instead of a full cartridge reference
- Create a `ChrMemory` trait that the mapper implements, reducing coupling

---


## 9. Testing Quality

### Current Issues Found

#### 9.1 Extensive Test Coverage ✓

The codebase has excellent test coverage with:
- Unit tests for individual components
- Integration tests using nestest.log
- Hardware behavior tests (VBlank timing, sprite 0 hit, etc.)

#### 9.2 Test Helpers Are Well-Organized

```rust
#[cfg(test)]
fn create_test_memory() -> MemController {
    let ppu = Rc::new(RefCell::new(ppu::Ppu::new(crate::nes::TvSystem::Ntsc)));
    let apu = Rc::new(RefCell::new(apu::Apu::new()));
    MemController::new(ppu, apu)
}
```

#### 9.3 Consider Property-Based Testing

**Proposed Enhancement:**
For thorough testing of edge cases, consider adding `proptest` or `quickcheck`:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_address_mirroring(addr in 0x0000u16..=0x1FFFu16) {
        let mut mem = create_test_memory();
        let expected_addr = addr & 0x07FF;
        mem.write(addr, 0x42, false);
        assert_eq!(mem.read(expected_addr), 0x42);
    }
}
```

#### 9.4 Missing Fuzzing Tests

Consider adding fuzz tests for:
- ROM parsing (`Cartridge::new`)
- CPU instruction decoding
- Memory access patterns

---

## 10. Idiomatic Rust

### Current Issues Found

#### 10.2 Manual Index Loops

**Location:** `src/ppu/ppu.rs`

```rust
for i in 0..8 {
    let value = self.sprites.read_oam((source_addr + i) as u8);
    self.sprites.write_oam(i as u8, value);
}
```

**Proposed Refactoring:**
Use iterator adapters where possible:

```rust
(0..8).for_each(|i| {
    let value = self.sprites.read_oam((source_addr + i) as u8);
    self.sprites.write_oam(i as u8, value);
});
```

Or even better, if the OAM had slice access:

```rust
let source = self.sprites.oam_slice(source_addr, 8);
self.sprites.write_oam_slice(0, source);
```

#### 10.3 Effective Use of `Option` Combinators ✓

```rust
self.cartridge
    .as_ref()
    .map(|cart| cart.borrow().mapper().read_prg(addr))
    .unwrap_or(0)
```

This is idiomatic Rust.

#### 10.4 Consider Using `let-else`

**Location:** Various

```rust
// Current pattern:
let Some(cartridge) = self.cartridge.as_ref() else {
    return Ok(());
};
```

This is already being used in newer code (good!):

```rust
// src/cartridge/cartridge.rs
let Some(save_path) = self.save_path.as_ref() else {
    return Ok(());
};
```

---

## Summary of High-Priority Refactorings

### 1. Error Handling (Medium Effort, High Impact)
- Create domain-specific error types using `thiserror`
- Replace panics in non-test code with proper `Result` handling

### 2. Public API Encapsulation (Medium Effort, Medium Impact)
- Make CPU fields private, expose through getters
- Remove public access to PPU registers

### 3. Reduce RefCell Usage (High Effort, Medium Impact)
- Consider making `MemController::read` take `&mut self`
- Investigate alternatives to `Rc<RefCell<T>>` pattern for component sharing

### 4. Ring Buffer for Audio (Low Effort, Low-Medium Impact)
- Replace `VecDeque` with a proper ring buffer crate

---

## Conclusion

The codebase is generally well-structured and follows many Rust best practices. The main areas for improvement are:

1. **Error handling** - Moving away from panics to proper error types
2. **Encapsulation** - Reducing public field exposure  
3. **Interior mutability** - Reducing `RefCell` usage where possible

The code demonstrates good understanding of NES hardware emulation and uses appropriate patterns for the domain (lookup tables, modular components, extensive testing).
