//! 32 KB battery-backed SRAM cartridge save backend.
//!
//! Real GBA SRAM lives at the `0x0E000000`–`0x0E007FFF` slice of the GBA
//! address map and is exclusively 8-bit. This module owns the byte buffer
//! and exposes mirrored read/write/snapshot helpers; the bus layer (or
//! tests) is responsible for masking incoming addresses to the SRAM window
//! before calling [`Sram::read`] / [`Sram::write`].

use crate::gba::cartridge::save_type::SaveType;
use serde::{Deserialize, Serialize};

/// Default SRAM size for shipping titles. 64 KB SRAM was rare and exposed
/// the upper 32 KB through bank-switching that very few games used; mirror
/// the common 32 KB case here.
pub const SRAM_SIZE: usize = 32 * 1024;

/// Convenience: the [`SaveType`] this backend implements.
pub const SAVE_TYPE: SaveType = SaveType::Sram32K;

/// 32 KB battery-backed SRAM with mirrored access.
#[derive(Debug, Clone)]
pub struct Sram {
    data: Vec<u8>,
}

/// Serializable SRAM snapshot for GBA save states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SramState {
    data: Vec<u8>,
}

impl Default for Sram {
    fn default() -> Self {
        Self::new()
    }
}

impl Sram {
    /// Create a fresh 32 KB SRAM filled with `0xFF` (matches the erased
    /// state of a battery cell after long power-off).
    pub fn new() -> Self {
        Self {
            data: vec![0xFF; SRAM_SIZE],
        }
    }

    /// Read a byte from `offset`, mirrored within the SRAM window.
    pub fn read(&self, offset: usize) -> u8 {
        self.data[offset % SRAM_SIZE]
    }

    /// Write `value` to `offset`, mirrored within the SRAM window.
    pub fn write(&mut self, offset: usize, value: u8) {
        self.data[offset % SRAM_SIZE] = value;
    }

    /// Borrow the entire backing buffer (for `.sav` flush).
    pub fn snapshot(&self) -> &[u8] {
        &self.data
    }

    /// Capture SRAM state for save-state serialization.
    pub fn capture_state(&self) -> SramState {
        SramState {
            data: self.data.clone(),
        }
    }

    /// Restore SRAM state from a save-state snapshot.
    pub fn restore_state(&mut self, state: &SramState) -> Result<(), String> {
        if state.data.len() != SRAM_SIZE {
            return Err(format!(
                "SRAM save-state length mismatch: expected {SRAM_SIZE}, got {}",
                state.data.len()
            ));
        }
        self.data.clone_from(&state.data);
        Ok(())
    }

    /// Restore the SRAM contents from `data`. Bytes past [`SRAM_SIZE`] are
    /// silently ignored; if `data` is shorter than the SRAM the trailing
    /// bytes are left untouched (typical when restoring from a partial
    /// `.sav`).
    pub fn restore(&mut self, data: &[u8]) {
        let n = data.len().min(SRAM_SIZE);
        self.data[..n].copy_from_slice(&data[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_sram_reads_as_0xff() {
        let sram = Sram::new();
        assert_eq!(sram.read(0), 0xFF);
        assert_eq!(sram.read(SRAM_SIZE - 1), 0xFF);
    }

    #[test]
    fn write_then_read_round_trips() {
        let mut sram = Sram::new();
        sram.write(0x0000, 0xAA);
        sram.write(0x1234, 0x5A);
        sram.write(SRAM_SIZE - 1, 0x42);
        assert_eq!(sram.read(0x0000), 0xAA);
        assert_eq!(sram.read(0x1234), 0x5A);
        assert_eq!(sram.read(SRAM_SIZE - 1), 0x42);
    }

    #[test]
    fn access_mirrors_within_sram_window() {
        let mut sram = Sram::new();
        sram.write(0x0010, 0x77);
        // 0x0010 + SRAM_SIZE should mirror back to 0x0010.
        assert_eq!(sram.read(0x0010 + SRAM_SIZE), 0x77);
        assert_eq!(sram.read(0x0010 + 2 * SRAM_SIZE), 0x77);
    }

    #[test]
    fn snapshot_then_restore_preserves_contents() {
        let mut a = Sram::new();
        for (i, byte) in (0..256u32).enumerate() {
            a.write(i, byte as u8);
        }
        let snapshot = a.snapshot().to_vec();

        let mut b = Sram::new();
        b.restore(&snapshot);
        for i in 0..256 {
            assert_eq!(b.read(i), i as u8);
        }
    }

    #[test]
    fn restore_truncates_oversized_input() {
        let mut sram = Sram::new();
        let oversized = vec![0xCC; SRAM_SIZE * 2];
        sram.restore(&oversized);
        // Should not have panicked and should be filled to capacity only.
        assert_eq!(sram.snapshot().len(), SRAM_SIZE);
        assert_eq!(sram.read(0), 0xCC);
        assert_eq!(sram.read(SRAM_SIZE - 1), 0xCC);
    }

    #[test]
    fn restore_short_input_leaves_tail_intact() {
        let mut sram = Sram::new();
        sram.write(SRAM_SIZE - 1, 0x99);
        sram.restore(&[0x11, 0x22, 0x33]);
        assert_eq!(sram.read(0), 0x11);
        assert_eq!(sram.read(2), 0x33);
        assert_eq!(sram.read(SRAM_SIZE - 1), 0x99);
    }

    #[test]
    fn save_state_restores_sram_contents() {
        let mut sram = Sram::new();
        sram.write(0x0000, 0x12);
        sram.write(0x1234, 0x34);
        sram.write(SRAM_SIZE - 1, 0x56);

        let state = sram.capture_state();

        sram.write(0x0000, 0xAA);
        sram.write(0x1234, 0xBB);
        sram.write(SRAM_SIZE - 1, 0xCC);
        sram.restore_state(&state).expect("restore SRAM state");

        assert_eq!(sram.read(0x0000), 0x12);
        assert_eq!(sram.read(0x1234), 0x34);
        assert_eq!(sram.read(SRAM_SIZE - 1), 0x56);
    }

    #[test]
    fn save_state_roundtrips_through_json() {
        let mut sram = Sram::new();
        sram.write(0x0042, 0x99);

        let bytes = serde_json::to_vec(&sram.capture_state()).expect("serialize SRAM state");
        let decoded: SramState = serde_json::from_slice(&bytes).expect("deserialize SRAM state");
        let mut restored = Sram::new();
        restored
            .restore_state(&decoded)
            .expect("restore SRAM state");

        assert_eq!(restored.read(0x0042), 0x99);
    }
}
