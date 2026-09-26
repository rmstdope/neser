pub mod background;
mod bg_fifo;
pub mod dmg_palette;
mod obj_fifo;
mod pixel_fifo;
#[allow(clippy::module_inception)]
mod ppu;
pub mod registers;
pub mod rendering;
pub mod screen_buffer;
pub mod sprites;
pub mod timing;
#[cfg(test)]
mod trace_tests;
pub mod window;

pub use dmg_palette::GbPalette;
pub use ppu::Ppu;
pub(crate) use ppu::StopDisplayMode;
