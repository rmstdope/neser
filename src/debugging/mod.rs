pub mod breakpoints;
// Depends on DebuggerUiAction from `ui` module (also feature-gated).
// Tests run under `cargo test --lib` (default features include `native`).
#[cfg(feature = "native")]
pub mod control;
mod disasm;
mod logging;
pub mod ppu_viewer;
pub(crate) mod snapshot;
mod tracing;
mod types;

#[cfg(feature = "native")]
pub mod ui;

pub use logging::log_info;
#[allow(unused_imports)] // Used by frontend features
pub use snapshot::{DebuggerViewState, snapshot};
pub use tracing::*;
pub use types::DebuggerSnapshot;
