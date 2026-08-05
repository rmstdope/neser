pub mod config;
pub mod gameboy;
mod gb;
pub mod save_state;

// `CpuTraceLine`'s only consumer is the debugger UI, which is compiled out in a
// `frontend`-without-`native` build; the re-export is still part of this
// module's API there, so it is kept rather than cfg-gated to match.
#[allow(unused_imports)]
pub use gb::{CpuTraceLine, Gb};
