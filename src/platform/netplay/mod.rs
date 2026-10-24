//! The rules that keep two emulators in lockstep over an unreliable link (nr-ayk5.2).
//!
//! [`lockstep::Lockstep`] is a pure state machine with no clock and no I/O: the caller feeds
//! it the local pad and the bytes that arrived, runs a frame only when it returns both pads,
//! and sends whatever [`lockstep::Lockstep::packet`] says. [`wire`] is the packet format.
//! The test-only `simulation` module proves the rules over a channel with latency, jitter
//! and loss.

pub mod lockstep;
#[cfg(test)]
pub(crate) mod simulation;
pub mod wire;
