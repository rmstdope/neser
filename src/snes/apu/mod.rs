//! SNES APU (Audio Processing Unit) emulation.
//!
//! The APU is a self-contained subsystem: a ~1.024 MHz SPC700 CPU plus an S-DSP
//! sharing 64 KB ARAM, communicating with the main 65816 only through the four
//! I/O ports (`$2140–$2143` ⟷ `$F4–$F7`).
//!
//! Implemented incrementally under epic #2721:
//! - `spc700`: SPC700 CPU core (this sub-issue, #2773).
//! - ARAM, IPL boot ROM, ports, timers, and the S-DSP follow in later sub-issues.

pub mod spc700;
