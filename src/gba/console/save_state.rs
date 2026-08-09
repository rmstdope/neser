//! Game Boy Advance save-state serialization.
//!
//! Defines a versioned [`GbaSaveState`] struct that captures GBA emulator
//! state for snapshot save/restore.  Serialised as JSON (matching the NES
//! and GB save-state formats) via [`GbaSaveState::to_bytes`] /
//! [`GbaSaveState::from_bytes`].
//!
//! This is the initial scaffold for the GBA save-state pipeline.  It
//! captures the CPU plus bus memory regions (EWRAM, IWRAM, PRAM, VRAM, OAM,
//! SRAM mirror), bus-owned peripherals, PPU state, APU state, cartridge save
//! backend state, and a few simple bus scalars. Because every save-state
//! carries a `version` field, breaking changes to the captured shape will bump
//! [`GBA_SAVESTATE_VERSION`] and the loader will reject older states with
//! a clear [`SaveStateError::IncompatibleVersion`] error.
//!
//! [`SaveStateError::IncompatibleVersion`]: crate::platform::save_state::SaveStateError::IncompatibleVersion

use serde::{Deserialize, Serialize};

use crate::gba::apu::ApuState;
use crate::gba::bus::{
    DmaController, InterruptController, IoRegisters, Timers, Waitstates, sio::Sio,
};
use crate::gba::cartridge::SaveBackendState;
use crate::gba::cpu::Arm7tdmiState;
use crate::gba::input::Keypad;
use crate::gba::ppu::PpuState;
use crate::platform::save_state::SaveStateError;

/// Current save-state format version for Game Boy Advance.
/// Increment this when making breaking changes to the state format.
pub const GBA_SAVESTATE_VERSION: u32 = 7;
const GBA_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE: u32 = 6;

/// Save-state format versions this build can load (current plus legacy).
const SUPPORTED_SAVESTATE_VERSIONS: [u32; 2] = [
    GBA_SAVESTATE_VERSION,
    GBA_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE,
];

/// Serializable snapshot of the [`GbaBus`](crate::gba::GbaBus) memory
/// regions and a small number of associated scalar fields.
///
/// The BIOS image is intentionally **not** serialized: the GBA BIOS is
/// copyrighted firmware that the user supplies separately, and embedding
/// it in save-state files would both bloat them and risk leaking
/// firmware bytes when states are shared.  Only BIOS protection/latch
/// state is captured; the BIOS already loaded into the running emulator
/// is preserved across a load.
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
    /// Cartridge save backend, including SRAM/EEPROM/Flash data and volatile
    /// EEPROM/Flash command state.
    #[serde(default)]
    pub cart_save: SaveBackendState,
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
    /// APU state including channel, FIFO, mixer, and timing state.
    pub apu: ApuState,
    /// Whether external BIOS reads are currently locked out.
    pub bios_locked: bool,
    /// Last value driven on the bus (used to model open-bus reads).
    pub last_bus_value: u32,
    /// Last BIOS opcode fetched by the CPU prefetcher.
    #[serde(default)]
    pub bios_open_bus_value: u32,
    /// Whether the CPU execution context is currently in BIOS.
    #[serde(default)]
    pub executing_bios: bool,
    /// DMA internal data latch (separate from CPU open-bus).
    #[serde(default)]
    pub dma_latch: u32,
    /// Whether the DMA latch has been initialized by a DMA read.
    #[serde(default)]
    pub dma_latch_valid: bool,
    /// Remaining CPU-instruction window where unused reads see DMA bus value.
    #[serde(default)]
    pub dma_open_bus_instructions: u8,
    /// Last Game Pak opcode-prefetch value used by unused-area CPU reads.
    #[serde(default)]
    pub gamepak_prefetch_open_bus_value: u32,
    /// Whether `gamepak_prefetch_open_bus_value` has been initialized.
    #[serde(default)]
    pub gamepak_prefetch_open_bus_valid: bool,
    /// CPU-visible TM0 sample phase while software polls DISPSTAT H-Blank edges.
    #[serde(default)]
    pub hblank_edge_timer_sample_index: u8,
    /// Dynamic wait-state timing derived from WAITCNT.
    pub waitstates: Waitstates,
    /// Undocumented BIOS-written register at 0x04000410.
    pub undoc_0x410: u8,
    /// Pending HALTCNT halt request consumed by the CPU wrapper.
    pub halt_requested: bool,
    /// Global timer cycle phase used when newly enabling prescaled timers.
    #[serde(default)]
    pub timer_global_cycles: u32,
    /// Remaining cycles before a newly started SIO transfer begins shifting.
    #[serde(default)]
    pub sio_start_delay_cycles: u32,
    /// Remaining cycles before a newly asserted IRQ line reaches the CPU.
    #[serde(default)]
    pub irq_line_delay_cycles: u32,
    /// Enabled, pending IRQ sources after the previous CPU/peripheral step.
    #[serde(default)]
    pub irq_sources_were_asserted: u16,
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

impl GbaSaveState {
    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveStateError> {
        crate::platform::save_state::to_bytes(self)
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    ///
    /// Returns [`SaveStateError::IncompatibleVersion`] when the deserialized
    /// state's `version` field is unsupported.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveStateError> {
        let state: Self = crate::platform::save_state::from_bytes(bytes)?;
        crate::platform::save_state::check_version(state.version, &SUPPORTED_SAVESTATE_VERSIONS)?;
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
    /// Returns [`SaveStateError::IncompatibleVersion`] if the snapshot
    /// version is unsupported, or
    /// [`SaveStateError::RestoreFailed`] if the captured state cannot
    /// be applied to the current bus (e.g. region size mismatch).
    pub fn load_state(&mut self, state: &GbaSaveState) -> Result<(), SaveStateError> {
        crate::platform::save_state::check_version(state.version, &SUPPORTED_SAVESTATE_VERSIONS)?;
        let current_input = self.bus().keypad.pressed_mask();
        self.bus_mut()
            .restore_memory_state(&state.bus)
            .map_err(SaveStateError::RestoreFailed)?;
        let bus = self.bus_mut();
        bus.keypad.set_pressed_mask(current_input, &mut bus.ic);
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
    fn test_gba_savestate_version_is_7() {
        assert_eq!(GBA_SAVESTATE_VERSION, 7);
    }

    #[test]
    fn test_version_6_save_state_without_pending_apu_samples_loads() {
        let gba = make_gba();
        let save = gba.save_state();
        let mut json = serde_json::to_value(&save).expect("serialize save state");
        json["version"] =
            serde_json::json!(GBA_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE);
        let apu = json["bus"]["apu"]
            .as_object_mut()
            .expect("APU state should be an object");
        apu.remove("pending_samples");
        apu.insert(
            "pending_sample".to_string(),
            serde_json::json!([0.125, 0.5]),
        );
        let bytes = serde_json::to_vec(&json).expect("serialize legacy save state");

        let loaded = GbaSaveState::from_bytes(&bytes).expect("legacy save state should load");

        assert_eq!(
            loaded.version,
            GBA_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE
        );
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

    #[test]
    fn test_load_state_preserves_current_physical_button_state() {
        let mut gba = make_gba();
        gba.set_button(0, 0, true);
        let saved = gba.save_state();

        gba.set_button(0, 0, false);
        assert_eq!(
            gba.get_joypad_button_states(0) & 0x01,
            0,
            "test setup must release A before restoring"
        );

        gba.load_state(&saved).expect("restore should succeed");

        assert_eq!(
            gba.get_joypad_button_states(0) & 0x01,
            0,
            "loading a save state must not resurrect a stale physical A press"
        );
    }

    #[test]
    fn test_load_state_preserves_current_physical_shoulder_button_state() {
        let mut gba = make_gba();
        gba.set_button(0, 8, true);
        let saved = gba.save_state();

        gba.set_button(0, 8, false);
        assert_ne!(
            gba.bus().keypad.read_keyinput() & (1 << 9),
            0,
            "test setup must release L before restoring"
        );

        gba.load_state(&saved).expect("restore should succeed");

        assert_ne!(
            gba.bus().keypad.read_keyinput() & (1 << 9),
            0,
            "loading a save state must not resurrect a stale physical L press"
        );
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
            Err(SaveStateError::IncompatibleVersion { found, supported }) => {
                assert_eq!(found, 9999);
                assert_eq!(supported, SUPPORTED_SAVESTATE_VERSIONS.to_vec());
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
            Err(SaveStateError::IncompatibleVersion { found, supported }) => {
                assert_eq!(found, 9999);
                assert_eq!(supported, SUPPORTED_SAVESTATE_VERSIONS.to_vec());
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
            Err(SaveStateError::DeserializationFailed(_))
        ));
    }

    #[test]
    fn test_region_size_mismatch_returns_restore_error() {
        let mut gba = make_gba();
        let mut save = gba.save_state();
        // Truncate EWRAM to an invalid size.
        save.bus.ewram.truncate(16);

        let result = gba.load_state(&save);
        assert!(matches!(result, Err(SaveStateError::RestoreFailed(_))));
    }

    #[test]
    fn test_main_components_implement_stateful() {
        // The CPU snapshot is captured through the `Stateful` trait.
        fn assert_stateful<T: crate::platform::save_state::Stateful>() {}
        assert_stateful::<crate::gba::cpu::Arm7tdmi>();
    }

    /// Regenerates the committed golden save-state fixture used by
    /// `test_golden_save_state_v7_loads`.
    ///
    /// Appropriate only when `GBA_SAVESTATE_VERSION` is bumped. After running
    /// this helper you MUST manually add `"_neser_fixture_marker": "gba-v7-golden"`
    /// back into the JSON before recompressing, so that the sentinel assertion
    /// in the load test continues to detect future accidental regenerations.
    ///
    /// Writing is therefore opt-in, so `cargo test -- --include-ignored` cannot
    /// clobber the fixture (#3107):
    /// `NESER_REGENERATE_FIXTURES=1 cargo test --no-default-features --lib \
    ///   gba::console::save_state::tests::regenerate_golden_save_state_fixture -- --ignored`
    #[test]
    #[ignore = "regenerates a committed fixture; run manually with NESER_REGENERATE_FIXTURES=1"]
    fn regenerate_golden_save_state_fixture() {
        let mut gba = make_gba();
        for _ in 0..1000 {
            gba.run_tick_for_tests();
        }
        let bytes = gba.save_state().to_bytes().expect("serialize save state");
        let compressed = crate::platform::save_state::gzip_compress(&bytes);
        if !crate::platform::save_state::fixture_regeneration_enabled() {
            return;
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gba/console/testdata");
        std::fs::create_dir_all(&dir).expect("create testdata dir");
        std::fs::write(dir.join("savestate_golden_v7.json.gz"), compressed).expect("write fixture");
    }

    #[test]
    fn test_golden_save_state_v7_loads() {
        // Round-trip stability test: the committed fixture must deserialize and
        // restore successfully with the current loader. This proves the
        // serialized format is stable across refactors that do not bump
        // GBA_SAVESTATE_VERSION.
        //
        // The fixture carries a `_neser_fixture_marker` key that current code
        // never emits; serde ignores it on load (unknown-field tolerance). Its
        // presence here detects accidental regeneration -- if the marker
        // disappears the fixture has been silently replaced by current output
        // and no longer tests anything meaningful.
        //
        // Genuine backward compatibility (loading v6 states) is covered by
        // `test_version_6_save_state_without_pending_apu_samples_loads`.
        let compressed = include_bytes!("testdata/savestate_golden_v7.json.gz");
        let bytes = crate::platform::save_state::gzip_decompress(compressed);

        assert!(
            bytes
                .windows(b"\"_neser_fixture_marker\"".len())
                .any(|w| w == b"\"_neser_fixture_marker\""),
            "golden fixture must contain the sentinel; do not replace it with plain current output"
        );

        let state = GbaSaveState::from_bytes(&bytes).expect("golden save state should deserialize");
        assert_eq!(state.version, GBA_SAVESTATE_VERSION);

        let mut gba = make_gba();
        gba.load_state(&state)
            .expect("golden save state should load");
    }
}
