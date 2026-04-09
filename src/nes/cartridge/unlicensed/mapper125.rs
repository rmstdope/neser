//! Mapper 125 — Whirlwind Manu LH32 (FDS pirate conversion)
//!
//! Specifications:
//! - Mesen2 reference: `Core/NES/Mappers/Whirlwind/Lh32.h`
//! - NES 2.0 DB: mapper=125, submapper=0, mirroring=V
//!
//! Memory map (8 KB PRG pages):
//! - CPU `$6000-$7FFF`: Switchable 8 KB PRG-ROM bank (selected by register write to `$6000`)
//! - CPU `$8000-$9FFF`: Fixed PRG-ROM bank (4th from last, index -4)
//! - CPU `$A000-$BFFF`: Fixed PRG-ROM bank (3rd from last, index -3)
//! - CPU `$C000-$DFFF`: 8 KB PRG-RAM (work RAM for the FDS conversion)
//! - CPU `$E000-$FFFF`: Fixed PRG-ROM bank (last, index -1)
//!
//! PPU `$0000-$1FFF`: 8 KB CHR-RAM (no CHR-ROM banking)
//!
//! Register:
//! - Write to `$6000`: value selects the 8 KB PRG-ROM bank at `$6000-$7FFF`.
//!   Only `$6000` is decoded; `$6001-$7FFF` are not register writes.
//!
//! Mirroring: hard-wired from the iNES header; no software control.
//!
//! Known Limitations:
//! - No IRQ or expansion audio; the board has neither.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 125;
const PRG_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper125 {
    base: BaseMapper,
    /// PRG-ROM bank index mapped to `$6000-$7FFF`.
    prg_reg: u8,
    /// 8 KB work RAM mapped to `$C000-$DFFF`.
    wram: Vec<u8>,
}

impl Mapper125 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_prg_6000_banking();

        let mut mapper = Self {
            base,
            prg_reg: 0,
            wram: vec![0; 8 * 1024],
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        self.base.select_prg_page(0, -4);
        self.base.select_prg_page(1, -3);
        // Slot 2 ($C000-$DFFF) is WRAM — handled manually in read_prg/write_prg
        self.base.select_prg_page(3, -1);
        self.base.select_prg_6000_page(self.prg_reg as i16);
    }
}

impl Mapper for Mapper125 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_6000(addr).unwrap_or(0),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0x8000..=0xBFFF | 0xE000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000 => {
                self.prg_reg = value;
                self.apply_state();
            }
            0xC000..=0xDFFF => {
                self.wram[(addr - 0xC000) as usize] = value;
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&reg) = data.first() {
            self.prg_reg = reg;
            self.apply_state();
        }
    }

    fn reset(&mut self) {
        self.prg_reg = 0;
        self.apply_state();
    }

    fn wram_size(&self) -> usize {
        self.wram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.wram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(self.wram.len());
        self.wram[..len].copy_from_slice(&data[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const TEST_PRG_BANKS_8K: usize = 16; // 128 KB PRG-ROM (matches NES20DB entry)

    fn make_mapper() -> Mapper125 {
        Mapper125::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS_8K),
            vec![], // CHR-RAM
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_125_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, TEST_PRG_BANKS_8K),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 125 must be creatable via factory");
    }

    // ── Reset / power-on PRG layout ───────────────────────────────────────────

    #[test]
    fn reset_maps_fixed_prg_banks() {
        let mapper = make_mapper();
        // 16 banks: -4 = 12, -3 = 13, -1 = 15
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "$8000 should read bank -4 (12)"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            13,
            "$A000 should read bank -3 (13)"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "$E000 should read bank -1 (15)"
        );
    }

    #[test]
    fn reset_maps_prg_6000_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "$6000 should read bank 0 on reset"
        );
    }

    // ── $C000-$DFFF is PRG-RAM ────────────────────────────────────────────────

    #[test]
    fn c000_dfff_is_writable_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x42);
        mapper.write_prg(0xDFFF, 0xAB);
        assert_eq!(
            mapper.read_prg(0xC000),
            0x42,
            "WRAM at $C000 must be writable"
        );
        assert_eq!(
            mapper.read_prg(0xDFFF),
            0xAB,
            "WRAM at $DFFF must be writable"
        );
    }

    #[test]
    fn c000_dfff_is_independent_of_prg_rom() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "PRG-ROM at $8000 must be unchanged"
        );
    }

    #[test]
    fn c000_dfff_defaults_to_zero() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 0, "WRAM should default to 0");
        assert_eq!(mapper.read_prg(0xD000), 0, "WRAM should default to 0");
    }

    // ── Register at $6000 ─────────────────────────────────────────────────────

    #[test]
    fn write_to_6000_selects_prg_bank_at_6000() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 5);
        assert_eq!(
            mapper.read_prg(0x6000),
            5,
            "bank 5 should be mapped to $6000"
        );

        mapper.write_prg(0x6000, 10);
        assert_eq!(
            mapper.read_prg(0x6000),
            10,
            "bank 10 should be mapped to $6000"
        );
    }

    #[test]
    fn write_to_6001_is_not_a_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 5);
        mapper.write_prg(0x6001, 10);
        assert_eq!(
            mapper.read_prg(0x6000),
            5,
            "$6001 write must not change the PRG register"
        );
    }

    #[test]
    fn register_write_does_not_affect_fixed_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "$8000 must remain fixed at bank -4"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            13,
            "$A000 must remain fixed at bank -3"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "$E000 must remain fixed at bank -1"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_is_hard_wired_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring must match the header"
        );
    }

    // ── Soft reset ────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_prg_bank_to_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 7);
        assert_eq!(mapper.read_prg(0x6000), 7, "precondition: PRG bank 7");

        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "reset must restore PRG bank 0 at $6000"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_round_trips_prg_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 9);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x6000), 9, "restored PRG bank must be 9");
    }

    #[test]
    fn wram_snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x42);
        mapper.write_prg(0xC100, 0xAB);
        let snapshot = mapper.wram_snapshot();

        let mut restored = make_mapper();
        restored.load_wram_snapshot(&snapshot);

        assert_eq!(
            restored.read_prg(0xC000),
            0x42,
            "WRAM at $C000 must be restored"
        );
        assert_eq!(
            restored.read_prg(0xC100),
            0xAB,
            "WRAM at $C100 must be restored"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_mapper_hardware() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();

        assert!(!caps.has_irq, "no IRQ on this board");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_dynamic_mirroring, "mirroring is hard-wired");
        assert_eq!(caps.prg_bank_size_kb, 8, "PRG bank size is 8 KB");
    }

    // ── WRAM size ─────────────────────────────────────────────────────────────

    #[test]
    fn wram_size_is_8kb() {
        let mapper = make_mapper();
        assert_eq!(mapper.wram_size(), 8 * 1024, "WRAM must be 8 KB");
    }
}
