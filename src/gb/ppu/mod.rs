pub mod background;
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

pub use ppu::Ppu;
