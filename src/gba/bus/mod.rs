//! GBA system memory bus.
//!
//! Routes the ARM7TDMI 32-bit address space across the GBA memory regions
//! (BIOS, EWRAM, IWRAM, I/O, PRAM, VRAM, OAM, cart ROM/SRAM) per GBATek's
//! "GBA Memory Map" tables. Implements the [`Bus`](super::cpu::Bus) trait
//! used by the CPU, and exposes hooks for stepping the timer system and
//! routing interrupts.
//!
//! See `architecture.md` for the GBA module layout.
//!
//! <https://problemkaputt.de/gbatek.htm#gbamemorymap>

mod addressing;
mod cpu_bus;
pub mod dma;
mod dma_bus;
mod gba_bus;
pub mod interrupt;
pub mod io;
pub mod memory;
pub mod sio;
pub mod timer;
mod waitstates;

pub use dma::{DmaBus, DmaChannel, DmaController};
pub use gba_bus::GbaBus;
pub use interrupt::{InterruptController, bits as irq_bits};
pub use io::{IoRegisters, REG_IE, REG_IF, REG_IME};
pub use timer::{Timer, Timers};
pub use waitstates::{Waitstates, WidthClass};
