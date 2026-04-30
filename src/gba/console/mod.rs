//! GBA console wrapper and configuration.
//!
//! Provides the platform-facing [`Gba`] struct that implements the [`Emulator`]
//! trait, allowing the GBA to be driven through the common [`Console`] interface.
//!
//! [`Emulator`]: crate::platform::emulator::Emulator
//! [`Console`]: crate::platform::emulator::Console

pub mod config;
pub mod gba;
pub mod save_state;
