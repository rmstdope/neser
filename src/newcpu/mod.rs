//! New cycle-accurate CPU implementation
//!
//! This module implements a from-scratch cycle-accurate 6502 CPU where cycle-accurate
//! execution is the default and only execution path. It runs in parallel with the
//! existing CPU implementation during development.

pub mod types;

pub use types::*;
