//! Mapper 061 - NTDEC address latch multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_061>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 061 - NTDEC address latch multicart
///
/// Hardware: NTDEC 0324 (submapper 0) or BS-N032 (submapper 1)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_061>
/// - PRG-ROM: Up to 512 KiB (submapper 0/1) via 4-bit bank selector
/// - CHR: Up to 128 KiB (sub 0) or 256 KiB (sub 1)
/// - Mirroring: Programmable (H/V)
///
/// Register encoded in the 16-bit address on any write to $8000-$FFFF:
///
/// A~[1... CCCC McpN PPPP]
///   - bits[3:0] (PPPP) = PRG A18..A15 (base of 32KB block, 4 bits)
///   - bit[4]    (N)    = PRG mode: 0=NROM-256 (A14 from CPU), 1=NROM-128 (A14 from p)
///   - bit[5]    (p)    = PRG A14 when N=1
///   - bit[6]    (c)    = CHR A13 (submapper 1 only)
///   - bit[7]    (M)    = Mirroring: 0=Vertical, 1=Horizontal
///   - bits[11:8](CCCC) = CHR A16..A13 (submapper 0) or A17..A14 (submapper 1)
///
/// PRG 16KB bank:
///   NROM-128 (N=1): bank16 = (PPPP << 1) | p
///   NROM-256 (N=0): bank16 = (PPPP << 1) | (cpu_a14), where cpu_a14 = addr >= $C000
///
/// CHR 8KB bank:
///   Submapper 0: bank8 = CCCC         (4-bit → 16 banks of 8KB = 128KB)
///   Submapper 1: bank8 = (CCCC << 1) | c  (5-bit → 32 banks of 8KB = 256KB)
pub struct Mapper61 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    submapper: u8,
    /// Latched register bits from last write address
    prg_base: u8, // PPPP = bits[3:0]
    prg_mode: bool, // N = bit[4]; false=NROM-256, true=NROM-128
    prg_a14: u8,    // p = bit[5]; used when prg_mode=true
    chr_a13: u8,    // c = bit[6]; submapper 1 only
    chr_bank: u8,   // CCCC = bits[11:8]
    mirroring: NametableLayout,
}

impl Mapper61 {
    const MAPPER_NUMBER: u8 = 61;
    const PRG_16K_SIZE: usize = 0x4000; // 16 KiB
    const PRG_BANK_MASK: usize = Self::PRG_16K_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    #[allow(dead_code)]
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self::new_with_submapper(prg_rom, chr_rom, mirroring, 0)
    }

    pub fn new_with_submapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        _mirroring: NametableLayout,
        submapper: u8,
    ) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            submapper,
            prg_base: 0,
            prg_mode: false,
            prg_a14: 0,
            chr_a13: 0,
            chr_bank: 0,
            mirroring: NametableLayout::Vertical,
        }
    }

    fn num_prg_16k_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_16K_SIZE
    }

    fn num_chr_banks(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_SIZE
    }

    /// Decode the address into the mapper register fields.
    fn latch_address(&mut self, addr: u16) {
        self.prg_base = (addr & 0x000F) as u8;
        self.prg_mode = (addr & 0x0010) != 0;
        self.prg_a14 = ((addr >> 5) & 0x01) as u8;
        self.chr_a13 = ((addr >> 6) & 0x01) as u8;
        self.mirroring = if (addr & 0x0080) != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        };
        self.chr_bank = ((addr >> 8) & 0x0F) as u8;
    }

    fn effective_chr_bank(&self) -> usize {
        if self.submapper == 1 {
            // 5-bit CHR bank: CCCC (4 bits) shifted up, plus c (1 bit)
            ((self.chr_bank as usize) << 1) | (self.chr_a13 as usize)
        } else {
            self.chr_bank as usize
        }
    }

    fn prg_bank_for_addr(&self, addr: u16) -> usize {
        let count = self.num_prg_16k_banks();
        if count == 0 {
            return 0;
        }
        let base = (self.prg_base as usize) << 1; // PPPP × 2 gives 16KB index
        let bank = if self.prg_mode {
            // NROM-128: A14 from p
            base | (self.prg_a14 as usize)
        } else {
            // NROM-256: A14 from CPU (addr >= $C000 → A14=1)
            let cpu_a14 = if addr >= 0xC000 { 1 } else { 0 };
            base | cpu_a14
        };
        bank % count
    }
}

impl Mapper for Mapper61 {
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

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.latch_address(addr);
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let count = self.num_chr_banks();
        if count == 0 {
            return self.chr_memory.read(addr);
        }
        let bank = self.effective_chr_bank() % count;
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
            | ((self.prg_a14) << 1)
            | ((self.chr_a13) << 2)
            | ((matches!(self.mirroring, NametableLayout::Horizontal) as u8) << 3);
        vec![self.prg_base, flags, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_base = data[0];
            self.prg_mode = (data[1] & 0x01) != 0;
            self.prg_a14 = (data[1] >> 1) & 0x01;
            self.chr_a13 = (data[1] >> 2) & 0x01;
            self.mirroring = if (data[1] & 0x08) != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
            self.chr_bank = data[2];
        }
    }

    fn reset(&mut self) {
        self.prg_base = 0;
        self.prg_mode = false;
        self.prg_a14 = 0;
        self.chr_a13 = 0;
        self.chr_bank = 0;
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

    fn make_mapper() -> Mapper61 {
        let prg = banked_data(16 * 1024, 32);
        let chr = banked_data(8 * 1024, 16);
        Mapper61::new(prg, chr, NametableLayout::Vertical)
    }

    fn make_mapper_sub1() -> Mapper61 {
        let prg = banked_data(16 * 1024, 32);
        let chr = banked_data(8 * 1024, 32);
        Mapper61::new_with_submapper(prg, chr, NametableLayout::Vertical, 1)
    }

    #[test]
    fn mapper_61_is_registered() {
        let result = create_mapper(MapperContext::new(
            61,
            banked_data(16 * 1024, 32),
            banked_data(8 * 1024, 16),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 61 must be registered");
    }

    #[test]
    fn default_maps_prg_bank0_at_8000_and_1_at_c000() {
        let mapper = make_mapper();
        // Default: PPPP=0, N=0 (NROM-256): $8000 → bank 0, $C000 → bank 1
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn nrom_128_mirrors_same_bank() {
        let mut mapper = make_mapper();
        // N=1 (bit4), p=0: bank = (PPPP<<1)|0 = 0; address = 0x8010
        mapper.write_prg(0x8010, 0); // N=1, PPPP=0 → bank16=0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 0, "NROM-128 mirrors same bank");
    }

    #[test]
    fn prg_bank_selection_pppp() {
        let mut mapper = make_mapper();
        // PPPP=3, N=0: base=6; $8000→6, $C000→7
        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn chr_bank_sub0() {
        let mut mapper = make_mapper();
        // CCCC=5 (bits 11:8 = 0x500): address = 0x8500
        mapper.write_prg(0x8500, 0);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn chr_bank_sub1_uses_c_bit() {
        let mut mapper = make_mapper_sub1();
        // CCCC=2, c=1 (bit6): bank = (2<<1)|1 = 5; address = 0x8240
        mapper.write_prg(0x8240, 0); // bits[11:8]=2, bit6=1
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn mirroring_bit7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // bit7=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8080, 0); // bit7=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8393, 0); // some state
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(r.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }
}
