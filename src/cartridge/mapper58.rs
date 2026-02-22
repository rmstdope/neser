//! Mapper 058 - BMC multicart (address latch)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_058>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
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
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    prg_bank: u8,   // bits [2:0] of address latch
    chr_bank: u8,   // bits [5:3] of address latch
    prg_mode: bool, // bit 6: 0=NROM-256 (32KB), 1=NROM-128 (16KB mirror)
    mirroring: NametableLayout,
}

impl Mapper58 {
    const MAPPER_NUMBER: u8 = 58;
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

    /// Return the 16KB PRG bank index for the given CPU address.
    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let count = self.num_prg_16k_banks();
        if count == 0 {
            return 0;
        }
        let bank = if self.prg_mode {
            // NROM-128: mirror same 16KB at both $8000 and $C000
            (self.prg_bank as usize) % count
        } else {
            // NROM-256: two consecutive 16KB banks; A14 from CPU
            let base = (self.prg_bank as usize) & !1; // even-align
            let half = if addr >= 0xC000 { 1 } else { 0 };
            (base | half) % count
        };
        bank
    }
}

impl Mapper for Mapper58 {
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
        let _ = value; // data bus is not used; bank selection is from address
        if (0x8000..=0xFFFF).contains(&addr) {
            let a = (addr & 0x00FF) as u8; // low 8 bits of address carry the register
            self.prg_bank = a & 0x07;
            self.chr_bank = (a >> 3) & 0x07;
            self.prg_mode = (a & 0x40) != 0;
            self.mirroring = if (a & 0x80) != 0 {
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
        self.chr_memory.read_at_index(bank * Self::CHR_BANK_SIZE + offset)
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
            self.prg_mode = (data[2] & 1) != 0;
            self.mirroring = if (data[2] & 2) != 0 {
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

    fn make_mapper() -> Mapper58 {
        let prg = banked_data(16 * 1024, 8);
        let chr = banked_data(8 * 1024, 8);
        Mapper58::new(prg, chr, NametableLayout::Vertical)
    }

    #[test]
    fn mapper_58_is_registered() {
        let result = create_mapper(MapperContext::new(
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
        assert_eq!(mapper.read_prg(0xC000), 1, "Default NROM-256: C000 maps to bank 1");
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
