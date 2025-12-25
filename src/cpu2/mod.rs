//! Second attempt at cycle-accurate 6502 CPU emulation
//!
//! This module implements a cycle-accurate 6502 CPU with separate addressing
//! modes and operations.

pub mod addressing;
pub mod cpu;
pub mod instruction;
pub mod instruction_types;
pub mod traits;
pub mod types;

// Re-export commonly used types
pub use addressing::MemoryAccess;
pub use cpu::Cpu2;
pub use types::CpuState;
