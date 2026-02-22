//! Mapper 062 - Super 700-in-1 (address latch + data latch)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_062>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
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
///   → 7-bit PRG bank (128 × 16KB = 2MB)
///
///   CHR:
///   - CCCCC (bits 4:0 of address) = high 5 bits of CHR bank
///   - cc (bits 1:0 of data) = low 2 bits of CHR bank
///   → 7-bit CHR bank (128 × 8KB = 1MB)
///
///   O (bit 5 of address) = PRG mode:
///     0: 32KB at $8000-$FFFF (A14 from CPU)
///     1: 16KB mirrored at both $8000-$BFFF and $C000-$FFFF
///
///   M (bit 7 of address) = Mirroring: 0=Vertical, 1=Horizontal
pub struct Mapper62 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    /// Full 7-bit PRG bank register (selects 16KB page)
    prg_bank: u8,
    /// Full 7-bit CHR bank register (selects 8KB page)
    chr_bank: u8,
    /// PRG mode: false=32KB (NROM-256), true=16KB mirrored (NROM-128)
    prg_mode: bool,
    mirroring: NametableLayout,
}

impl Mapper62 {
    const MAPPER_NUMBER: u8 = 62;
    const PRG_16K_SIZE: usize = 0x4000; // 16 KiB
    const PRG_BANK_MASK: usize = Self::PRG_16K_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, _mirroring: NametableLayout) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            prg_bank: 0,
            chr_bank: 0,
            prg_mode: false,
            mirroring: NametableLayout::Vertical,
        }
    }

    fn num_prg_16k_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_16K_SIZE
    }

    fn num_chr_banks(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
    }

    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let count = self.num_prg_16k_banks();
        if count == 0 {
            return 0;
        }
        let bank = if self.prg_mode {
            // NROM-128: 16KB mirror
            (self.prg_bank as usize) % count
        } else {
            // NROM-256: 32KB, A14 from CPU
            let base = (self.prg_bank as usize) & !1; // even-align
            let half = if addr >= 0xC000 { 1 } else { 0 };
            (base | half) % count
        };
        bank
    }
}

impl Mapper for Mapper62 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let bank = self.prg_bank_for_addr(addr);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.prg_rom
                    .get(bank * Self::PRG_16K_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            // PRG bank: P (bit6 of addr) = high bit, pp_pppp (bits 13:8) = low 6 bits
            let prg_low = ((addr >> 8) & 0x3F) as u8; // bits 13:8
            let prg_high = ((addr >> 6) & 0x01) as u8; // bit 6
            self.prg_bank = (prg_high << 6) | prg_low;

            // CHR bank: CCCCC (bits 4:0 of addr) = high 5 bits, cc (bits 1:0 of data) = low 2 bits
            let chr_high = (addr & 0x001F) as u8; // bits 4:0
            let chr_low = value & 0x03; // data bits 1:0
            self.chr_bank = (chr_high << 2) | chr_low;

            self.prg_mode = (addr & 0x0020) != 0; // bit 5
            self.mirroring = if (addr & 0x0080) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let count = self.num_chr_banks();
        if count == 0 {
            return self.chr_memory.read(addr);
        }
        let bank = (self.chr_bank as usize) % count;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.chr_memory
            .read_at_index(bank * Self::CHR_BANK_SIZE + offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.prg_mode as u8)
            | ((matches!(self.mirroring, NametableLayout::Horizontal) as u8) << 1);
        vec![self.prg_bank, self.chr_bank, flags]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.prg_mode = (data[2] & 0x01) != 0;
            self.mirroring = if (data[2] & 0x02) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.prg_mode = false;
        self.mirroring = NametableLayout::Vertical;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
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
        Mapper62::new(prg, chr, NametableLayout::Vertical)
    }

    #[test]
    fn mapper_62_is_registered() {
        let result = create_mapper(MapperContext::new(
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
