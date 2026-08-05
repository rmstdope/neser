#[cfg(feature = "native")]
pub mod control;
pub(crate) mod disasm;
pub mod ppu_viewer;
pub(crate) mod snapshot;
pub(crate) mod types;

#[cfg(feature = "native")]
pub mod ui;

#[allow(unused_imports)]
pub use snapshot::{DebuggerViewState, snapshot};
// Consumed by `ui` (native only) and the wasm frontend, so it reads as unused
// in a `frontend`-without-`native` build even though both other builds need it.
#[allow(unused_imports)]
pub use types::DebuggerSnapshot;
