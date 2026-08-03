//! Shared save-state primitives used by every console core.
//!
//! This module provides the building blocks that all four consoles (NES, Game
//! Boy / CGB, Game Boy Advance, SNES) use to implement snapshot save/restore:
//!
//! - [`Stateful`] — a uniform capture/restore contract for state-owning
//!   components (CPU, PPU, APU, register blocks, …).
//! - [`SaveStateError`] — a shared, console-agnostic error type for the common
//!   failure modes (version, (de)serialization, restore).
//! - [`to_bytes`] / [`from_bytes`] — generic JSON (de)serialization helpers.
//! - [`check_version`] — a shared version-gate helper.
//!
//! The on-disk format is JSON (via `serde_json`). Changing the format is
//! tracked separately as epic #2825 item I2.2 and is intentionally out of scope
//! here.
//!
//! # Compile-time guarantee
//!
//! Each console assembles its top-level save-state by calling
//! [`Stateful::capture_state`] / [`Stateful::restore_state`] on its
//! state-owning components. Because the aggregate snapshot is produced *through*
//! the trait, adding a new state-owning component that does not implement
//! [`Stateful`] fails to compile until the trait is implemented for it.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A component that can capture and restore its serializable state.
///
/// Implemented by state-owning emulator components. [`restore_state`] is
/// intentionally **infallible**: validation that can fail (format version,
/// mapper / ROM identity, memory-region sizes) is performed at the console
/// boundary *before* a captured snapshot is applied, not inside the trait.
///
/// [`restore_state`]: Stateful::restore_state
pub trait Stateful {
    /// The serializable snapshot produced by [`capture_state`](Self::capture_state).
    type State: Serialize + DeserializeOwned;

    /// Capture the current state as a serializable snapshot.
    fn capture_state(&self) -> Self::State;

    /// Restore the component from a previously captured snapshot.
    fn restore_state(&mut self, state: &Self::State);
}

/// Errors shared by all console save-state pipelines.
///
/// Console-specific failures (for example the NES mapper mismatch or the SNES
/// ROM-identity mismatch) live in slim per-console error enums that convert
/// `From<SaveStateError>`; this type only models the failure modes common to
/// every console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveStateError {
    /// The save-state format version is not one this build can load.
    IncompatibleVersion {
        /// Version found in the save-state.
        found: u32,
        /// Versions this build accepts.
        supported: Vec<u32>,
    },
    /// Serializing the save-state failed.
    SerializationFailed(String),
    /// Deserializing the save-state failed.
    DeserializationFailed(String),
    /// Applying a deserialized snapshot to the running emulator failed.
    RestoreFailed(String),
}

impl std::fmt::Display for SaveStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleVersion { found, supported } => write!(
                f,
                "incompatible save-state version (expected one of {supported:?}, found {found})"
            ),
            Self::SerializationFailed(msg) => write!(f, "serialization failed: {msg}"),
            Self::DeserializationFailed(msg) => write!(f, "deserialization failed: {msg}"),
            Self::RestoreFailed(msg) => write!(f, "restore failed: {msg}"),
        }
    }
}

impl std::error::Error for SaveStateError {}

/// Serialize a save-state value to JSON-encoded bytes.
///
/// # Errors
///
/// Returns [`SaveStateError::SerializationFailed`] if serialization fails.
pub fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, SaveStateError> {
    serde_json::to_vec(value).map_err(|e| SaveStateError::SerializationFailed(e.to_string()))
}

/// Deserialize a save-state value from JSON-encoded bytes.
///
/// # Errors
///
/// Returns [`SaveStateError::DeserializationFailed`] if deserialization fails.
pub fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, SaveStateError> {
    serde_json::from_slice(bytes).map_err(|e| SaveStateError::DeserializationFailed(e.to_string()))
}

/// Check whether a save-state version is supported.
///
/// # Errors
///
/// Returns [`SaveStateError::IncompatibleVersion`] if `found` is not contained
/// in `supported`.
pub fn check_version(found: u32, supported: &[u32]) -> Result<(), SaveStateError> {
    if supported.contains(&found) {
        Ok(())
    } else {
        Err(SaveStateError::IncompatibleVersion {
            found,
            supported: supported.to_vec(),
        })
    }
}

/// Gzip-compress bytes.
///
/// Test-only helper used by the per-console golden-fixture regeneration tests so
/// that committed save-state fixtures stay small.
#[cfg(test)]
pub(crate) fn gzip_compress(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(bytes)
        .expect("gzip compression should not fail on an in-memory buffer");
    encoder
        .finish()
        .expect("gzip finalize should not fail on an in-memory buffer")
}

/// Whether the golden-fixture regeneration tests may write to the repository.
///
/// The four `regenerate_golden_save_state_fixture` tests are `#[ignore]`d, but
/// `cargo test -- --include-ignored` runs them anyway, and they overwrite
/// committed fixtures. That is destructive rather than merely untidy: each
/// fixture's value comes from having been written by an *older* build (see the
/// `test_golden_save_state_v*_loads` tests), so regenerating one silently turns
/// a compatibility test into a round-trip test, in an opaque `.gz` diff.
///
/// Gating the write on an explicit opt-in keeps any ordinary test run
/// non-destructive. Presence-based, matching `NESER_CAPTURE_SCREEN`.
#[cfg(test)]
pub(crate) fn fixture_regeneration_enabled() -> bool {
    std::env::var_os("NESER_REGENERATE_FIXTURES").is_some()
}

/// Gzip-decompress bytes produced by [`gzip_compress`].
///
/// Test-only helper used by the per-console golden-fixture loading tests.
#[cfg(test)]
pub(crate) fn gzip_decompress(bytes: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .expect("decompressing a committed fixture should not fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
    struct DummyState {
        version: u32,
        counter: u64,
        bytes: Vec<u8>,
    }

    /// A trivial state-owning component used to exercise the `Stateful` trait.
    struct DummyComponent {
        counter: u64,
        bytes: Vec<u8>,
    }

    impl Stateful for DummyComponent {
        type State = DummyState;

        fn capture_state(&self) -> Self::State {
            DummyState {
                version: 1,
                counter: self.counter,
                bytes: self.bytes.clone(),
            }
        }

        fn restore_state(&mut self, state: &Self::State) {
            self.counter = state.counter;
            self.bytes = state.bytes.clone();
        }
    }

    #[test]
    fn test_to_bytes_then_from_bytes_round_trips_value() {
        let value = DummyState {
            version: 3,
            counter: 42,
            bytes: vec![1, 2, 3, 4],
        };

        let bytes = to_bytes(&value).expect("serialization should succeed");
        let restored: DummyState = from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(restored, value);
    }

    #[test]
    fn test_from_bytes_on_invalid_data_returns_deserialization_failed() {
        let result = from_bytes::<DummyState>(b"not valid json");

        assert!(matches!(
            result,
            Err(SaveStateError::DeserializationFailed(_))
        ));
    }

    #[test]
    fn test_check_version_accepts_supported_version() {
        assert_eq!(check_version(6, &[6, 5, 4]), Ok(()));
    }

    #[test]
    fn test_check_version_rejects_unsupported_version() {
        let result = check_version(9, &[6, 5, 4]);

        assert_eq!(
            result,
            Err(SaveStateError::IncompatibleVersion {
                found: 9,
                supported: vec![6, 5, 4],
            })
        );
    }

    #[test]
    fn test_incompatible_version_display_lists_supported_versions() {
        let err = SaveStateError::IncompatibleVersion {
            found: 9,
            supported: vec![6, 5, 4],
        };

        assert_eq!(
            err.to_string(),
            "incompatible save-state version (expected one of [6, 5, 4], found 9)"
        );
    }

    #[test]
    fn test_serialization_failed_display() {
        let err = SaveStateError::SerializationFailed("boom".to_string());
        assert_eq!(err.to_string(), "serialization failed: boom");
    }

    #[test]
    fn test_deserialization_failed_display() {
        let err = SaveStateError::DeserializationFailed("bad".to_string());
        assert_eq!(err.to_string(), "deserialization failed: bad");
    }

    #[test]
    fn test_restore_failed_display() {
        let err = SaveStateError::RestoreFailed("nope".to_string());
        assert_eq!(err.to_string(), "restore failed: nope");
    }

    #[test]
    fn test_save_state_error_is_std_error() {
        // Compiles only if SaveStateError implements std::error::Error; also
        // confirm there is no underlying source for these leaf errors.
        let err = SaveStateError::RestoreFailed("x".to_string());
        let dyn_err: &dyn std::error::Error = &err;
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn test_stateful_round_trip_through_helpers() {
        let component = DummyComponent {
            counter: 7,
            bytes: vec![9, 8, 7],
        };

        // Capture -> serialize -> deserialize -> restore into a fresh component.
        let snapshot = component.capture_state();
        let bytes = to_bytes(&snapshot).expect("serialize snapshot");
        let decoded: DummyState = from_bytes(&bytes).expect("deserialize snapshot");

        let mut restored = DummyComponent {
            counter: 0,
            bytes: Vec::new(),
        };
        restored.restore_state(&decoded);

        assert_eq!(restored.counter, 7);
        assert_eq!(restored.bytes, vec![9, 8, 7]);
    }
}
