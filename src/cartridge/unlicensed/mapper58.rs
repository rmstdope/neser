//! Mapper 058 - BMC multicart (address latch)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_058>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 058 - BMC multicart (21-in-1, 50-in-1, etc.)
///
/// Hardware: Address latch (mask $8000); bank select from address lines.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_058>
/// - PRG-ROM: Up to 512 KiB (8 × 16/32 KiB banks)
/// - CHR: Up to 64 KiB (8 × 8 KiB banks)
/// - Mirroring: Programmable (H/V)
///
/// The entire register is encoded in the address bus:
///
/// A~[1... .... MSCC CPPP]
///   - bits[2:0] (PPP) = PRG A16..A14 (select 16KB or 32KB page)
///   - bits[5:3] (CCC) = CHR A15..A13 (8KB CHR bank)
///   - bit 6 (S)       = PRG Mode (0=NROM-256, 1=NROM-128)
///   - bit 7 (M)       = Mirroring (0=Vertical, 1=Horizontal)
///
/// PRG banking:
///   NROM-128 (S=1): bank = PPP (16KB); $8000-$BFFF and $C000-$FFFF mirror same page
///   NROM-256 (S=0): 32KB at $8000-$FFFF; A14 from CPU (PPP selects upper bits A16..A15)
///
/// CHR: 8KB bank = CCC.
pub struct Mapper58 {
    base: BaseMapper,
    prg_bank: u8,   // bits [2:0] of address latch
    chr_bank: u8,   // bits [5:3] of address latch
    prg_mode: bool, // bit 6: 0=NROM-256 (32KB), 1=NROM-128 (16KB mirror)
}

impl Mapper58 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
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
        // Default: NROM-256 mode, bank 0 → slot 0=0, slot 1=1
        base.select_prg_page(1, 1);
        Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
            prg_mode: false,
        }
    }

    fn update_banks(&mut self) {
        self.base
            .apply_nrom_prg_banking(self.prg_bank, self.prg_mode);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper58 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        let _ = value; // data bus is not used; bank selection is from address
        if (0x8000..=0xFFFF).contains(&addr) {
            let a = (addr & 0x00FF) as u8;
            self.prg_bank = a & 0x07;
            self.chr_bank = (a >> 3) & 0x07;
            self.prg_mode = (a & 0x40) != 0;
            self.base.set_mirroring_hv((a & 0x80) != 0);
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.prg_mode as u8)
            | ((matches!(self.base.mirroring(), NametableLayout::Horizontal) as u8) << 1);
        vec![self.prg_bank, self.chr_bank, flags]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.prg_mode = (data[2] & 1) != 0;
            self.base.set_mirroring_hv((data[2] & 2) != 0);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.prg_mode = false;
        self.base.set_mirroring(NametableLayout::Vertical);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper58 {
        let prg = banked_data(16 * 1024, 8);
        let chr = banked_data(8 * 1024, 8);
        Mapper58::new(MapperContext::new_for_test(
            58,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_58_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            58,
            banked_data(16 * 1024, 8),
            banked_data(8 * 1024, 8),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 58 must be registered");
    }

    #[test]
    fn default_maps_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "Default NROM-256: C000 maps to bank 1"
        );
    }

    #[test]
    fn nrom_128_mode_mirrors_same_bank() {
        let mut mapper = make_mapper();
        // Address with S=1 (bit 6), PPP=3: addr low byte = 0b0100_0011 = 0x43
        mapper.write_prg(0x8043, 0);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 3, "NROM-128: mirror same bank");
    }

    #[test]
    fn nrom_256_mode_uses_consecutive_banks() {
        let mut mapper = make_mapper();
        // Address with S=0, PPP=4: even base=4, $8000=4, $C000=5
        mapper.write_prg(0x8004, 0); // PPP=4, S=0
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    #[test]
    fn chr_bank_selection() {
        let mut mapper = make_mapper();
        // CCC = 5: bits[5:3] of low addr byte = 0b0010_1000 = 0x28
        mapper.write_prg(0x8028, 0);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn mirroring_bit7_of_addr() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // M=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8080, 0); // M=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn registers_snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8057, 0); // PPP=7, S=1, M=0, CCC=2
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }
}
