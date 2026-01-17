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

## 2. Error Handling Quality

### Current Issues Found

#### 2.1 Use of `unwrap()` and `expect()` in Non-Test Code

**Location:** Various files

```rust
// src/cartridge/cartridge.rs
let mapper = crate::cartridge::mapper::create_mapper_with_prg_ram_size(...)?;
```

While cartridge loading uses `Result`, some areas use panics:

```rust
// src/mem_controller.rs line 142-143
0x2007 => panic!("Should never happen!"),
```

```rust
// src/mem_controller.rs line 202-204
0x8000..=0xFFFF => {
    if let Some(ref cartridge) = self.cartridge {
        // ...
    } else {
        panic!("No cartridge mapped, cannot read from {:04X}", addr);
    }
}
```

**Proposed Refactoring:**
- Replace panics with proper error handling using `Result` types
- Create domain-specific error types using `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("No cartridge mapped for address {0:#06X}")]
    NoCartridge(u16),
    #[error("Invalid PPU register access at {0:#06X}")]
    InvalidPpuAccess(u16),
}
```


---

## 6. Performance Hot Spots

### Current Issues Found

#### 6.1 Per-Cycle Allocation in PPU Rendering

**Location:** `src/ppu/ppu.rs`

The `tick()` function runs every PPU cycle and performs multiple conditional checks. This is appropriate given the emulation requirements, but some minor optimizations are possible:

**Observation:**
The PPU tick function is well-structured with early returns and efficient branching.

#### 6.2 VecDeque for Audio Samples

**Location:** `src/apu/apu.rs`

```rust
pending_samples: VecDeque<f32>,

// In clock_with_expansion:
self.pending_samples.push_back(self.mix() + expansion_audio.max(0.0));

if self.pending_samples.len() > MAX_PENDING_SAMPLES {
    self.pending_samples.pop_front();
}
```

**Proposed Refactoring:**
Consider using a ring buffer crate like `ringbuf` for better cache locality and fewer allocations:

```rust
use ringbuf::{HeapRb, Producer, Consumer};

// More efficient for audio streaming
let rb = HeapRb::<f32>::new(MAX_PENDING_SAMPLES);
```

#### 6.3 Lookup Table Usage ✓

**Location:** `src/apu/apu.rs`

The APU uses pre-computed lookup tables for the mixer, which is excellent:

```rust
const PULSE_TABLE: [f32; 31] = [...];
const TND_TABLE: [f32; 203] = [...];
```

This is a good example of performance optimization.

---

## 7. Interior Mutability Patterns

### Current Issues Found

#### 7.1 RefCell for Joypads

**Location:** `src/mem_controller.rs`

```rust
pub struct MemController {
    // ...
    joypad1: RefCell<Joypad>,
    joypad2: RefCell<Joypad>,
    open_bus: RefCell<u8>,
}
```

**Why This Might Be Justified:**
- Read operations need to modify joypad shift register state
- Open bus needs updating on reads

**Potential Improvement:**
The `read` method could take `&mut self` instead of `&self`, eliminating the need for `RefCell`:

```rust
pub fn read(&mut self, addr: u16) -> u8 {
    // Direct mutation instead of RefCell
}
```

#### 7.2 Nested RefCell Access

**Location:** `src/cpu/cpu.rs`

```rust
fn end_cpu_cycle_latch_interrupt_lines(&mut self) {
    // ...
    if self.ppu.borrow_mut().poll_nmi() {
        self.nmi_pending = true;
    }
    
    let irq_asserted_from_apu = self.apu.borrow().poll_irq();
    let irq_asserted_from_mapper = self.memory.borrow().mapper_irq_pending();
}
```

Multiple `borrow()` and `borrow_mut()` calls in the same function increase complexity.

**Proposed Refactoring:**
Consider batching the borrows or redesigning to avoid multiple RefCell accesses per cycle.

---

## 8. Module Structure

### Current Issues Found

#### 8.1 Module Organization is Generally Good ✓

The codebase has clean module separation:
- `src/cpu/` - CPU emulation
- `src/ppu/` - PPU emulation  
- `src/apu/` - APU emulation
- `src/cartridge/` - Cartridge and mapper implementations

#### 8.2 Large PPU Module

**Location:** `src/ppu/ppu.rs`

At ~2400 lines, the PPU implementation is quite large. The modular component approach (Timing, Status, Registers, etc.) helps, but the main `tick()` function is complex.

**Proposed Refactoring:**
Consider breaking out the rendering logic:

```
src/ppu/
├── mod.rs
├── ppu.rs          // Core PPU struct and public API
├── tick.rs         // Main tick logic (extracted)
├── background.rs
├── sprites.rs
├── registers.rs
├── timing.rs
├── status.rs
├── memory.rs
└── rendering.rs
```

#### 8.3 Consistent Use of `mod.rs` Pattern ✓

All multi-file modules use the `mod.rs` pattern consistently.

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

#### 10.1 Good Use of Match Expressions ✓

The codebase makes good use of pattern matching:

```rust
match addr {
    0x0000..=0x1FFF => self.cpu_ram[(addr & 0x07FF) as usize],
    0x2000..=0x3FFF => match addr & 0x2007 {
        0x2000 => self.ppu.borrow().registers.io_bus(),
        // ...
    },
    // ...
}
```

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
