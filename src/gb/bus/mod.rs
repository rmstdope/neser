#[allow(unused_imports)]
pub use bus::{GbBus, StubBus};
pub use cgb_bus::CgbBus;
pub use dmg_bus::DmgBus;

#[allow(clippy::module_inception)]
mod bus;
mod cgb_bus;
mod dmg_bus;
pub mod hdma;
mod memory_map;
mod oam_dma;
mod serial;
mod snapshot;
mod tick_sequence;
