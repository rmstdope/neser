//! Game Boy Advance debugging support.
//!
//! Provides ARM7TDMI disassembly, breakpoint management, and CPU execution
//! tracing for the native desktop frontend. The whole module is gated with
//! `#[cfg(not(target_arch = "wasm32"))]` in `src/gba/mod.rs` so WASM builds
//! do not reference any of these symbols (satisfying the WASM-build
//! acceptance criterion of issue #2218).
//!
//! See [`disasm`] for instruction formatting and [`trace`] for the execution
//! ring buffer. Address breakpoints use the shared
//! [`crate::platform::debugging::breakpoints::BreakpointList`] (parameterized
//! over the 32-bit ARM address space). [`controller::GbaDebuggerController`]
//! glues them together for the upcoming debugger UI.

pub mod controller;
pub mod disasm;
pub mod trace;

#[allow(unused_imports)]
pub use controller::{BreakpointHit, GbaDebuggerController};
#[allow(unused_imports)]
pub use disasm::{disasm_arm, disasm_thumb};
#[allow(unused_imports)]
pub use trace::{CpuTrace, DEFAULT_TRACE_CAPACITY, TraceEntry};
