//! ARM7TDMI CPU core for Game Boy Advance emulation.
//!
//! This module implements the ARM7TDMI 32-bit processor used in the GBA. It is
//! split across several sub-modules:
//!
//! * [`registers`] – the register file (R0–R15, CPSR, banked SPSRs), CPU mode
//!   model and condition-code evaluation.
//! * [`bus`] – the memory bus trait used by the CPU together with a small
//!   in-memory implementation that is convenient for unit tests.
//! * [`arm`] – decoder/executor for the 32-bit ARM instruction set.
//! * [`thumb`] – decoder/executor for the 16-bit Thumb instruction set.
//! * [`arm7tdmi`] – the [`Arm7tdmi`] struct that ties everything together
//!   (fetch / decode / execute, prefetch, interrupt dispatch).
//!
//! The implementation focuses on a representative subset of the ARM7TDMI
//! instruction set sufficient to satisfy the acceptance test vectors of the
//! foundational sub-issue (ARM ADD, Thumb PUSH/POP, IRQ/FIQ dispatch,
//! Thumb↔ARM switch, conditional skip, …) while leaving room for the rest of
//! the instruction set to be added in subsequent increments. The module
//! structure mirrors the existing NES (`src/nes/cpu`) and Game Boy
//! (`src/gb/cpu`) cores for consistency.

pub mod arm;
pub mod arm7tdmi;
pub mod bus;
pub mod registers;
pub mod thumb;

#[allow(unused_imports)]
pub use arm7tdmi::{Arm7tdmi, Arm7tdmiState, ExceptionVector};
#[allow(unused_imports)]
pub use bus::{Bus, RamBus};
#[allow(unused_imports)]
pub use registers::{CpuMode, Registers, condition_met};
