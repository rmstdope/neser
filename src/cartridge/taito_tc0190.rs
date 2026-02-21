//! Mapper 033 – Taito TC0190
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::common::ChrMemory;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 033 – Taito TC0190
///
/// Hardware: Taito TC0190 / TC0350 (subset)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_033>
/// - PRG-ROM: Up to 512KB, two 8KB switchable banks + two fixed banks
/// - CHR: Two 2KB switchable banks + four 1KB switchable banks
/// - Mirroring: Programmable (H/V) via bit 6 of register $8000
///
/// PRG layout ($8000 CPU):
/// ```text
///   $8000   $A000   $C000   $E000
/// +-------+-------+-------+-------+
/// | $8000 | $8001 | { -2} | { -1} |
/// +-------+-------+-------+-------+
/// ```
///
/// CHR layout ($0000 PPU):
/// ```text
///   $0000   $0800   $1000   $1400   $1800   $1C00
/// +-------+-------+-------+-------+-------+-------+
/// | $8002 | $8003 | $A000 | $A001 | $A002 | $A003 |
/// +-------+-------+-------+-------+-------+-------+
/// ```
///
/// Registers (range $8000–$BFFF, mask $A003):
/// - `$8000` [.MPP PPPP]: M=Mirroring (0=Vert, 1=Horz), P=PRG bank 0 (8KB @ $8000)
/// - `$8001` [..PP PPPP]: PRG bank 1 (8KB @ $A000)
/// - `$8002` [CCCC CCCC]: CHR bank 0 (2KB @ $0000)
/// - `$8003` [CCCC CCCC]: CHR bank 1 (2KB @ $0800)
/// - `$A000` [CCCC CCCC]: CHR bank 2 (1KB @ $1000)
/// - `$A001` [CCCC CCCC]: CHR bank 3 (1KB @ $1400)
/// - `$A002` [CCCC CCCC]: CHR bank 4 (1KB @ $1800)
/// - `$A003` [CCCC CCCC]: CHR bank 5 (1KB @ $1C00)
pub struct TaitoTc0190Mapper {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    prg_ram: Vec<u8>,

    mirroring: NametableLayout,

    prg_bank: [u8; 2],
    chr_bank_2k: [u8; 2],
    chr_bank_1k: [u8; 4],
}

impl TaitoTc0190Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const CHR_BANK_2K_SIZE: usize = 0x0800; // 2KB
    const CHR_BANK_1K_SIZE: usize = 0x0400; // 1KB
    const PRG_RAM_SIZE: usize = 0x2000; // 8KB
    const REGISTER_MASK: u16 = 0xA003; // bits 13, 1, 0 select the active register

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new(chr_rom),
            prg_ram: vec![0; Self::PRG_RAM_SIZE],
            mirroring,
            prg_bank: [0; 2],
            chr_bank_2k: [0; 2],
            chr_bank_1k: [0; 4],
        }
    }

    fn prg_bank_count(&self) -> usize {
        self.prg_rom.len() / Self::PRG_BANK_SIZE
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        Self::bank_index(bank, self.prg_bank_count())
    }

    fn read_prg_rom_bank(&self, bank_index: usize, offset: usize) -> u8 {
        let addr = bank_index * Self::PRG_BANK_SIZE + offset;
        self.prg_rom.get(addr).copied().unwrap_or(0)
    }

    fn chr_bank_count_2k(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_2K_SIZE
    }

    fn chr_bank_count_1k(&self) -> usize {
        self.chr_memory.size() / Self::CHR_BANK_1K_SIZE
    }

    fn chr_bank_index_2k(&self, bank: u8) -> usize {
        Self::bank_index(bank, self.chr_bank_count_2k())
    }

    fn chr_bank_index_1k(&self, bank: u8) -> usize {
        Self::bank_index(bank, self.chr_bank_count_1k())
    }

    /// Returns `bank % count`, or 0 when no banks are present.
    fn bank_index(bank: u8, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        (bank as usize) % count
    }

    fn map_chr_to_byte_index(&self, addr: u16) -> usize {
        let addr = (addr & 0x1FFF) as usize;
        let (bank_index, bank_size) = match addr {
            0x0000..=0x07FF => (
                self.chr_bank_index_2k(self.chr_bank_2k[0]),
                Self::CHR_BANK_2K_SIZE,
            ),
            0x0800..=0x0FFF => (
                self.chr_bank_index_2k(self.chr_bank_2k[1]),
                Self::CHR_BANK_2K_SIZE,
            ),
            0x1000..=0x13FF => (
                self.chr_bank_index_1k(self.chr_bank_1k[0]),
                Self::CHR_BANK_1K_SIZE,
            ),
            0x1400..=0x17FF => (
                self.chr_bank_index_1k(self.chr_bank_1k[1]),
                Self::CHR_BANK_1K_SIZE,
            ),
            0x1800..=0x1BFF => (
                self.chr_bank_index_1k(self.chr_bank_1k[2]),
                Self::CHR_BANK_1K_SIZE,
            ),
            0x1C00..=0x1FFF => (
                self.chr_bank_index_1k(self.chr_bank_1k[3]),
                Self::CHR_BANK_1K_SIZE,
            ),
            _ => return 0,
        };
        bank_index * bank_size + (addr & (bank_size - 1))
    }
}

impl Mapper for TaitoTc0190Mapper {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                self.prg_ram.get(offset).copied().unwrap_or(0)
            }
            0x8000..=0xFFFF => {
                let count = self.prg_bank_count();
                if count == 0 {
                    return 0;
                }
                let bank_offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);
                let bank_index = match addr {
                    0x8000..=0x9FFF => self.prg_bank_index(self.prg_bank[0]),
                    0xA000..=0xBFFF => self.prg_bank_index(self.prg_bank[1]),
                    0xC000..=0xDFFF => count.saturating_sub(2),
                    0xE000..=0xFFFF => count.saturating_sub(1),
                    _ => 0,
                };
                self.read_prg_rom_bank(bank_index, bank_offset)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr - 0x6000) as usize;
                if let Some(byte) = self.prg_ram.get_mut(offset) {
                    *byte = value;
                }
            }
            0x8000..=0xBFFF => match addr & Self::REGISTER_MASK {
                0x8000 => {
                    self.prg_bank[0] = value & 0x3F;
                    self.mirroring = if (value & 0x40) != 0 {
                        NametableLayout::Horizontal
                    } else {
                        NametableLayout::Vertical
                    };
                }
                0x8001 => self.prg_bank[1] = value & 0x3F,
                0x8002 => self.chr_bank_2k[0] = value,
                0x8003 => self.chr_bank_2k[1] = value,
                0xA000 => self.chr_bank_1k[0] = value,
                0xA001 => self.chr_bank_1k[1] = value,
                0xA002 => self.chr_bank_1k[2] = value,
                0xA003 => self.chr_bank_1k[3] = value,
                _ => {}
            },
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let byte_index = self.map_chr_to_byte_index(addr);
        self.chr_memory.read_at_index(byte_index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let byte_index = self.map_chr_to_byte_index(addr);
        self.chr_memory.write_at_index(byte_index, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        33
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.prg_ram.len());
        self.prg_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        crate::console::initialize_ram(&mut self.prg_ram, mode);
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // [0]: mirroring (0=Vert, 1=Horz)
        // [1..=2]: prg_bank[0..1]
        // [3..=4]: chr_bank_2k[0..1]
        // [5..=8]: chr_bank_1k[0..3]
        let mut snap = Vec::with_capacity(9);
        snap.push(if self.mirroring == NametableLayout::Horizontal {
            1
        } else {
            0
        });
        snap.extend_from_slice(&self.prg_bank);
        snap.extend_from_slice(&self.chr_bank_2k);
        snap.extend_from_slice(&self.chr_bank_1k);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.mirroring = if data[0] != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            };
            self.prg_bank.copy_from_slice(&data[1..3]);
            self.chr_bank_2k.copy_from_slice(&data[3..5]);
            self.chr_bank_1k.copy_from_slice(&data[5..9]);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::taito_tc0190::TaitoTc0190Mapper;
    use crate::cartridge::test_helpers::banked_data;

    fn create_tc0190(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(33, prg_rom, chr_rom, mirroring))
    }

    // -------------------------------------------------------------------
    // PRG banking
    // -------------------------------------------------------------------

    #[test]
    fn prg_bank0_is_switchable_at_8000() {
        // 6 banks × 8KB so bank indices don't alias accidentally
        let prg_rom = banked_data(8 * 1024, 6);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8000, 2); // select bank 2 at $8000
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0x9FFF), 2);

        mapper.write_prg(0x8000, 3); // switch to bank 3
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn prg_bank1_is_switchable_at_a000() {
        let prg_rom = banked_data(8 * 1024, 6);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8001, 1); // select bank 1 at $A000
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xBFFF), 1);

        mapper.write_prg(0x8001, 4);
        assert_eq!(mapper.read_prg(0xA000), 4);
    }

    #[test]
    fn prg_c000_is_fixed_to_second_last_bank() {
        let prg_rom = banked_data(8 * 1024, 6); // banks 0–5; second-last = 4
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        // Changing bank 0/1 must not affect $C000
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.read_prg(0xC000), 4); // second-last bank = 4
        assert_eq!(mapper.read_prg(0xDFFF), 4);
    }

    #[test]
    fn prg_e000_is_fixed_to_last_bank() {
        let prg_rom = banked_data(8 * 1024, 6); // last bank = 5
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.read_prg(0xE000), 5); // last bank = 5
        assert_eq!(mapper.read_prg(0xFFFF), 5);
    }

    #[test]
    fn prg_bank0_bit6_is_not_used_for_banking() {
        // Only bits 5:0 select the PRG bank; bit 6 is the mirroring bit
        let prg_rom = banked_data(8 * 1024, 3); // 3 banks
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        // Write 0x41 → bit6 set (mirroring=H), bank bits = 0x01 → bank 1
        mapper.write_prg(0x8000, 0x41);
        assert_eq!(mapper.read_prg(0x8000), 1);
    }

    // -------------------------------------------------------------------
    // CHR banking – 2KB windows
    // -------------------------------------------------------------------

    #[test]
    fn chr_2k_bank0_switchable_at_0000() {
        // Use a non-power-of-two count (3 banks × 2KB = 6KB) to catch wrap bugs
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 3);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8002, 1); // CHR bank 0 → 2KB bank 1
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x07FF), 1);
    }

    #[test]
    fn chr_2k_bank1_switchable_at_0800() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 3);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8003, 2); // CHR bank 1 → 2KB bank 2
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0FFF), 2);
    }

    #[test]
    fn chr_2k_bank_value_is_2kb_multiple_not_1kb() {
        // Unlike MMC3, the register value is already a 2KB index (LSB not dropped)
        // Use 5 banks × 2KB so index 3 doesn't alias to 1
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(2 * 1024, 5);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        // Writing 3 selects 2KB bank #3, not bank #2 (which MMC3 would select)
        mapper.write_prg(0x8002, 3);
        assert_eq!(mapper.read_chr(0x0000), 3);
    }

    // -------------------------------------------------------------------
    // CHR banking – 1KB windows
    // -------------------------------------------------------------------

    #[test]
    fn chr_1k_banks_switchable_at_1000_to_1c00() {
        // 48 banks × 1KB – non-power-of-two avoids modulo false passes
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0xA000, 10); // $1000
        mapper.write_prg(0xA001, 20); // $1400
        mapper.write_prg(0xA002, 30); // $1800
        mapper.write_prg(0xA003, 40); // $1C00

        assert_eq!(mapper.read_chr(0x1000), 10);
        assert_eq!(mapper.read_chr(0x1400), 20);
        assert_eq!(mapper.read_chr(0x1800), 30);
        assert_eq!(mapper.read_chr(0x1C00), 40);
    }

    #[test]
    fn chr_windows_do_not_alias_each_other() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 48);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8002, 0); // 2KB bank 0 → $0000
        mapper.write_prg(0x8003, 1); // 2KB bank 1 → $0800
        mapper.write_prg(0xA000, 2); // 1KB bank 2 → $1000
        mapper.write_prg(0xA001, 3); // 1KB bank 3 → $1400
        mapper.write_prg(0xA002, 4); // 1KB bank 4 → $1800
        mapper.write_prg(0xA003, 5); // 1KB bank 5 → $1C00

        // Each CHR window must return its own bank marker
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0800), 2); // 2KB bank 1 → 1KB banks 2+3
        assert_eq!(mapper.read_chr(0x1000), 2);
        assert_eq!(mapper.read_chr(0x1400), 3);
        assert_eq!(mapper.read_chr(0x1800), 4);
        assert_eq!(mapper.read_chr(0x1C00), 5);
    }

    // -------------------------------------------------------------------
    // Mirroring
    // -------------------------------------------------------------------

    #[test]
    fn mirroring_bit6_zero_selects_vertical() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8000, 0x00); // bit 6 clear → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_bit6_one_selects_horizontal() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8000, 0x40); // bit 6 set → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // -------------------------------------------------------------------
    // PRG-RAM ($6000–$7FFF)
    // -------------------------------------------------------------------

    #[test]
    fn prg_ram_read_write() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = TaitoTc0190Mapper::new(prg_rom, chr_rom, NametableLayout::Vertical);

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(mapper.read_prg(0x7FFF), 0xCD);
    }

    #[test]
    fn prg_ram_snapshot_round_trips() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = vec![0; 8 * 1024];

        let mut mapper = TaitoTc0190Mapper::new(prg_rom, chr_rom, NametableLayout::Vertical);
        mapper.write_prg(0x6000, 0xBB);

        let snap = mapper.wram_snapshot();
        assert_eq!(snap[0], 0xBB);

        mapper.write_prg(0x6000, 0x00);
        mapper.load_wram_snapshot(&snap);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);
    }

    // -------------------------------------------------------------------
    // Register snapshot / restore
    // -------------------------------------------------------------------

    #[test]
    fn registers_snapshot_restores_all_banks_and_mirroring() {
        let prg_rom = banked_data(8 * 1024, 6);
        let chr_rom = banked_data(1024, 48);

        let mut mapper = create_tc0190(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8000, 0x43); // mirroring=H, prg_bank[0]=3
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 2);
        mapper.write_prg(0x8003, 4);
        mapper.write_prg(0xA000, 10);
        mapper.write_prg(0xA001, 20);
        mapper.write_prg(0xA002, 30);
        mapper.write_prg(0xA003, 40);

        let snap = mapper.registers_snapshot();

        let mut restored =
            create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical).expect("mapper 33");
        restored.restore_registers(&snap);

        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xA000), 5);
        assert_eq!(restored.read_chr(0x0000), 4); // chr_bank_2k[0]=2 → 2KB bank 2 → 1KB idx 4
        assert_eq!(restored.read_chr(0x0800), 8); // 2KB bank 4 → start = 1KB idx 8
        assert_eq!(restored.read_chr(0x1000), 10);
        assert_eq!(restored.read_chr(0x1400), 20);
        assert_eq!(restored.read_chr(0x1800), 30);
        assert_eq!(restored.read_chr(0x1C00), 40);
    }

    // -------------------------------------------------------------------
    // Address mirror writes (mask $A003)
    // -------------------------------------------------------------------

    #[test]
    fn mirrored_write_addresses_reach_correct_registers() {
        // $8004 & $A003 == $8000; $8005 & $A003 == $8001; etc.
        let prg_rom = banked_data(8 * 1024, 6);
        let chr_rom = banked_data(1024, 48);

        let mut mapper = create_tc0190(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("mapper 33 must be supported");

        mapper.write_prg(0x8004, 2); // mirrors $8000 → prg_bank[0]=2
        assert_eq!(mapper.read_prg(0x8000), 2);

        mapper.write_prg(0x8005, 4); // mirrors $8001 → prg_bank[1]=4
        assert_eq!(mapper.read_prg(0xA000), 4);

        mapper.write_prg(0x9002, 0); // 0x9002 & 0xA003 = 0x8002 → chr_bank_2k[0]
        mapper.write_prg(0x9003, 1);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0800), 2); // chr_bank_2k[1]=1 → 1KB idx 2
    }
}
