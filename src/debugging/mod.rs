pub mod breakpoints;
mod logging;
mod tracing;

pub use logging::log_info;
pub use tracing::*;

// Re-export NES-specific debugging modules for backward compatibility.
// These moved to crate::nes::debugging but consumers still import from here.
#[cfg(feature = "native")]
pub use crate::nes::debugging::control;
pub use crate::nes::debugging::ppu_viewer;
#[cfg(feature = "native")]
pub use crate::nes::debugging::ui;

#[allow(unused_imports)]
pub use crate::nes::debugging::snapshot::{DebuggerViewState, snapshot};
#[allow(unused_imports)]
pub use crate::nes::debugging::types::DebuggerSnapshot;
