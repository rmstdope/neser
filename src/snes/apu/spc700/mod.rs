//! SPC700 CPU core for the SNES APU.
//!
//! This sub-module contains the cycle-accurate SPC700 CPU (`cpu`) and the bus
//! abstraction it runs against (`bus`). The CPU is generic over [`Spc700Bus`]
//! so it can be unit-tested with a flat-RAM bus and verified against
//! SingleStepTests `spc700` vectors, mirroring the 65816 core's design.

mod bus;
#[allow(clippy::module_inception)]
mod cpu;

#[allow(unused_imports)] // Wired into the APU/ARAM in later sub-issues.
pub use bus::{FlatRamBus, Spc700Bus};
#[allow(unused_imports)] // Wired into the APU in later sub-issues.
pub use cpu::{
    FLAG_BREAK, FLAG_CARRY, FLAG_DIRECT_PAGE, FLAG_HALF_CARRY, FLAG_INTERRUPT, FLAG_NEGATIVE,
    FLAG_OVERFLOW, FLAG_ZERO, Spc700,
};
