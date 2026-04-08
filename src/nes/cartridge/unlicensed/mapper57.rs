//! Mapper 057 - BMC GK-192 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_057>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 057 - BMC GK-192 multicart
///
/// Hardware: GK-192 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_057>
/// - PRG-ROM: Up to 128 KiB (8 x 16 KiB banks)
/// - CHR: Up to 128 KiB (16 x 8 KiB banks)
/// - Mirroring: Programmable (H/V)
///
/// Two registers, selected by address bit 11:
/// - $8000 (bit 11 = 0): [CH.. ..AA]
///   - C = CHR Mode (0=CNROM/8KB from $8000, 1=NROM/8KB from $8800)
///   - H = CHR A16
///   - A = CHR A13-A14 in CNROM mode
/// - $8800 (bit 11 = 1): [PPPO MBbb]
///   - P = PRG bank (3 bits, bits 7:5)
///   - O = PRG Mode (0=16KB mirrored, 1=32KB)
///   - M = Mirroring (0=Vert, 1=Horz)
///   - B = CHR A15
///   - b = CHR A13-A14 in NROM mode (bits 1:0)
///
/// CHR bank (8KB):
///   C=0: bank = (H<<3) | (B<<2) | AA
///   C=1: bank = (H<<3) | (B<<2) | bb
///
/// PRG bank (16KB or 32KB):
///   O=0: 16KB bank PPP at both $8000-$BFFF and $C000-$FFFF
///   O=1: 32KB at $8000-$FFFF = banks (PPP & 6) and (PPP | 1)
pub struct Mapper57 {
    base: BaseMapper,
    // $8000 register: [CH.. ..AA]
    reg0: u8,
    // $8800 register: [PPPO MBbb]
    reg1: u8,
}

impl Mapper57 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        // Default: NROM-128, bank 0 → both slots same
        base.select_prg_page(1, 0);
        let mut s = Self {
            base,
            reg0: 0,
            reg1: 0,
        };
        s.update_banks();
        s
    }

    fn prg_16k_bank(&self) -> usize {
        ((self.reg1 >> 5) & 0x07) as usize
    }

    fn prg_mode_32k(&self) -> bool {
        (self.reg1 & 0x10) != 0
    }

    fn chr_bank(&self) -> usize {
        let h = ((self.reg0 >> 6) & 0x01) as usize;
        let b = ((self.reg1 >> 2) & 0x01) as usize;
        let chr_mode = (self.reg0 & 0x80) != 0;
        let low = if chr_mode {
            // NROM mode: use bb from $8800
            (self.reg1 & 0x03) as usize
        } else {
            // CNROM mode: use AA from $8000
            (self.reg0 & 0x03) as usize
        };
        (h << 3) | (b << 2) | low
    }

    fn update_banks(&mut self) {
        let prg_bank = self.prg_16k_bank();
        if self.prg_mode_32k() {
            let base = prg_bank & !1;
            self.base.select_prg_page(0, base as i16);
            self.base.select_prg_page(1, (base | 1) as i16);
        } else {
            self.base.select_prg_page(0, prg_bank as i16);
            self.base.select_prg_page(1, prg_bank as i16);
        }
        self.base.select_chr_page(0, self.chr_bank() as i16);

        self.base.set_mirroring_hv((self.reg1 & 0x08) != 0);
    }
}

impl Mapper for Mapper57 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if let 0x8000..=0xFFFF = addr {
            if (addr & 0x0800) != 0 {
                self.reg1 = value;
            } else {
                self.reg0 = value;
            }
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg0, self.reg1]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.reg0 = data[0];
            self.reg1 = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg0 = 0;
        self.reg1 = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper57 {
        let prg = banked_data(16 * 1024, 8);
        let chr = banked_data(8 * 1024, 16);
        Mapper57::new(MapperContext::new_for_test(
            57,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_57_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            57,
            banked_data(16 * 1024, 8),
            banked_data(8 * 1024, 16),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 57 must be registered");
    }

    #[test]
    fn default_prg_reads_bank_0_at_8000_and_c000() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 0, "16KB mode mirrors same bank");
    }

    #[test]
    fn prg_bank_select_via_reg1() {
        let mut mapper = make_mapper();
        // Set PPP = 3 (bits 7:5 of reg1 = 0110_0000 = 0x60)
        mapper.write_prg(0x8800, 0x60); // PPP=3, O=0 (16KB mode)
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3, "16KB mode: same bank at C000");
    }

    #[test]
    fn prg_32kb_mode() {
        let mut mapper = make_mapper();
        // PPP=2 (0b010), O=1 → 32KB: base = 2 (even), $8000=bank2, $C000=bank3
        mapper.write_prg(0x8800, 0b0100_0001 << 1 | 0x10); // PPP=4, O=1
        // PPP=4 → base = 4 (even), $8000=4, $C000=5
        mapper.write_prg(0x8800, 0b1000_0000 | 0x10); // reg1: PPP=4, O=1 → 0x90
        // 0x90 = 1001_0000: PPP = bits[7:5] = 100 = 4, O = bit4 = 1
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    #[test]
    fn chr_cnrom_mode_uses_aa_from_reg0() {
        let mut mapper = make_mapper();
        // C=0 (CNROM), H=0, A=2 → chr bank = (0<<3)|(0<<2)|2 = 2
        mapper.write_prg(0x8000, 0b0000_0010); // reg0: C=0, A=2
        assert_eq!(mapper.read_chr(0x0000), 2);
    }

    #[test]
    fn chr_nrom_mode_uses_bb_from_reg1() {
        let mut mapper = make_mapper();
        // C=1 (NROM), H=0; bb=3 (reg1 bits 1:0)
        mapper.write_prg(0x8000, 0b1000_0000); // reg0: C=1
        mapper.write_prg(0x8800, 0b0000_0011); // reg1: bb=3
        assert_eq!(mapper.read_chr(0x0000), 3);
    }

    #[test]
    fn mirroring_bit3_of_reg1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8800, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8800, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0x8800, 0xFF);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn registers_snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x42);
        mapper.write_prg(0x8800, 0x65);
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }
}
