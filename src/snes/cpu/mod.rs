//! SNES CPU (65816) emulation.

#[allow(clippy::module_inception)]
mod cpu;

#[allow(unused_imports)] // Will be used by console module in later sub-issues
pub use cpu::Cpu;
