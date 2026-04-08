//! Mapper 274 – BMC-80013B
//!
//! Specifications:
//! - Primary reference: Mesen2 `Core/NES/Mappers/Unlicensed/Bmc80013B.h`
//!   (NESdev wiki page unavailable at time of implementation)
//!
//! # Hardware overview
//!
//! Used by multicart boards labeled BMC-80013B.
//!
//! - PRG-ROM: Two switchable 16 KiB windows: $8000–$BFFF (page 0) and $C000–$FFFF (page 1).
//! - CHR-ROM: 8 KiB window at $0000–$1FFF, fixed to bank 0.
//! - Mirroring: controlled by bit 4 of reg0 (0 = Horizontal, 1 = Vertical).
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Register writes
//!
//! The write address selects the target register via bits [14:13]:
//!
//! | Address range  | (addr >> 13) & 3 | Effect                                      |
//! |----------------|-----------------|---------------------------------------------|
//! | $8000–$9FFF    | 0               | reg0 = value                                |
//! | $A000–$BFFF    | 1               | reg1 = value; mode = 1                      |
//! | $C000–$DFFF    | 2               | reg1 = value; mode = 2                      |
//! | $E000–$FFFF    | 3               | reg1 = value; mode = 3                      |
//!
//! # PRG banking
//!
//! - PRG page 1 ($C000–$FFFF): `reg1 & 0x7F` (always).
//! - PRG page 0 ($8000–$BFFF):
//!   - If `mode & 0x02` (mode = 2 or 3): `(reg0 & 0x0F) | (reg1 & 0x70)`
//!   - Otherwise (mode = 0 or 1):        `reg0 & 0x03`
//!
//! # CHR banking
//!
//! Fixed to bank 0; CHR-ROM content at $0000–$1FFF is always from bank 0.
//!
//! # Mirroring
//!
//! - `reg0 & 0x10 == 0` → Horizontal
//! - `reg0 & 0x10 != 0` → Vertical
//!
//! # Power-on / reset state
//!
//! reg0 = 0, reg1 = 0, mode = 0: PRG page 0 = 0, PRG page 1 = 0, Horizontal mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 274;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 274 – BMC-80013B multicart.
///
/// Three internal state variables:
/// - `reg0`: written by $8000–$9FFF; controls low PRG page 0 bits and mirroring.
/// - `reg1`: written by $A000–$FFFF; controls PRG page 1 and high PRG page 0 bits.
/// - `mode`: records the last write range for reg1 (0–3); gates PRG page 0 formula.
pub struct Mapper274 {
    base: BaseMapper,
    reg0: u8,
    reg1: u8,
    mode: u8,
}

impl Mapper274 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            reg0: 0,
            reg1: 0,
            mode: 0,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        let prg0: i16 = if self.mode & 0x02 != 0 {
            ((self.reg0 & 0x0F) | (self.reg1 & 0x70)) as i16
        } else {
            (self.reg0 & 0x03) as i16
        };
        let prg1 = (self.reg1 & 0x7F) as i16;
        self.base.select_prg_page(0, prg0);
        self.base.select_prg_page(1, prg1);
        self.base.select_chr_page(0, 0);
        self.base.set_mirroring_hv((self.reg0 & 0x10) == 0);
    }
}

impl Mapper for Mapper274 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        let reg = ((addr >> 13) & 0x03) as u8;
        if reg == 0 {
            self.reg0 = value;
        } else {
            self.reg1 = value;
            self.mode = reg;
        }
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg0, self.reg1, self.mode]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 3 {
            return;
        }
        self.reg0 = data[0];
        self.reg1 = data[1];
        self.mode = data[2] & 0x03;
        self.apply_state();
    }

    fn reset(&mut self) {
        self.reg0 = 0;
        self.reg1 = 0;
        self.mode = 0;
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to detect false-pass from modulo wrapping.
    const PRG_BANKS_16K: usize = 11;
    const CHR_BANKS_8K: usize = 7;

    fn make_mapper() -> Mapper274 {
        Mapper274::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_274_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 274 must be registered in factory");
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_pages_are_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xBFFF), 0, "$BFFF should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xFFFF), 0, "$FFFF should be PRG bank 0");
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank should be 0 at power-on"
        );
        assert_eq!(
            mapper.read_chr(0x1FFF),
            0,
            "CHR $1FFF should be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should be Horizontal at power-on (reg0 bit 4 = 0)"
        );
    }

    // ── Register address decoding ────────────────────────────────────────────

    #[test]
    fn write_8000_to_9fff_sets_reg0() {
        let mut mapper = make_mapper();
        // reg0 = 0x02: mode 0 → PRG page 0 = reg0 & 0x03 = 2
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2, "write to $8000 should set reg0");
        mapper.write_prg(0x9FFF, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 1, "write to $9FFF should set reg0");
    }

    #[test]
    fn write_a000_to_bfff_sets_reg1_and_mode_1() {
        let mut mapper = make_mapper();
        // mode 1 → PRG page 0 = reg0 & 0x03 = 0; PRG page 1 = reg1 & 0x7F = value & 0x7F
        mapper.write_prg(0xA000, 0x05);
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "write to $A000 should set reg1 (page 1 = 5)"
        );
        mapper.write_prg(0xBFFF, 0x03);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "write to $BFFF should set reg1 (page 1 = 3)"
        );
    }

    #[test]
    fn write_c000_to_dfff_sets_reg1_and_mode_2() {
        let mut mapper = make_mapper();
        // mode 2 → PRG page 0 = (reg0 & 0x0F) | (reg1 & 0x70); PRG page 1 = reg1 & 0x7F
        mapper.write_prg(0xC000, 0x05);
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "write to $C000 should set reg1 (page 1 = 5)"
        );
        mapper.write_prg(0xDFFF, 0x07);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "write to $DFFF should set reg1 (page 1 = 7)"
        );
    }

    #[test]
    fn write_e000_to_ffff_sets_reg1_and_mode_3() {
        let mut mapper = make_mapper();
        // mode 3 → PRG page 0 = (reg0 & 0x0F) | (reg1 & 0x70); PRG page 1 = reg1 & 0x7F
        mapper.write_prg(0xE000, 0x04);
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "write to $E000 should set reg1 (page 1 = 4)"
        );
        mapper.write_prg(0xFFFF, 0x06);
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "write to $FFFF should set reg1 (page 1 = 6)"
        );
    }

    // ── PRG page 1 banking ($C000–$FFFF = reg1 & 0x7F) ──────────────────────

    #[test]
    fn prg_page_1_always_follows_reg1_bits_6_0() {
        let mut mapper = make_mapper();
        // Any write to $A000–$FFFF sets reg1; page 1 = reg1 & 0x7F
        mapper.write_prg(0xA000, 0x03);
        assert_eq!(mapper.read_prg(0xC000), 3, "page 1 = 3 via $A000 write");

        mapper.write_prg(0xE000, 0x07);
        assert_eq!(mapper.read_prg(0xC000), 7, "page 1 = 7 via $E000 write");

        // Bit 7 of reg1 is masked out for page 1
        mapper.write_prg(0xA000, 0x88); // bit 7 set; reg1 & 0x7F = 0x08
        assert_eq!(
            mapper.read_prg(0xC000),
            8 % PRG_BANKS_16K as u8,
            "page 1 masks bit 7: 0x88 & 0x7F = 8"
        );
    }

    #[test]
    fn prg_page_1_covers_c000_to_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x05);
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 in page 1 bank 5");
        assert_eq!(mapper.read_prg(0xFFFF), 5, "$FFFF in page 1 bank 5");
    }

    // ── PRG page 0 banking ($8000–$BFFF) ────────────────────────────────────

    // Mode 0 or 1: page 0 = reg0 & 0x03

    #[test]
    fn prg_page_0_mode_0_uses_reg0_bits_1_0() {
        let mut mapper = make_mapper();
        // Initial mode = 0 → page 0 = reg0 & 0x03
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "mode 0: page 0 = reg0 & 0x03 = 3"
        );
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "mode 0: page 0 = reg0 & 0x03 = 2"
        );
        // Bits above 1 in reg0 don't affect page 0 in mode 0/1
        mapper.write_prg(0x8000, 0xFC); // reg0 & 0x03 = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "mode 0: page 0 = reg0 & 0x03 = 0 (upper bits masked)"
        );
    }

    #[test]
    fn prg_page_0_mode_1_uses_reg0_bits_1_0() {
        let mut mapper = make_mapper();
        // Write to $A000 (reg1, mode=1); then page 0 formula uses mode 1 (not & 0x02)
        mapper.write_prg(0xA000, 0x00); // mode = 1
        mapper.write_prg(0x8000, 0x03); // reg0 = 0x03
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "mode 1: page 0 = reg0 & 0x03 = 3"
        );
        mapper.write_prg(0x8000, 0x01); // reg0 = 0x01
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "mode 1: page 0 = reg0 & 0x03 = 1"
        );
    }

    #[test]
    fn prg_page_0_mode_2_combines_reg0_low_and_reg1_high() {
        let mut mapper = make_mapper();
        // Write to $C000 (reg1, mode=2); page 0 = (reg0 & 0x0F) | (reg1 & 0x70)
        mapper.write_prg(0x8000, 0x03); // reg0 = 0x03 → reg0 & 0x0F = 0x03
        mapper.write_prg(0xC000, 0x10); // reg1 = 0x10 → reg1 & 0x70 = 0x10; result = 0x03 | 0x10 = 0x13 = 19
        assert_eq!(
            mapper.read_prg(0x8000),
            19 % PRG_BANKS_16K as u8,
            "mode 2: page 0 = (reg0 & 0x0F) | (reg1 & 0x70)"
        );
    }

    #[test]
    fn prg_page_0_mode_3_combines_reg0_low_and_reg1_high() {
        let mut mapper = make_mapper();
        // Write to $E000 (reg1, mode=3); page 0 = (reg0 & 0x0F) | (reg1 & 0x70)
        mapper.write_prg(0x8000, 0x05); // reg0 = 0x05 → reg0 & 0x0F = 0x05
        mapper.write_prg(0xE000, 0x20); // reg1 = 0x20 → reg1 & 0x70 = 0x20; result = 0x05 | 0x20 = 0x25 = 37
        assert_eq!(
            mapper.read_prg(0x8000),
            37 % PRG_BANKS_16K as u8,
            "mode 3: page 0 = (reg0 & 0x0F) | (reg1 & 0x70)"
        );
    }

    #[test]
    fn prg_page_0_covers_8000_to_bfff() {
        let mut mapper = make_mapper();
        // mode 0, reg0 = 0x02 → page 0 = 2
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 in page 0 bank 2");
        assert_eq!(mapper.read_prg(0xBFFF), 2, "$BFFF in page 0 bank 2");
    }

    #[test]
    fn prg_pages_0_and_1_are_independent() {
        let mut mapper = make_mapper();
        // page 0 (mode 0): reg0 & 0x03 = 2
        mapper.write_prg(0x8000, 0x02); // reg0 = 2
        // page 1: reg1 & 0x7F = 5
        mapper.write_prg(0xA000, 0x05); // reg1 = 5, mode = 1
        assert_eq!(mapper.read_prg(0x8000), 2, "page 0 = 2");
        assert_eq!(mapper.read_prg(0xC000), 5, "page 1 = 5");
    }

    // ── CHR banking (always bank 0) ──────────────────────────────────────────

    #[test]
    fn chr_is_always_bank_0_regardless_of_prg_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xA000, 0xFF);
        mapper.write_prg(0xC000, 0xFF);
        mapper.write_prg(0xE000, 0xFF);
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR always bank 0 at $0000");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR always bank 0 at $1FFF");
    }

    #[test]
    fn chr_8kb_window_spans_0000_to_1fff() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    // ── Mirroring control (reg0 bit 4) ───────────────────────────────────────

    #[test]
    fn reg0_bit4_set_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // bit 4 = 1 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "reg0 bit4=1 → Vertical mirroring"
        );
    }

    #[test]
    fn reg0_bit4_clear_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // set Vertical
        mapper.write_prg(0x8000, 0x00); // clear bit 4 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "reg0 bit4=0 → Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_does_not_affect_prg_banking() {
        let mut mapper = make_mapper();
        // mode 0: page 0 = reg0 & 0x03; mirroring from bit 4
        mapper.write_prg(0x8000, 0x12); // reg0 = 0x12: bit4=1 (Vertical), bits1:0=2
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "page 0 should still be 2 with bit4 set"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring is vertical"
        );
    }

    // ── Reset behavior ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        // Set various non-default states
        mapper.write_prg(0x8000, 0x13); // reg0: vertical + page 0 bits
        mapper.write_prg(0xC000, 0x45); // reg1: mode=2
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "page 0 = 0 after reset");
        assert_eq!(mapper.read_prg(0xC000), 0, "page 1 = 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Horizontal mirroring after reset"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_state() {
        let mut mapper = make_mapper();
        // Set mode 2 state: reg0=0x12, reg1=0x05, mode=2 (write to $C000)
        mapper.write_prg(0x8000, 0x12); // reg0 = 0x12
        mapper.write_prg(0xC000, 0x05); // reg1 = 0x05, mode = 2
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // page 0 = (reg0 & 0x0F) | (reg1 & 0x70) = (0x12 & 0x0F) | (0x05 & 0x70) = 0x02 | 0x00 = 2
        assert_eq!(
            restored.read_prg(0x8000),
            2,
            "restored page 0 correct (mode 2)"
        );
        // page 1 = reg1 & 0x7F = 5
        assert_eq!(restored.read_prg(0xC000), 5, "restored page 1 = 5");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "restored mirroring = Vertical (reg0 bit4 = 1)"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x02);
        mapper.restore_registers(&[0x00, 0x00]); // only 2 bytes, needs 3
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "state unchanged after short restore data"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring required");
        assert!(
            !caps.has_chr_banking,
            "CHR is fixed to bank 0, no CHR banking"
        );
        assert_eq!(caps.prg_bank_size_kb, 16, "16 KB PRG banks");
        assert_eq!(caps.chr_bank_size_kb, 8, "8 KB CHR bank");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xE000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 274 must never assert IRQ");
    }
}
