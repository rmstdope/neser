//! Mapper 77 - IREM NINA-03 (Napoleon Senki)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing scenarios.

use crate::cartridge::BaseMapper;
use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;

/// Size of the switchable CHR-ROM page (4 KB).
const CHR_ROM_PAGE_SIZE: usize = 4 * 1024;

/// Size of the on-board CHR-RAM (2 KB).
const CHR_RAM_SIZE: usize = 2 * 1024;

/// Mapper 77 - IREM NINA-03 (Napoleon Senki)
///
/// Hardware: IREM NINA-03 / HOLYDIVER PCB (used exclusively by Napoleon Senki)
///
/// Specifications: <https://www.nesdev.org/wiki/INES_Mapper_077>
/// - PRG-ROM: 32 KB fixed at $8000–$FFFF (no banking)
/// - CHR-ROM: 16 KB total; 4 KB page selected by register bits [3:0] at $0000–$0FFF
/// - CHR-RAM: 2 KB on-board SRAM at $1000–$17FF (not bankable); also used as
///   nametable RAM (accessible to the PPU as nametable memory)
/// - Register: any write to $8000–$FFFF; bits [3:0] select the active 4 KB CHR-ROM page
/// - Mirroring: one-screen lower (fixed; both nametable addresses map to the same bank)
///
/// PPU CHR address space layout:
/// - `$0000–$0FFF`: 4 KB CHR-ROM page (bank-switched by register bits [3:0])
/// - `$1000–$17FF`: 2 KB CHR-RAM (fixed, not bankable)
/// - `$1800–$1FFF`: CHR-RAM mirror (same 2 KB; used by the PPU as nametable)
pub struct Mapper77 {
    base: BaseMapper,
    /// Currently selected 4 KB CHR-ROM bank (bits [3:0] of the write register).
    chr_bank: u8,
    /// 2 KB on-board CHR-RAM at $1000–$17FF.
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper77 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            chr_bank_size_kb: 4,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        // Mapper 77 has fixed one-screen lower nametable mirroring.
        base.set_mirroring(NametableLayout::SingleScreenLower);
        Self {
            base,
            chr_bank: 0,
            chr_ram: [0; CHR_RAM_SIZE],
        }
    }

    /// Read a byte from the 4 KB CHR-ROM page mapped to $0000–$0FFF.
    ///
    /// `offset` must be in 0x000–0xFFF.
    #[inline]
    fn read_chr_rom_page(&self, offset: usize) -> u8 {
        let chr_size = self.base.chr_size();
        if chr_size == 0 {
            return 0;
        }
        let bank_count = chr_size / CHR_ROM_PAGE_SIZE;
        let effective_bank = (self.chr_bank as usize) % bank_count.max(1);
        let index = effective_bank * CHR_ROM_PAGE_SIZE + offset;
        self.base.read_chr_at_index(index)
    }
}

impl Mapper for Mapper77 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Any write to $8000–$FFFF updates the CHR bank register (bits [3:0]).
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank = value & 0x0F;
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        match addr {
            // 4 KB CHR-ROM page (bank-switched).
            0x0000..=0x0FFF => self.read_chr_rom_page(addr as usize),
            // 2 KB CHR-RAM (fixed).
            0x1000..=0x17FF => self.chr_ram[(addr - 0x1000) as usize],
            // $1800–$1FFF mirrors the same 2 KB CHR-RAM (nametable use by hardware).
            0x1800..=0x1FFF => self.chr_ram[(addr - 0x1800) as usize],
            _ => 0,
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        match addr {
            // CHR-ROM is read-only.
            0x0000..=0x0FFF => {}
            // 2 KB CHR-RAM is writable.
            0x1000..=0x17FF => self.chr_ram[(addr - 0x1000) as usize] = value,
            // $1800–$1FFF mirrors the same 2 KB CHR-RAM.
            0x1800..=0x1FFF => self.chr_ram[(addr - 0x1800) as usize] = value,
            _ => {}
        }
    }

    fn mapper_number(&self) -> u8 {
        77
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.chr_bank = bank & 0x0F;
        }
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.to_vec()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let len = data.len().min(CHR_RAM_SIZE);
        self.chr_ram[..len].copy_from_slice(&data[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// Create a Mapper77 with 32 KB PRG-ROM and 16 KB CHR-ROM (4 banks of 4 KB).
    fn make_mapper() -> Mapper77 {
        let prg = vec![0u8; 32 * 1024];
        let chr = banked_data(4 * 1024, 4);
        Mapper77::new(MapperContext::new_for_test(
            77,
            prg,
            chr,
            NametableLayout::Horizontal, // overridden to SingleScreenLower by mapper
        ))
    }

    #[test]
    fn mapper_77_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            77,
            vec![0u8; 32 * 1024],
            banked_data(4 * 1024, 4),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 77 must be registered in the factory");
    }

    #[test]
    fn chr_rom_bank_0_selected_on_startup() {
        // `banked_data` fills each bank-N page with byte value N.
        let mapper = make_mapper();
        // Bank 0 → all bytes in that 4KB page equal 0.
        assert_eq!(mapper.read_chr_rom_page(0x000), 0);
        assert_eq!(mapper.read_chr_rom_page(0xFFF), 0);
    }

    #[test]
    fn write_to_8000_selects_chr_rom_bank() {
        let mut mapper = make_mapper();

        // Select bank 0: $0000–$0FFF should return bank-0 data (value 0).
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0, "bank 0 at $0000");

        // Select bank 1: $0000–$0FFF should return bank-1 data (value 1).
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 1, "bank 1 at $0000");

        // Select bank 2.
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_chr(0x0000), 2, "bank 2 at $0000");

        // Select bank 3.
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 3, "bank 3 at $0000");
    }

    #[test]
    fn write_anywhere_in_prg_range_updates_chr_bank() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_chr(0x0000), 2, "write to $FFFF should select bank 2");

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 1, "write to $C000 should select bank 1");
    }

    #[test]
    fn register_masks_lower_4_bits_only() {
        let mut mapper = make_mapper();

        // Upper nibble should be ignored; only bits [3:0] count.
        mapper.write_prg(0x8000, 0xF1); // bits [3:0] = 1
        assert_eq!(mapper.read_chr(0x0000), 1, "only bits [3:0] should select the bank");
    }

    #[test]
    fn bank_wraps_when_exceeding_available_banks() {
        let mut mapper = make_mapper(); // 4 banks

        // Bank 4 wraps to bank 0.
        mapper.write_prg(0x8000, 0x04);
        assert_eq!(mapper.read_chr(0x0000), 0, "bank 4 should wrap to bank 0");

        // Bank 5 wraps to bank 1.
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 1, "bank 5 should wrap to bank 1");
    }

    #[test]
    fn chr_ram_at_1000_is_readable_and_writable() {
        let mut mapper = make_mapper();

        mapper.write_chr(0x1000, 0xAB);
        assert_eq!(mapper.read_chr(0x1000), 0xAB, "$1000 CHR-RAM write should persist");

        mapper.write_chr(0x17FF, 0xCD);
        assert_eq!(mapper.read_chr(0x17FF), 0xCD, "$17FF CHR-RAM write should persist");
    }

    #[test]
    fn chr_ram_writes_do_not_affect_chr_rom_page() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x01);
        let bank1_val = mapper.read_chr(0x0000);

        mapper.write_chr(0x1000, 0xFF);

        assert_eq!(
            mapper.read_chr(0x0000),
            bank1_val,
            "CHR-RAM write must not alter CHR-ROM reads"
        );
    }

    #[test]
    fn chr_rom_page_is_read_only() {
        let mut mapper = make_mapper();

        let original = mapper.read_chr(0x0000);
        mapper.write_chr(0x0000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            original,
            "writes to CHR-ROM region must be ignored"
        );
    }

    #[test]
    fn mirroring_is_single_screen_lower() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "mapper 77 uses fixed one-screen lower mirroring"
        );
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03);

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            3,
            "restored mapper should use the same CHR bank"
        );
    }

    #[test]
    fn chr_ram_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x1000, 0xDE);
        mapper.write_chr(0x17FF, 0xAD);

        let snap = mapper.chr_ram_snapshot();

        let mut restored = make_mapper();
        restored.restore_chr_ram(&snap);

        assert_eq!(restored.read_chr(0x1000), 0xDE, "CHR-RAM byte at $1000 should be restored");
        assert_eq!(restored.read_chr(0x17FF), 0xAD, "CHR-RAM byte at $17FF should be restored");
    }
}
