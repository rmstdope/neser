//! Mapper 046 - Rumble Station (Color Dreams multicart)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_046>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 046 - Rumble Station (Color Dreams multicart)
///
/// Hardware: NES-on-a-Chip + Color Dreams inner mapper
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_046>
/// - PRG-ROM: Up to 1 MiB (32 KiB fixed page at $8000-$FFFF)
/// - CHR: Up to 1 MiB (8 KiB fixed bank at $0000-$1FFF)
/// - Mirroring: Fixed from header
/// - PRG-RAM: None ($6000-$7FFF is outer bank register)
///
/// Register layout:
/// - $6000-$7FFF: [CCCC PPPP] — outer register; bits[7:4]=CHR high, bits[3:0]=PRG high
/// - $8000-$FFFF: [.CCC ...P] — inner register; bits[6:4]=CHR low, bit[0]=PRG low
///
/// Final banks:
/// - CHR 8K bank = (outer[7:4] << 3) | inner[6:4]  (7-bit)
/// - PRG 32K page = (outer[3:0] << 1) | inner[0]   (5-bit)
///
/// Known games: Rumble Station 15-in-1
pub struct Mapper46 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    outer: u8,
    inner: u8,
}

impl Mapper46 {
    const MAPPER_NUMBER: u8 = 46;
    const PRG_PAGE_SIZE: usize = 0x8000; // 32 KiB
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            outer: 0,
            inner: 0,
        }
    }

    fn prg_page_count(&self) -> usize {
        let pages = self.prg_rom.len() / Self::PRG_PAGE_SIZE;
        if pages == 0 { 1 } else { pages }
    }

    fn chr_bank_count(&self) -> usize {
        let banks = self.chr_memory.size() / Self::CHR_BANK_SIZE;
        if banks == 0 { 1 } else { banks }
    }

    fn prg_page(&self) -> usize {
        let page = ((self.outer & 0x0F) as usize) << 1 | ((self.inner & 0x01) as usize);
        page % self.prg_page_count()
    }

    fn chr_bank(&self) -> usize {
        let bank = ((self.outer >> 4) as usize) << 3 | (((self.inner >> 4) & 0x07) as usize);
        bank % self.chr_bank_count()
    }
}

impl Mapper for Mapper46 {
    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let offset = (addr as usize) & (Self::PRG_PAGE_SIZE - 1);
        let mapped = self.prg_page() * Self::PRG_PAGE_SIZE + offset;
        self.prg_rom.get(mapped).copied().unwrap_or(0)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.outer = value,
            0x8000..=0xFFFF => self.inner = value,
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        let mapped = self.chr_bank() * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.read_at_index(mapped)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        let mapped = self.chr_bank() * Self::CHR_BANK_SIZE + offset;
        self.chr_memory.write_at_index(mapped, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0 // $6000-$7FFF is the outer bank register, not RAM
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
        vec![self.outer, self.inner]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.outer = data[0];
            self.inner = data[1];
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper46;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to prevent false-pass wrapping.
    const PRG_PAGES: usize = 5; // 5 × 32 KiB = 160 KiB PRG
    const CHR_BANKS: usize = 5; // 5 × 8 KiB = 40 KiB CHR

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(32 * 1024, PRG_PAGES);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(46, prg, chr, NametableLayout::Vertical))
            .expect("Mapper 46 should be implemented")
    }

    fn make_mapper_direct() -> Mapper46 {
        let prg = banked_data(32 * 1024, PRG_PAGES);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper46::new(prg, chr, NametableLayout::Vertical)
    }

    // --- Factory ---

    #[test]
    fn mapper_46_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            46,
            banked_data(32 * 1024, PRG_PAGES),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 46 must be registered in the factory"
        );
    }

    // --- PRG banking ---

    #[test]
    fn prg_defaults_to_page_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG must default to page 0");
    }

    #[test]
    fn prg_inner_bit_selects_odd_page() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // inner bit[0] = 1 → page 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "inner bit[0]=1 must select PRG page 1"
        );
    }

    #[test]
    fn prg_outer_nibble_shifts_page_by_2() {
        let mut mapper = make_mapper();
        // outer[3:0]=1, inner[0]=0 → page = (1<<1)|0 = 2
        mapper.write_prg(0x6000, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "outer[3:0]=1 must select PRG page 2"
        );
    }

    #[test]
    fn prg_outer_and_inner_combined() {
        let mut mapper = make_mapper();
        // outer[3:0]=1, inner[0]=1 → page = (1<<1)|1 = 3
        mapper.write_prg(0x6000, 0x01);
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "outer[3:0]=1, inner[0]=1 must select PRG page 3"
        );
    }

    #[test]
    fn prg_page_wraps_when_out_of_range() {
        let mut mapper = make_mapper();
        // outer[3:0]=2, inner[0]=1 → page = 5 → wraps to 0 with 5 pages
        mapper.write_prg(0x6000, 0x02);
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG page must wrap");
    }

    #[test]
    fn prg_offset_within_page_is_correct() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // page 2
        // Last byte of page 2 should also be 2
        assert_eq!(
            mapper.read_prg(0xFFFF),
            2,
            "last byte of page must match bank"
        );
    }

    // --- CHR banking ---

    #[test]
    fn chr_defaults_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must default to bank 0");
    }

    #[test]
    fn chr_inner_high_nibble_selects_bank() {
        let mut mapper = make_mapper();
        // inner bits[6:4] = 1 → CHR bank 1 (when outer[7:4]=0)
        mapper.write_prg(0x8000, 0x10); // inner = 0x10, bits[6:4] = 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "inner[6:4]=1 must select CHR bank 1"
        );
    }

    #[test]
    fn chr_outer_high_nibble_shifts_bank_by_3() {
        let mut mapper = make_mapper();
        // outer[7:4]=1, inner[6:4]=0 → CHR bank = (1<<3)|0 = 8 → wraps to 3 with 5 banks
        mapper.write_prg(0x6000, 0x10); // outer = 0x10, bits[7:4] = 1
        assert_eq!(
            mapper.read_chr(0x0000),
            (8 % CHR_BANKS) as u8,
            "outer[7:4]=1 must shift CHR bank by 8"
        );
    }

    #[test]
    fn chr_outer_and_inner_combined() {
        let mut mapper = make_mapper();
        // outer[7:4]=1, inner[6:4]=1 → CHR bank = 8|1 = 9 → 9%5 = 4
        mapper.write_prg(0x6000, 0x10);
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_chr(0x0000), (9 % CHR_BANKS) as u8);
    }

    #[test]
    fn chr_ram_writable_when_no_chr_rom() {
        let prg = banked_data(32 * 1024, PRG_PAGES);
        let mut mapper = Mapper46::new(prg, vec![], NametableLayout::Vertical);
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(mapper.read_chr(0x0100), 0xAB);
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_from_header() {
        let prg = banked_data(32 * 1024, PRG_PAGES);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mapper = Mapper46::new(prg, chr, NametableLayout::Horizontal);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x6000, 0x12); // outer
        mapper.write_prg(0x8000, 0x34); // inner

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }
}
