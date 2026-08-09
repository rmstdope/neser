//! Game Boy save-state serialization.
//!
//! Defines a versioned [`GbSaveState`] struct that captures the full emulator
//! state for both DMG and CGB models.  Serialised as JSON (matching the NES
//! save-state format) via `to_bytes()` / `from_bytes()`.
//!
//! Bus capture/restore methods live in `dmg_bus.rs` and `cgb_bus.rs` (where
//! private fields are accessible).  CPU capture/restore lives here since
//! `Sm83` fields are `pub`.

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::gb::apu::Apu;
use crate::gb::bus::hdma::HdmaState;
use crate::gb::cpu::{Registers, Sm83};
use crate::gb::input::joypad::Joypad;
use crate::gb::model::DmgModel;
use crate::gb::ppu::Ppu;
use crate::gb::sgb::SgbState;
use crate::gb::timer::Timer;
use crate::platform::save_state::{SaveStateError, Stateful};

/// Current save-state format version for Game Boy.
/// Increment this when making breaking changes to the state format.
pub const GB_SAVESTATE_VERSION: u32 = 6;
const GB_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE: u32 = 5;
const GB_LEGACY_SAVESTATE_VERSION_WITHOUT_CGB_RTC_PHASE: u32 = 4;

/// Save-state format versions this build can load (current plus legacy).
const SUPPORTED_SAVESTATE_VERSIONS: [u32; 3] = [
    GB_SAVESTATE_VERSION,
    GB_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE,
    GB_LEGACY_SAVESTATE_VERSION_WITHOUT_CGB_RTC_PHASE,
];

/// Identifies which bus variant was active when the state was saved.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GbBusType {
    Dmg,
    Cgb,
}

/// SM83 CPU state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sm83State {
    pub regs: Registers,
    pub ime: bool,
    pub halted: bool,
    #[serde(default)]
    pub stopped: bool,
    pub halt_bug: bool,
    pub ime_pending: bool,
    pub cycles: u64,
}

/// DMG/CGB bus state snapshot (excluding the cartridge itself).
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusState {
    pub bus_type: GbBusType,
    pub ppu: Ppu,
    #[serde_as(as = "[_; 0x8000]")]
    pub wram: [u8; 0x8000],
    #[serde_as(as = "[_; 0x7F]")]
    pub hram: [u8; 0x7F],
    pub timer: Timer,
    pub joypad: Joypad,
    pub apu: Apu,
    pub if_reg: u8,
    pub ie_reg: u8,
    pub dma_active: bool,
    pub dma_source: u8,
    pub dma_position: u8,
    pub dma_oam_blocked: bool,
    // CGB HDMA state (None for DMG)
    pub hdma: Option<HdmaState>,
    // CGB WRAM bank register (None for DMG)
    pub svbk: Option<u8>,
    // CGB KEY1 register (None for DMG)
    pub key1: Option<u8>,
    // CGB APU tick accumulator for double-speed mode (None for DMG)
    pub apu_tick_accumulator: Option<u8>,
    // CGB cartridge RTC tick accumulator for double-speed mode (None for DMG)
    #[serde(default)]
    pub rtc_tick_accumulator: Option<u16>,
    // CGB undocumented registers $FF72-$FF75 (None for DMG)
    pub ff72: Option<u8>,
    pub ff73: Option<u8>,
    pub ff74: Option<u8>,
    pub ff75: Option<u8>,
    // CGB KEY0 register ($FF4C) - DMG compatibility mode (None for DMG)
    #[serde(default)]
    pub key0: Option<u8>,
    #[serde(default)]
    pub key0_locked: Option<bool>,
    // CGB $FEA0-$FEFF extra OAM RAM (None for DMG and older save states)
    #[serde(default)]
    pub cgb_extra_oam: Option<Vec<u8>>,
    // DMG-only fields (None for CGB)
    pub boot_rom_active: Option<bool>,
    pub sb: Option<u8>,
    pub sc: Option<u8>,
    pub serial_buf: Option<Vec<u8>>,
    pub serial_bits_remaining: Option<u8>,
    pub serial_master_clock: Option<bool>,
    pub model: Option<DmgModel>,
    // Optional minimal SGB command/input state (None for normal DMG/CGB and older save states)
    #[serde(default)]
    pub sgb: Option<SgbState>,
}

/// Complete Game Boy emulator state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GbSaveState {
    /// Version of the save-state format.
    pub version: u32,
    /// CPU state.
    pub cpu: Sm83State,
    /// Bus state (PPU, APU, timer, joypad, RAM, etc.).
    pub bus: BusState,
    /// Cartridge RAM snapshot (battery-backed SRAM).
    pub cart_ram: Vec<u8>,
    /// MBC register state (opaque bytes).
    pub mbc_state: Vec<u8>,
}

impl GbSaveState {
    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveStateError> {
        crate::platform::save_state::to_bytes(self)
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveStateError> {
        let state: Self = crate::platform::save_state::from_bytes(bytes)?;
        crate::platform::save_state::check_version(state.version, &SUPPORTED_SAVESTATE_VERSIONS)?;
        Ok(state)
    }
}

// ── Convenience save / load for Gb<DmgBus> ─────────────────────────────────

use super::Gb;
use crate::gb::bus::DmgBus;

impl Gb<DmgBus> {
    /// Capture a full save-state snapshot.
    pub fn save_state(&self) -> GbSaveState {
        GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: self.cpu.capture_state(),
            bus: self.cpu.bus.capture_bus_state(),
            cart_ram: self.cpu.bus.cart_ram_snapshot(),
            mbc_state: self.cpu.bus.mbc_state_snapshot(),
        }
    }

    /// Restore state from a save-state snapshot.
    pub fn load_state(&mut self, state: &GbSaveState) -> Result<(), SaveStateError> {
        self.cpu.restore_state(&state.cpu);
        self.cpu
            .bus
            .restore_bus_state(&state.bus)
            .map_err(SaveStateError::RestoreFailed)?;
        self.reconcile_stop_display_after_state_load();
        self.cpu.bus.restore_cart_ram(&state.cart_ram);
        self.cpu.bus.restore_mbc_state(&state.mbc_state);
        Ok(())
    }
}

// ── Capture / Restore helpers for SM83 CPU ─────────────────────────────────

impl<B: crate::gb::bus::GbBus> Stateful for Sm83<B> {
    type State = Sm83State;

    /// Capture the CPU state for serialization.
    fn capture_state(&self) -> Sm83State {
        Sm83State {
            regs: self.regs,
            ime: self.ime,
            halted: self.halted,
            stopped: self.stopped,
            halt_bug: self.halt_bug,
            ime_pending: self.ime_pending(),
            cycles: self.cycles(),
        }
    }

    /// Restore CPU state from a deserialized snapshot.
    fn restore_state(&mut self, state: &Sm83State) {
        self.regs = state.regs;
        self.ime = state.ime;
        self.halted = state.halted;
        self.stopped = state.stopped;
        self.halt_bug = state.halt_bug;
        self.set_ime_pending(state.ime_pending);
        self.set_cycles(state.cycles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::bus::{CgbBus, DmgBus, GbBus};
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::console::Gb;
    use crate::gb::model::{CgbModel, DmgModel};
    use crate::gb::ppu::StopDisplayMode;

    fn minimal_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn minimal_cgb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0143] = 0xC0; // CGB-only flag
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn make_dmg() -> Gb<DmgBus> {
        let cart = load_cartridge(&minimal_rom()).expect("valid ROM");
        Gb::new(DmgBus::new(cart, DmgModel::DmgB))
    }

    fn make_cgb() -> Gb<CgbBus> {
        let cart = load_cartridge(&minimal_cgb_rom()).expect("valid ROM");
        let mut gb = Gb::new(CgbBus::new(cart, CgbModel::default(), true));
        gb.cpu.reset_registers_cgb();
        gb
    }

    // ── Version checks ─────────────────────────────────────────────────────

    #[test]
    fn test_gb_savestate_version_is_6() {
        assert_eq!(GB_SAVESTATE_VERSION, 6);
    }

    #[test]
    fn test_version_5_save_state_without_pending_apu_samples_loads() {
        let gb = make_cgb();
        let save = GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: gb.cpu.capture_state(),
            bus: gb.cpu.bus.capture_bus_state(),
            cart_ram: gb.cpu.bus.cart_ram_snapshot(),
            mbc_state: gb.cpu.bus.mbc_state_snapshot(),
        };
        let mut json = serde_json::to_value(&save).expect("serialize save state");
        json["version"] =
            serde_json::json!(GB_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE);
        let apu = json["bus"]["apu"]
            .as_object_mut()
            .expect("APU state should be an object");
        apu.remove("pending_samples");
        apu.insert("pending_sample".to_string(), serde_json::json!(0.125));
        let bytes = serde_json::to_vec(&json).expect("serialize legacy save state");

        let loaded = GbSaveState::from_bytes(&bytes).expect("legacy save state should load");

        assert_eq!(
            loaded.version,
            GB_LEGACY_SAVESTATE_VERSION_WITH_SINGLE_PENDING_APU_SAMPLE
        );
    }

    #[test]
    fn test_version_4_save_state_without_cgb_rtc_phase_loads() {
        let gb = make_cgb();
        let save = GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: gb.cpu.capture_state(),
            bus: gb.cpu.bus.capture_bus_state(),
            cart_ram: gb.cpu.bus.cart_ram_snapshot(),
            mbc_state: gb.cpu.bus.mbc_state_snapshot(),
        };
        let mut json = serde_json::to_value(&save).expect("serialize save state");
        json["version"] = serde_json::json!(GB_LEGACY_SAVESTATE_VERSION_WITHOUT_CGB_RTC_PHASE);
        json["bus"]
            .as_object_mut()
            .expect("bus state should be an object")
            .remove("rtc_tick_accumulator");
        let bytes = serde_json::to_vec(&json).expect("serialize legacy save state");

        let loaded = GbSaveState::from_bytes(&bytes).expect("legacy save state should load");

        assert_eq!(
            loaded.version,
            GB_LEGACY_SAVESTATE_VERSION_WITHOUT_CGB_RTC_PHASE
        );
        assert_eq!(loaded.bus.rtc_tick_accumulator, None);
    }

    // ── DMG round-trip ─────────────────────────────────────────────────────

    #[test]
    fn test_dmg_save_state_roundtrip() {
        let mut gb = make_dmg();
        for _ in 0..10 {
            gb.step();
        }

        let save = gb.save_state();
        let bytes = save.to_bytes().expect("serialization should succeed");
        let loaded = GbSaveState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(loaded.version, GB_SAVESTATE_VERSION);
        assert_eq!(loaded.cpu.regs, gb.cpu.regs);
        assert_eq!(loaded.bus.bus_type, GbBusType::Dmg);
    }

    // ── CGB round-trip ─────────────────────────────────────────────────────

    #[test]
    fn test_cgb_save_state_roundtrip() {
        let mut gb = make_cgb();
        for _ in 0..10 {
            gb.step();
        }

        let cpu_state = gb.cpu.capture_state();
        let bus_state = gb.cpu.bus.capture_bus_state();

        let save = GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: cpu_state,
            bus: bus_state,
            cart_ram: gb.cpu.bus.cart_ram_snapshot(),
            mbc_state: gb.cpu.bus.mbc_state_snapshot(),
        };

        let bytes = save.to_bytes().expect("serialization should succeed");
        let loaded = GbSaveState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(loaded.version, GB_SAVESTATE_VERSION);
        assert_eq!(loaded.cpu.regs, gb.cpu.regs);
        assert_eq!(loaded.bus.bus_type, GbBusType::Cgb);
    }

    // ── Version mismatch ───────────────────────────────────────────────────

    #[test]
    fn test_incompatible_version_error() {
        let mut gb = make_dmg();
        gb.step();

        let mut save = gb.save_state();
        save.version = 9999;

        let bytes = serde_json::to_vec(&save).unwrap();
        let result = GbSaveState::from_bytes(&bytes);
        assert!(result.is_err());
        match result {
            Err(SaveStateError::IncompatibleVersion { found, supported }) => {
                assert_eq!(found, 9999);
                assert_eq!(supported, SUPPORTED_SAVESTATE_VERSIONS.to_vec());
            }
            _ => panic!("Expected IncompatibleVersion error"),
        }
    }

    // ── Invalid data ───────────────────────────────────────────────────────

    #[test]
    fn test_invalid_json_returns_deserialization_error() {
        let result = GbSaveState::from_bytes(b"not valid json");
        assert!(matches!(
            result,
            Err(SaveStateError::DeserializationFailed(_))
        ));
    }

    // ── Restore round-trip ─────────────────────────────────────────────────

    #[test]
    fn test_dmg_capture_restore_preserves_cpu_registers() {
        let mut gb = make_dmg();
        for _ in 0..10 {
            gb.step();
        }
        let original_regs = gb.cpu.regs;
        let state = gb.cpu.capture_state();

        // Change registers
        gb.cpu.regs.a = 0xFF;
        gb.cpu.regs.pc = 0x1234;

        // Restore
        gb.cpu.restore_state(&state);
        assert_eq!(gb.cpu.regs, original_regs);
    }

    #[test]
    fn test_dmg_capture_restore_preserves_bus_state() {
        let mut gb = make_dmg();
        for _ in 0..10 {
            gb.step();
        }
        let bus_state = gb.cpu.bus.capture_bus_state();

        // Modify WRAM
        gb.cpu.bus.write(0xC100, 0xAB);
        assert_eq!(gb.cpu.bus.read(0xC100), 0xAB);

        // Restore
        gb.cpu
            .bus
            .restore_bus_state(&bus_state)
            .expect("restore should succeed");
        assert_eq!(gb.cpu.bus.read(0xC100), 0x00);
    }

    #[test]
    fn test_dmg_load_state_reconciles_stopped_cpu_display_mode() {
        // Given: an older-style stopped CPU state whose PPU snapshot has no STOP display override.
        let mut gb = make_dmg();
        gb.cpu.stopped = true;
        let save = gb.save_state();

        // When: the state is loaded.
        let mut restored = make_dmg();
        restored.load_state(&save).expect("load state");

        // Then: the DMG STOP display is restored to the blank white output.
        assert_eq!(
            restored.cpu.bus.ppu().screen_buffer().get_pixel(0, 0),
            (0xFF, 0xFF, 0xFF)
        );
        assert_eq!(
            restored.cpu.bus.ppu().stop_display_mode(),
            StopDisplayMode::SolidWhite
        );
    }

    #[test]
    fn test_cgb_load_state_preserves_saved_stop_display_mode() {
        // Given: a CGB Mode 3 STOP state saved after timing has advanced away from Mode 3.
        let mut gb = make_cgb();
        gb.cpu.stopped = true;
        gb.cpu
            .bus
            .ppu_mut()
            .enter_stop_display_mode(StopDisplayMode::PreserveCurrent);
        let save = GbSaveState {
            version: GB_SAVESTATE_VERSION,
            cpu: gb.cpu.capture_state(),
            bus: gb.cpu.bus.capture_bus_state(),
            cart_ram: gb.cpu.bus.cart_ram_snapshot(),
            mbc_state: gb.cpu.bus.mbc_state_snapshot(),
        };

        // When: the state is loaded.
        let mut restored = make_cgb();
        restored.cpu.restore_state(&save.cpu);
        restored
            .cpu
            .bus
            .restore_bus_state(&save.bus)
            .expect("restore bus");
        restored.reconcile_stop_display_after_state_load();

        // Then: reconciliation trusts the serialized display mode instead of recomputing black.
        assert_eq!(
            restored.cpu.bus.ppu().stop_display_mode(),
            StopDisplayMode::PreserveCurrent
        );
    }

    #[test]
    fn test_cgb_capture_restore_preserves_cpu_registers() {
        let mut gb = make_cgb();
        for _ in 0..10 {
            gb.step();
        }
        let original_regs = gb.cpu.regs;
        let state = gb.cpu.capture_state();

        gb.cpu.regs.a = 0xFF;
        gb.cpu.restore_state(&state);
        assert_eq!(gb.cpu.regs, original_regs);
    }

    // ── Bus-type mismatch ──────────────────────────────────────────────────

    #[test]
    fn test_dmg_restore_rejects_cgb_bus_state() {
        let mut dmg = make_dmg();
        let mut cgb = make_cgb();
        for _ in 0..5 {
            dmg.step();
            cgb.step();
        }
        let cgb_state = cgb.cpu.bus.capture_bus_state();
        let result = dmg.cpu.bus.restore_bus_state(&cgb_state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bus type mismatch"));
    }

    #[test]
    fn test_cgb_restore_rejects_dmg_bus_state() {
        let mut dmg = make_dmg();
        let mut cgb = make_cgb();
        for _ in 0..5 {
            dmg.step();
            cgb.step();
        }
        let dmg_state = dmg.cpu.bus.capture_bus_state();
        let result = cgb.cpu.bus.restore_bus_state(&dmg_state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bus type mismatch"));
    }

    #[test]
    fn test_main_components_implement_stateful() {
        // The CPU snapshot is captured through the `Stateful` trait.
        fn assert_stateful<T: Stateful>() {}
        assert_stateful::<Sm83<DmgBus>>();
        assert_stateful::<Sm83<CgbBus>>();
    }

    /// Regenerates the committed golden save-state fixture used by
    /// `test_golden_save_state_v6_loads`.
    ///
    /// Appropriate only when `GB_SAVESTATE_VERSION` is bumped. After running
    /// this helper you MUST manually add `"_neser_fixture_marker": "gb-v6-golden"`
    /// back into the JSON before recompressing, so that the sentinel assertion
    /// in the load test continues to detect future accidental regenerations.
    ///
    /// Writing is therefore opt-in, so `cargo test -- --include-ignored` cannot
    /// clobber the fixture (#3107):
    /// `NESER_REGENERATE_FIXTURES=1 cargo test --no-default-features --lib \
    ///   gb::console::save_state::tests::regenerate_golden_save_state_fixture -- --ignored`
    #[test]
    #[ignore = "regenerates a committed fixture; run manually with NESER_REGENERATE_FIXTURES=1"]
    fn regenerate_golden_save_state_fixture() {
        let mut gb = make_dmg();
        for _ in 0..1000 {
            gb.step();
        }
        let bytes = gb.save_state().to_bytes().expect("serialize save state");
        let compressed = crate::platform::save_state::gzip_compress(&bytes);
        if !crate::platform::save_state::fixture_regeneration_enabled() {
            return;
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gb/console/testdata");
        std::fs::create_dir_all(&dir).expect("create testdata dir");
        std::fs::write(dir.join("savestate_golden_v6.json.gz"), compressed).expect("write fixture");
    }

    #[test]
    fn test_golden_save_state_v6_loads() {
        // Round-trip stability test: the committed fixture must deserialize and
        // restore successfully with the current loader. This proves the
        // serialized format is stable across refactors that do not bump
        // GB_SAVESTATE_VERSION.
        //
        // The fixture carries a `_neser_fixture_marker` key that current code
        // never emits; serde ignores it on load (unknown-field tolerance). Its
        // presence here detects accidental regeneration -- if the marker
        // disappears the fixture has been silently replaced by current output
        // and no longer tests anything meaningful.
        //
        // Genuine backward compatibility (loading v4 and v5 states) is covered
        // by `test_version_4_save_state_without_cgb_rtc_phase_loads` and
        // `test_version_5_save_state_without_pending_apu_samples_loads`.
        let compressed = include_bytes!("testdata/savestate_golden_v6.json.gz");
        let bytes = crate::platform::save_state::gzip_decompress(compressed);

        assert!(
            bytes
                .windows(b"\"_neser_fixture_marker\"".len())
                .any(|w| w == b"\"_neser_fixture_marker\""),
            "golden fixture must contain the sentinel; do not replace it with plain current output"
        );

        let state = GbSaveState::from_bytes(&bytes).expect("golden save state should deserialize");
        assert_eq!(state.version, GB_SAVESTATE_VERSION);

        let mut gb = make_dmg();
        gb.load_state(&state)
            .expect("golden save state should load");
    }
}
