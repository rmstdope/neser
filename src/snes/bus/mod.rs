//! SNES bus architecture and memory access.

#[allow(clippy::module_inception)]
mod bus;

#[cfg(test)]
pub use bus::TestBus;
pub use bus::{SnesBus, StubBus};
