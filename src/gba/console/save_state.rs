//! Game Boy Advance save-state serialization.
//!
//! Defines a versioned [`GbaSaveState`] struct that captures GBA emulator
//! state for snapshot save/restore.  Serialised as JSON (matching the NES
//! and GB save-state formats) via [`GbaSaveState::to_bytes`] /
//! [`GbaSaveState::from_bytes`].
//!
//! This is the initial scaffold for the GBA save-state pipeline.  It
//! currently captures the CPU plus bus memory regions (BIOS, EWRAM, IWRAM,
//! PRAM, VRAM, OAM, SRAM), bus-owned peripherals, and a few simple bus scalars.
//! Subsystem state for the PPU, APU, and cartridge save backends will be added
//! as those modules are wired into the [`Gba`](super::gba::Gba)
//! console wrapper.  Because every save-state carries a `version` field,
//! breaking changes to the captured shape will simply bump
//! [`GBA_SAVESTATE_VERSION`] and the loader will reject older states with
//! a clear [`GbaSaveStateError::IncompatibleVersion`] error.

use serde::{Deserialize, Serialize};

use crate::gba::bus::{
    DmaController, InterruptController, IoRegisters, Timers, Waitstates, sio::Sio,
};
use crate::gba::cpu::Arm7tdmiState;
use crate::gba::input::Keypad;
use crate::gba::ppu::PpuState;

/// Current save-state format version for Game Boy Advance.
/// Increment this when making breaking changes to the state format.
pub const GBA_SAVESTATE_VERSION: u32 = 4;

/// Serializable snapshot of the [`GbaBus`](crate::gba::GbaBus) memory
/// regions and a small number of associated scalar fields.
///
/// The BIOS image is intentionally **not** serialized: the GBA BIOS is
/// copyrighted firmware that the user supplies separately, and embedding
/// it in save-state files would both bloat them and risk leaking
/// firmware bytes when states are shared.  Only the [`bios_locked`]
/// flag is captured; the BIOS already loaded into the running emulator
/// is preserved across a load.
///
/// [`bios_locked`]: Self::bios_locked
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusMemoryState {
    /// 256 KB on-board work RAM (EWRAM).
    pub ewram: Vec<u8>,
    /// 32 KB on-chip work RAM (IWRAM).
    pub iwram: Vec<u8>,
    /// 1 KB Palette RAM.
    pub pram: Vec<u8>,
    /// 96 KB VRAM.
    pub vram: Vec<u8>,
    /// 1 KB OAM.
    pub oam: Vec<u8>,
    /// 64 KB cartridge SRAM region (battery-backed RAM).
    pub sram: Vec<u8>,
    /// Raw I/O backing store for registers not yet owned by a subsystem.
    pub io: IoRegisters,
    /// Interrupt controller (`IE`, `IF`, `IME`).
    pub ic: InterruptController,
    /// Timer bank including live counters and prescaler accumulators.
    pub timers: Timers,
    /// DMA controller including armed/live transfer state.
    pub dma: DmaController,
    /// Serial I/O controller including in-progress transfer countdown.
    pub sio: Sio,
    /// Keypad state including KEYCNT, pressed buttons, and IRQ edge latch.
    pub keypad: Keypad,
    /// PPU state including display registers, timing, and framebuffer.
    pub ppu: PpuState,
    /// Whether external BIOS reads are currently locked out.
    pub bios_locked: bool,
    /// Last value driven on the bus (used to model open-bus reads).
    pub last_bus_value: u32,
    /// DMA internal data latch (separate from CPU open-bus).
    #[serde(default)]
    pub dma_latch: u32,
    /// Dynamic wait-state timing derived from WAITCNT.
    pub waitstates: Waitstates,
    /// Undocumented BIOS-written register at 0x04000410.
    pub undoc_0x410: u8,
    /// Pending HALTCNT halt request consumed by the CPU wrapper.
    pub halt_requested: bool,
}

/// Complete Game Boy Advance emulator state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GbaSaveState {
    /// Version of the save-state format.
    pub version: u32,
    /// ARM7TDMI CPU state.
    pub cpu: Arm7tdmiState,
    /// Bus memory state (RAM regions and a few scalar fields).
    pub bus: BusMemoryState,
}

/// Errors that can occur when (de)serializing a GBA save-state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GbaSaveStateError {
    /// The save-state format version is incompatible.
    IncompatibleVersion { expected: u32, found: u32 },
    /// Deserialization failed.
    DeserializationFailed(String),
    /// Serialization failed.
    SerializationFailed(String),
    /// Restoring the captured state into the running emulator failed
    /// (e.g. region size mismatch).
    RestoreFailed(String),
}

impl std::fmt::Display for GbaSaveStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleVersion { expected, found } => write!(
                f,
                "incompatible save-state version (expected {expected}, found {found})"
            ),
            Self::DeserializationFailed(msg) => write!(f, "deserialization failed: {msg}"),
            Self::SerializationFailed(msg) => write!(f, "serialization failed: {msg}"),
            Self::RestoreFailed(msg) => write!(f, "restore failed: {msg}"),
        }
    }
}

impl std::error::Error for GbaSaveStateError {}

impl GbaSaveState {
    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, GbaSaveStateError> {
        serde_json::to_vec(self).map_err(|e| GbaSaveStateError::SerializationFailed(e.to_string()))
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    ///
    /// Returns [`GbaSaveStateError::IncompatibleVersion`] when the
    /// deserialized state's `version` field does not match
    /// [`GBA_SAVESTATE_VERSION`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GbaSaveStateError> {
        let state: Self = serde_json::from_slice(bytes)
            .map_err(|e| GbaSaveStateError::DeserializationFailed(e.to_string()))?;
        if state.version != GBA_SAVESTATE_VERSION {
            return Err(GbaSaveStateError::IncompatibleVersion {
                expected: GBA_SAVESTATE_VERSION,
                found: state.version,
            });
        }
        Ok(state)
    }
}

// ── Convenience save / load on Gba ─────────────────────────────────────────

use super::gba::Gba;

impl Gba {
    /// Capture a full save-state snapshot of the current GBA state.
    pub fn save_state(&self) -> GbaSaveState {
        GbaSaveState {
            version: GBA_SAVESTATE_VERSION,
            cpu: self.capture_cpu_state(),
            bus: self.bus().capture_memory_state(),
        }
    }

    /// Restore the GBA state from a save-state snapshot.
    ///
    /// Returns [`GbaSaveStateError::IncompatibleVersion`] if the snapshot
    /// version does not match [`GBA_SAVESTATE_VERSION`], or
    /// [`GbaSaveStateError::RestoreFailed`] if the captured state cannot
    /// be applied to the current bus (e.g. region size mismatch).
    pub fn load_state(&mut self, state: &GbaSaveState) -> Result<(), GbaSaveStateError> {
        if state.version != GBA_SAVESTATE_VERSION {
            return Err(GbaSaveStateError::IncompatibleVersion {
                expected: GBA_SAVESTATE_VERSION,
                found: state.version,
            });
        }
        self.bus_mut()
            .restore_memory_state(&state.bus)
            .map_err(GbaSaveStateError::RestoreFailed)?;
        self.restore_cpu_state(&state.cpu);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
    };
    use crate::gba::console::gba::Gba;
    use crate::platform::app_context::AppContext;
    use crate::platform::emulator::Emulator;

    fn make_gba() -> Gba {
        Gba::new(AppContext::default())
    }

    fn minimal_valid_gba_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0xC0];
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    // ── Version checks ─────────────────────────────────────────────────────

    #[test]
    fn test_gba_savestate_version_is_4() {
        assert_eq!(GBA_SAVESTATE_VERSION, 4);
    }

    // ── Round-trip ─────────────────────────────────────────────────────────

    #[test]
    fn test_gba_save_state_roundtrip_through_bytes() {
        let gba = make_gba();
        let save = gba.save_state();
        let bytes = save.to_bytes().expect("serialization should succeed");
        let loaded = GbaSaveState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(loaded.version, GBA_SAVESTATE_VERSION);
        assert_eq!(loaded.bus.ewram.len(), save.bus.ewram.len());
        assert_eq!(loaded.bus.iwram.len(), save.bus.iwram.len());
        assert_eq!(loaded.bus.pram.len(), save.bus.pram.len());
        assert_eq!(loaded.bus.vram.len(), save.bus.vram.len());
        assert_eq!(loaded.bus.oam.len(), save.bus.oam.len());
        assert_eq!(loaded.bus.sram.len(), save.bus.sram.len());
        assert_eq!(loaded.bus.ic.ie, save.bus.ic.ie);
        assert_eq!(loaded.cpu.regs.r[15], save.cpu.regs.r[15]);
    }

    // ── Capture / restore preserves modified memory ────────────────────────

    #[test]
    fn test_save_state_captures_modified_memory() {
        let mut gba = make_gba();

        // Mutate EWRAM, IWRAM and SRAM through the bus.
        // EWRAM is at 0x0200_0000, IWRAM at 0x0300_0000, SRAM at 0x0E00_0000.
        use crate::gba::cpu::bus::Bus;
        gba.bus_mut().write8(0x0200_0010, 0xAA);
        gba.bus_mut().write8(0x0300_0020, 0xBB);
        gba.bus_mut().write8(0x0E00_0030, 0xCC);

        let saved = gba.save_state();

        // Overwrite memory after capture.
        gba.bus_mut().write8(0x0200_0010, 0x11);
        gba.bus_mut().write8(0x0300_0020, 0x22);
        gba.bus_mut().write8(0x0E00_0030, 0x33);
        assert_eq!(gba.bus_mut().read8(0x0200_0010), 0x11);

        // Restore — values should match the captured state.
        gba.load_state(&saved).expect("restore should succeed");
        assert_eq!(gba.bus_mut().read8(0x0200_0010), 0xAA);
        assert_eq!(gba.bus_mut().read8(0x0300_0020), 0xBB);
        assert_eq!(gba.bus_mut().read8(0x0E00_0030), 0xCC);
    }

    #[test]
    fn test_save_state_captures_and_restores_cpu_position() {
        let mut gba = make_gba();
        gba.load_rom(&minimal_valid_gba_rom(), "test.gba")
            .expect("valid GBA ROM");

        let saved = gba.save_state();
        let saved_pc = gba.cpu_pc();

        gba.run_tick_for_tests();
        assert_ne!(gba.cpu_pc(), saved_pc, "test must dirty CPU PC after save");

        gba.load_state(&saved).expect("restore should succeed");

        assert_eq!(gba.cpu_pc(), saved_pc);
    }

    // ── BIOS exclusion ─────────────────────────────────────────────────────

    #[test]
    fn test_save_state_does_not_embed_bios_bytes() {
        // Load a recognisable BIOS pattern.
        let mut gba = make_gba();
        let mut bios = vec![0u8; 16 * 1024];
        for (i, b) in bios.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        gba.bus_mut().load_bios(&bios);

        let bytes = gba.save_state().to_bytes().expect("serialization succeeds");

        // The save-state must not embed BIOS bytes.  Search for a unique
        // recognisable run of the BIOS pattern (256 sequential bytes is
        // astronomically unlikely to appear in a clean state otherwise).
        let needle: Vec<u8> = (0u8..=255).collect();
        let found = bytes.windows(needle.len()).any(|w| w == needle.as_slice());
        assert!(!found, "save-state must not embed BIOS firmware bytes");
    }

    #[test]
    fn test_load_state_preserves_existing_bios() {
        // Snapshot before BIOS is loaded.
        let mut gba = make_gba();
        let saved = gba.save_state();

        // Now load a BIOS image and overwrite EWRAM so the load actually
        // mutates the bus.
        let mut bios = vec![0u8; 16 * 1024];
        bios[0] = 0xDE;
        bios[1] = 0xAD;
        bios[2] = 0xBE;
        bios[3] = 0xEF;
        gba.bus_mut().load_bios(&bios);

        // Restore from the pre-BIOS snapshot.  The loaded BIOS must
        // remain in place because the snapshot didn't capture it.
        use crate::gba::cpu::bus::Bus;
        gba.load_state(&saved).expect("restore succeeds");
        assert_eq!(gba.bus_mut().read8(0x0000_0000), 0xDE);
        assert_eq!(gba.bus_mut().read8(0x0000_0001), 0xAD);
        assert_eq!(gba.bus_mut().read8(0x0000_0002), 0xBE);
        assert_eq!(gba.bus_mut().read8(0x0000_0003), 0xEF);
    }

    // ── Version mismatch ───────────────────────────────────────────────────

    #[test]
    fn test_incompatible_version_error_from_bytes() {
        let gba = make_gba();
        let mut save = gba.save_state();
        save.version = 9999;

        let bytes = serde_json::to_vec(&save).expect("raw serialization succeeds");
        let result = GbaSaveState::from_bytes(&bytes);
        match result {
            Err(GbaSaveStateError::IncompatibleVersion { expected, found }) => {
                assert_eq!(expected, GBA_SAVESTATE_VERSION);
                assert_eq!(found, 9999);
            }
            other => panic!("Expected IncompatibleVersion error, got {other:?}"),
        }
    }

    #[test]
    fn test_incompatible_version_error_from_load_state() {
        let mut gba = make_gba();
        let mut save = gba.save_state();
        save.version = 9999;

        let result = gba.load_state(&save);
        match result {
            Err(GbaSaveStateError::IncompatibleVersion { expected, found }) => {
                assert_eq!(expected, GBA_SAVESTATE_VERSION);
                assert_eq!(found, 9999);
            }
            other => panic!("Expected IncompatibleVersion error, got {other:?}"),
        }
    }

    // ── Invalid data ───────────────────────────────────────────────────────

    #[test]
    fn test_invalid_json_returns_deserialization_error() {
        let result = GbaSaveState::from_bytes(b"not valid json");
        assert!(matches!(
            result,
            Err(GbaSaveStateError::DeserializationFailed(_))
        ));
    }

    #[test]
    fn test_region_size_mismatch_returns_restore_error() {
        let mut gba = make_gba();
        let mut save = gba.save_state();
        // Truncate EWRAM to an invalid size.
        save.bus.ewram.truncate(16);

        let result = gba.load_state(&save);
        assert!(matches!(result, Err(GbaSaveStateError::RestoreFailed(_))));
    }

    // ── Display impl ───────────────────────────────────────────────────────

    #[test]
    fn test_error_display_formatting() {
        let e = GbaSaveStateError::IncompatibleVersion {
            expected: 1,
            found: 2,
        };
        assert!(format!("{e}").contains("incompatible save-state version"));

        let e = GbaSaveStateError::DeserializationFailed("oops".into());
        assert!(format!("{e}").contains("deserialization failed"));

        let e = GbaSaveStateError::SerializationFailed("oops".into());
        assert!(format!("{e}").contains("serialization failed"));

        let e = GbaSaveStateError::RestoreFailed("oops".into());
        assert!(format!("{e}").contains("restore failed"));
    }
}
