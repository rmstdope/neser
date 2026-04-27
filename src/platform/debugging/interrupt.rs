//! Unified interrupt types for debugger operations.
//!
//! This module provides interrupt type definitions used by the debugger
//! for run-to-interrupt and step-out operations.
//!
//! NES uses `crate::nes::cpu::InterruptKind` directly.
//! GB uses `crate::platform::debugging::breakpoints::GbInterruptKind`.
