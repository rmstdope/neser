//! SNES bus architecture and memory access.

#[allow(clippy::module_inception)]
mod bus;
mod system_bus;

#[cfg(test)]
pub use bus::TestBus;
pub use bus::{SnesBus, StubBus};
#[allow(unused_imports)]
pub use system_bus::SnesSystemBus;
