//! SNES bus architecture and memory access.

#[allow(clippy::module_inception)]
mod bus;

pub use bus::{SnesBus, StubBus, TestBus};
