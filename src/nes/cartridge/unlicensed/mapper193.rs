//! Mapper 193 - NTDEC TC-112
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 193;
const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 2 * 1024;

/// Mapper 193 - NTDEC TC-112
///
/// Hardware: NTDEC TC-112 mapper IC.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_193>
/// - PRG-ROM: Up to 128 KB; 8 KB switchable at $8000–$9FFF; $A000–$FFFF fixed to last three banks
/// - CHR: 8 KB (4 × 2 KB pages); $0000–$0FFF from CHR reg 0 (4 KB), $1000–$17FF from CHR reg 1,
///   $1800–$1FFF from CHR reg 2
/// - Mirroring: Software-controlled via bit 0 of register at $6004
///
/// Registers (write to $6000–$7FFF, decoded with mask $E007; low 3 bits select register):
///
/// ```text
/// addr & 7   Register
/// ────────   ─────────────────────────────────────────────────────────────────
///    0       $6000: [CCCC CC..] CHR Reg 0 → 4 KB at $0000 (banks value>>1 and value>>1+1)
///    1       $6001: [CCCC CCC.] CHR Reg 1 → 2 KB at $1000 (bank = value >> 1)
///    2       $6002: [CCCC CCC.] CHR Reg 2 → 2 KB at $1800 (bank = value >> 1)
///    3       $6003: [.... PPPP] PRG Reg   → 8 KB at $8000 (bank = value & 0x0F)
///    4       $6004: [.... ...M] Mirroring → 0 = Vertical, 1 = Horizontal
/// ```
pub struct Mapper193 {
    base: BaseMapper,
    chr0: u8,
    chr1: u8,
    chr2: u8,
    prg: u8,
    mirroring: bool,
    initial_mirroring: NametableLayout,
}

impl Mapper193 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let initial_mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            chr0: 0,
            chr1: 0,
            chr2: 0,
            prg: 0,
            mirroring: matches!(initial_mirroring, NametableLayout::Horizontal),
            initial_mirroring,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        // PRG: slot 0 switchable ($8000-$9FFF), slots 1/2/3 fixed at -3/-2/-1
        self.base.select_prg_page(0, (self.prg & 0x0F) as i16);
        self.base.select_prg_page(1, -3);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR: $6000 selects 4 KB at $0000 (two consecutive 2 KB banks)
        let chr0_bank = (self.chr0 >> 1) as i16;
        self.base.select_chr_page(0, chr0_bank);
        self.base.select_chr_page(1, chr0_bank + 1);
        // $6001 selects 2 KB at $1000; $6002 selects 2 KB at $1800
        self.base.select_chr_page(2, (self.chr1 >> 1) as i16);
        self.base.select_chr_page(3, (self.chr2 >> 1) as i16);

        if self.mirroring {
            self.base.set_mirroring(NametableLayout::Horizontal);
        } else {
            self.base.set_mirroring(NametableLayout::Vertical);
        }
    }
}

impl Mapper for Mapper193 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if !(0x6000..=0x7FFF).contains(&addr) {
            return;
        }
        match addr & 0x07 {
            0 => {
                self.chr0 = value;
                let bank = (value >> 1) as i16;
                self.base.select_chr_page(0, bank);
                self.base.select_chr_page(1, bank + 1);
            }
            1 => {
                self.chr1 = value;
                self.base.select_chr_page(2, (value >> 1) as i16);
            }
            2 => {
                self.chr2 = value;
                self.base.select_chr_page(3, (value >> 1) as i16);
            }
            3 => {
                self.prg = value;
                self.base.select_prg_page(0, (value & 0x0F) as i16);
            }
            4 => {
                self.mirroring = (value & 1) != 0;
                if self.mirroring {
                    self.base.set_mirroring(NametableLayout::Horizontal);
                } else {
                    self.base.set_mirroring(NametableLayout::Vertical);
                }
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.chr0,
            self.chr1,
            self.chr2,
            self.prg,
            self.base.mirroring().to_snapshot_byte(),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.chr0 = data[0];
            self.chr1 = data[1];
            self.chr2 = data[2];
            self.prg = data[3];
            let layout = NametableLayout::from_snapshot_byte(data[4]);
            self.mirroring = matches!(layout, NametableLayout::Horizontal);
            self.apply_state();
        }
    }

    fn reset(&mut self) {
        self.chr0 = 0;
        self.chr1 = 0;
        self.chr2 = 0;
        self.prg = 0;
        self.mirroring = matches!(self.initial_mirroring, NametableLayout::Horizontal);
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 8 × 8 KB = 64 KB PRG; 8 × 2 KB = 16 KB CHR
    const PRG_BANKS: usize = 8;
    const CHR_BANKS: usize = 8;

    fn make_mapper() -> Mapper193 {
        Mapper193::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory ──────────────────────────────────────────────────────────────

    #[test]
    fn mapper193_registers_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 193 must be creatable via factory");
    }

    // ── PRG – power-on state ─────────────────────────────────────────────────

    #[test]
    fn prg_slot0_defaults_to_bank0_at_8000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should read bank 0 on reset"
        );
    }

    #[test]
    fn prg_slots_1_2_3_fixed_to_last_three_banks() {
        let mapper = make_mapper();
        // 8 banks → -3 = 5, -2 = 6, -1 = 7
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 should be fixed to bank 5 (-3)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should be fixed to bank 6 (-2)"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should be fixed to bank 7 (-1)"
        );
    }

    // ── PRG – switching ───────────────────────────────────────────────────────

    #[test]
    fn write_to_6003_selects_prg_slot0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 should read bank 3 after writing 3 to $6003"
        );
    }

    #[test]
    fn prg_fixed_banks_unchanged_after_slot0_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 2);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 must remain fixed at bank 5"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 must remain fixed at bank 7"
        );
    }

    #[test]
    fn prg_reg_uses_only_low_4_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 0xF1); // bit 0 only from low nibble = bank 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG reg must mask to low 4 bits"
        );
    }

    // ── CHR – power-on state ─────────────────────────────────────────────────

    #[test]
    fn chr_initial_state_slots_mapped_correctly() {
        let mut mapper = make_mapper();
        // chr0=0: slot0=0, slot1=1; chr1=0: slot2=0; chr2=0: slot3=0
        assert_eq!(mapper.read_chr(0x0000), 0, "$0000 should read CHR bank 0");
        assert_eq!(mapper.read_chr(0x0800), 1, "$0800 should read CHR bank 1");
        assert_eq!(mapper.read_chr(0x1000), 0, "$1000 should read CHR bank 0");
        assert_eq!(mapper.read_chr(0x1800), 0, "$1800 should read CHR bank 0");
    }

    // ── CHR – switching ───────────────────────────────────────────────────────

    #[test]
    fn write_to_6000_selects_4kb_at_0000() {
        let mut mapper = make_mapper();
        // Write value 4 (0x04): bank = 4>>1 = 2; slots 0=2, 1=3
        mapper.write_prg(0x6000, 0x04);
        assert_eq!(mapper.read_chr(0x0000), 2, "$0000 should read CHR bank 2");
        assert_eq!(mapper.read_chr(0x0800), 3, "$0800 should read CHR bank 3");
    }

    #[test]
    fn write_to_6001_selects_2kb_at_1000() {
        let mut mapper = make_mapper();
        // Write value 8 (0x08): bank = 8>>1 = 4; slot 2=4
        mapper.write_prg(0x6001, 0x08);
        assert_eq!(mapper.read_chr(0x1000), 4, "$1000 should read CHR bank 4");
        // $1800 (slot 3) must be unchanged
        assert_eq!(
            mapper.read_chr(0x1800),
            0,
            "$1800 must remain at CHR bank 0"
        );
    }

    #[test]
    fn write_to_6002_selects_2kb_at_1800() {
        let mut mapper = make_mapper();
        // Write value 10 (0x0A): bank = 10>>1 = 5; slot 3=5
        mapper.write_prg(0x6002, 0x0A);
        assert_eq!(mapper.read_chr(0x1800), 5, "$1800 should read CHR bank 5");
        // $1000 (slot 2) must be unchanged
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "$1000 must remain at CHR bank 0"
        );
    }

    #[test]
    fn chr_reg0_uses_value_shifted_right_1_for_bank() {
        let mut mapper = make_mapper();
        // Odd value: 0x05 >> 1 = 2 (bit 0 discarded)
        mapper.write_prg(0x6000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR reg 0 bank = value >> 1");
        assert_eq!(
            mapper.read_chr(0x0800),
            3,
            "CHR reg 0 slot 1 = (value >> 1) + 1"
        );
    }

    // ── Register address mirroring (mask $E007 → use addr & 7) ──────────────

    #[test]
    fn register_address_mirrors_within_6000_7fff() {
        let mut mapper = make_mapper();
        // $6008 & 7 == 0 → same as $6000 (CHR reg 0)
        mapper.write_prg(0x6008, 0x06);
        assert_eq!(mapper.read_chr(0x0000), 3, "$6008 should act as CHR reg 0");
    }

    // ── No PRG-RAM ────────────────────────────────────────────────────────────

    #[test]
    fn no_prg_ram_at_6000_7fff() {
        let mapper = make_mapper();
        // No PRG-RAM → reads should use open bus, not SRAM
        // Verify read_prg_open_bus returns open bus (0xAA) for $6000 range
        let result = mapper.read_prg_open_bus(0x6000, 0xAA);
        assert_eq!(result, 0xAA, "No PRG-RAM: $6000 reads must return open bus");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn write_to_6004_bit0_1_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6004, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$6004 bit 0 = 1 → Horizontal"
        );
    }

    #[test]
    fn write_to_6004_bit0_0_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6004, 0x01); // first set horizontal
        mapper.write_prg(0x6004, 0x00); // then clear to vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$6004 bit 0 = 0 → Vertical"
        );
    }

    // ── Writes above $8000 are ignored ───────────────────────────────────────

    #[test]
    fn writes_above_8000_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 5); // should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG reg must not be written via $8000+"
        );
    }

    // ── Snapshot / Restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_regs() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04); // chr0
        mapper.write_prg(0x6001, 0x08); // chr1
        mapper.write_prg(0x6002, 0x0A); // chr2
        mapper.write_prg(0x6003, 0x03); // prg
        mapper.write_prg(0x6004, 0x01); // mirroring = H

        let snap = mapper.registers_snapshot();

        let mut fresh = make_mapper();
        fresh.restore_registers(&snap);

        assert_eq!(fresh.read_chr(0x0000), 2, "CHR slot 0 restored");
        assert_eq!(fresh.read_chr(0x0800), 3, "CHR slot 1 restored");
        assert_eq!(fresh.read_chr(0x1000), 4, "CHR slot 2 restored");
        assert_eq!(fresh.read_chr(0x1800), 5, "CHR slot 3 restored");
        assert_eq!(fresh.read_prg(0x8000), 3, "PRG slot 0 restored");
        assert_eq!(
            fresh.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring restored"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 5);
        mapper.write_prg(0x6000, 0x06);
        mapper.write_prg(0x6004, 0x01);

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 must reset to bank 0"
        );
        // Header mirroring (Horizontal) restored on reset
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must reset to header value"
        );
    }
}
