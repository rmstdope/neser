//! Mapper 338 – BMC-SA005-A (16-in-1 / 200-300-600-1000-in-1 multicart)
//!
//! Specifications:
//! - Primary reference: NesDev wiki clone (NES 2.0 Mapper 338)
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_338>
//! - UNIF board name: BMC-SA005-A
//!
//! ## Register Map ($8000–$FFFF, write)
//!
//! | Address       | Mask   | Bits         | Function                              |
//! |---------------|--------|--------------|---------------------------------------|
//! | $8000–$FFFF   | $8000  | `.... MBBB`  | PRG + CHR bank and mirroring select   |
//!
//! - **BBB** (bits [2:0]): Select 16 KiB PRG-ROM bank mapped to both $8000–$BFFF and
//!   $C000–$FFFF (both CPU windows mirror the same bank), and simultaneously select the
//!   8 KiB CHR-ROM bank at PPU $0000–$1FFF.
//! - **M** (bit 3): Nametable mirroring
//!   - 0 = Horizontal
//!   - 1 = Vertical
//!
//! ## Power-on / Reset
//!
//! Register initialised to 0: bank 0 for both PRG and CHR, horizontal mirroring.
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 338;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const BANK_MASK: u8 = 0x07;
const MIRROR_BIT: u8 = 0x08;

pub struct Mapper338 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper338 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self { base, reg: 0 };
        mapper.apply_state(0);
        mapper
    }

    fn apply_state(&mut self, reg: u8) {
        self.reg = reg;
        let bank = (reg & BANK_MASK) as i16;
        self.base.select_prg_page(0, bank);
        self.base.select_prg_page(1, bank);
        self.base.select_chr_page(0, bank);
        // bit 3: 0 = Horizontal (true), 1 = Vertical (false)
        self.base.set_mirroring_hv((reg & MIRROR_BIT) == 0);
    }
}

impl Mapper for Mapper338 {
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
        if addr >= 0x8000 {
            self.apply_state(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.apply_state(data[0]);
    }

    fn reset(&mut self) {
        self.apply_state(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS_16K: usize = 11; // 11 × 16 KB
    const CHR_BANKS_8K: usize = 9; // 9 × 8 KB

    fn make_mapper() -> Mapper338 {
        Mapper338::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_338_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 338 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_0_at_both_windows() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank should be 0 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should be horizontal at power-on (M=0)"
        );
    }

    // ── PRG banking: both windows use the same bank ───────────────────────────

    #[test]
    fn both_prg_windows_always_mirror_the_same_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // bank 3
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 should be bank 3");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should also be bank 3");
    }

    #[test]
    fn prg_bank_bits_2_0_select_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05); // bits[2:0] = 5
        assert_eq!(mapper.read_prg(0x8000), 5, "$8000 should be bank 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be bank 5");
    }

    #[test]
    fn prg_upper_bits_except_mirror_are_ignored() {
        // Bit 3 = mirror, bits [7:4] unused — only bits [2:0] select the bank
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xF4); // bits[2:0]=4, upper bits set but irrelevant
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "only bits [2:0] select PRG bank"
        );
        assert_eq!(mapper.read_prg(0xC000), 4, "$C000 mirrors same bank");
    }

    #[test]
    fn writes_below_8000_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05); // set a known state
        mapper.write_prg(0x7FFF, 0x02); // should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "write below $8000 must not change bank"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_follows_same_bits_as_prg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06); // bank 6
        assert_eq!(mapper.read_chr(0x0000), 6, "CHR should be bank 6");
    }

    #[test]
    fn chr_bank_0_selectable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8000, 0x00); // back to 0
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR should be bank 0");
    }

    #[test]
    fn chr_bank_updates_with_each_prg_write() {
        let mut mapper = make_mapper();
        for bank in 0u8..7 {
            mapper.write_prg(0x8000, bank);
            assert_eq!(
                mapper.read_chr(0x0000),
                bank,
                "CHR should be bank {bank} after write"
            );
        }
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn bit3_set_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x08); // bit 3 = 1 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit3=1 should select vertical mirroring"
        );
    }

    #[test]
    fn bit3_clear_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x08); // set vertical first
        mapper.write_prg(0x8000, 0x00); // clear bit 3
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit3=0 should select horizontal mirroring"
        );
    }

    #[test]
    fn mirror_bit_does_not_affect_prg_or_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0B); // bit3=1(V) + bits[2:0]=3
        assert_eq!(mapper.read_prg(0x8000), 3, "PRG bank should be 3 (not 11)");
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank should be 3");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_bank_and_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0B); // bank 3, vertical
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "restored PRG bank should be 3"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            3,
            "restored CHR bank should be 3"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Vertical,
            "restored mirroring should be vertical"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        mapper.restore_registers(&[]); // too short — must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "state must be unchanged after short restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0B); // bank 3, vertical
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be horizontal after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_dynamic_mirroring);
        assert!(caps.has_chr_banking);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
