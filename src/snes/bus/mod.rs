//! SNES bus architecture and memory access.

#[allow(clippy::module_inception)]
mod bus;
mod dma;
mod system_bus;

#[cfg(test)]
pub use bus::TestBus;
#[allow(unused_imports)]
pub use bus::{SnesBus, StubBus};
#[allow(unused_imports)]
pub use system_bus::SnesSystemBus;
