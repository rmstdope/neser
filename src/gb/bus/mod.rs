pub use bus::{GbBus, StubBus};
pub use dmg_bus::DmgBus;

#[allow(clippy::module_inception)]
mod bus;
mod dmg_bus;
