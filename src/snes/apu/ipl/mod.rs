//! Embedded clean-room SNES APU IPL ROM.

pub const EMBEDDED_IPL: [u8; 64] = *include_bytes!("ipl.bin");
