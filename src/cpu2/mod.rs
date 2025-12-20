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
pub use cpu::Cpu;
pub use traits::AddressingMode;
pub use traits::InstructionType;
pub use types::CpuState;
pub use crate::mem_controller::MemController;
