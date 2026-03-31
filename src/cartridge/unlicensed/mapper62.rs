//! Mapper 062 - Super 700-in-1 (address latch + data latch)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_062>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 062 - Super 700-in-1
///
/// Hardware: Address + data latch
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_062>
/// - PRG-ROM: Up to 2 MiB (7-bit 16KB bank selector)
/// - CHR: Up to 1 MiB (7-bit 8KB bank selector)
/// - Mirroring: Programmable (H/V)
///
/// Register on any write to $8000-$FFFF:
///
/// Address: A~[..pp pppp MPOC CCCC]
/// Data:    D~[.... ..cc]
///
///   PRG:
///   - pp pppp (bits 13:8 of address) = low 6 bits of PRG bank
///   - P (bit 6 of address) = high bit of PRG bank
///   - 7-bit PRG bank (128 × 16KB = 2MB)
///
///   CHR:
///   - CCCCC (bits 4:0 of address) = high 5 bits of CHR bank
///   - cc (bits 1:0 of data) = low 2 bits of CHR bank
///   - 7-bit CHR bank (128 × 8KB = 1MB)
///
///   O (bit 5 of address) = PRG mode:
///     0: 32KB at $8000-$FFFF (A14 from CPU)
///     1: 16KB mirrored at both $8000-$BFFF and $C000-$FFFF
///
///   M (bit 7 of address) = Mirroring: 0=Vertical, 1=Horizontal
pub struct Mapper62 {
    base: BaseMapper,
    /// Full 7-bit PRG bank register (selects 16KB page)
    pub(crate) prg_bank: u8,
    /// Full 7-bit CHR bank register (selects 8KB page)
    pub(crate) chr_bank: u8,
    /// PRG mode: false=32KB (NROM-256), true=16KB mirrored (NROM-128)
    pub(crate) prg_mode: bool,
}

impl Mapper62 {
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
        // Default: NROM-256, bank 0 → slot 0=0, slot 1=1
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

impl Mapper for Mapper62 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            let prg_low = ((addr >> 8) & 0x3F) as u8;
            let prg_high = ((addr >> 6) & 0x01) as u8;
            self.prg_bank = (prg_high << 6) | prg_low;

            let chr_high = (addr & 0x001F) as u8;
            let chr_low = value & 0x03;
            self.chr_bank = (chr_high << 2) | chr_low;

            self.prg_mode = (addr & 0x0020) != 0;
            self.base.set_mirroring_hv((addr & 0x0080) != 0);
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
            self.prg_mode = (data[2] & 0x01) != 0;
            self.base.set_mirroring_hv((data[2] & 0x02) != 0);
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

    fn make_mapper() -> Mapper62 {
        let prg = banked_data(16 * 1024, 128);
        let chr = banked_data(8 * 1024, 128);
        Mapper62::new(MapperContext::new_for_test(
            62,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_62_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            62,
            banked_data(16 * 1024, 128),
            banked_data(8 * 1024, 128),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 62 must be registered");
    }

    #[test]
    fn default_prg_bank0() {
        let mapper = make_mapper();
        // Default: prg_bank=0, prg_mode=false (NROM-256)
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn prg_bank_selection_via_address_bits() {
        let mut mapper = make_mapper();
        // Set PRG bank = 0x42: high = bit6 = 1, low = bits13:8 = 2 → bank = 0x42 = 66
        // Address: P (bit6=1) → 0x0040, pp_pppp (bits13:8 = 0x02) → addr |= 0x0200
        // addr = 0x8000 | 0x0040 | 0x0200 = 0x8240
        mapper.write_prg(0x8240, 0);
        assert_eq!(mapper.prg_bank, 0x42);
        // NROM-256: even base=0x42 & ~1 = 0x42 (already even), $8000→0x42, $C000→0x43
        assert_eq!(mapper.read_prg(0x8000), 0x42);
        assert_eq!(mapper.read_prg(0xC000), 0x43);
    }

    #[test]
    fn prg_nrom128_mode() {
        let mut mapper = make_mapper();
        // O=1 (bit5=1), PRG bank=5 (bits13:8=5)
        // addr = 0x8000 | 0x0020 | 0x0500 = 0x8520
        mapper.write_prg(0x8520, 0);
        assert_eq!(mapper.prg_bank, 5);
        assert!(mapper.prg_mode, "O=1 should set NROM-128 mode");
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.read_prg(0xC000), 5, "NROM-128 mirrors same bank");
    }

    #[test]
    fn chr_bank_from_address_and_data() {
        let mut mapper = make_mapper();
        // CHR high (bits4:0 of addr) = 0x0A = 10, CHR low (data bits 1:0) = 3
        // chr_bank = (10 << 2) | 3 = 43
        mapper.write_prg(0x800A, 3); // addr bits4:0 = 0x0A; data = 3
        assert_eq!(mapper.chr_bank, 43);
        assert_eq!(mapper.read_chr(0x0000), 43);
    }

    #[test]
    fn mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // bit7=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8080, 0); // bit7=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8A53, 2); // some arbitrary state
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }
}
