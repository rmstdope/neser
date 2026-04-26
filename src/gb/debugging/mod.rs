//! Game Boy debugger module.
//!
//! Provides disassembly, CPU tracing, and (under the `native` feature) interactive debugging
//! with breakpoints, stepping, and PPU visualization.

#![allow(unused_imports)] // Debugger types exported for future UI integration

pub mod disasm;

#[cfg(feature = "native")]
pub mod control;
#[cfg(feature = "native")]
pub mod ppu_viewer;
#[cfg(feature = "native")]
pub mod snapshot;
#[cfg(feature = "native")]
pub mod types;

#[cfg(feature = "native")]
pub use control::GbDebuggerController;
#[cfg(feature = "native")]
pub use snapshot::{GbDebuggerViewState, snapshot, snapshot_with_disasm_state};

// Export disasm types (available without native feature)
pub use disasm::{GbCpuDisasmLineSnapshot, GbCpuDisasmWindowState};

// Export other debugger types (native-only)
#[cfg(feature = "native")]
pub use types::{
    GbCpuRegsSnapshot, GbCpuTraceLineSnapshot, GbDebuggerSnapshot, GbMemoryWatchEntrySnapshot,
};

// Export PPU viewer types (native-only)
#[cfg(feature = "native")]
pub use ppu_viewer::{
    GbPpuViewerSnapshot, format_oam_entries, format_palette_info, render_bg_maps_rgba,
    render_tiles_rgba,
};
