//! Mapper 074 - MMC3 variant with CHR-RAM at banks 8–9 (Chinese pirate boards)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_074>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 074 - MMC3 with CHR-RAM at banks 8–9
///
/// Hardware: Chinese pirate board (MMC3 clone variant)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_074>
/// - Inner mapper: Full MMC3 (all registers and behavior)
/// - PRG-ROM: Up to 512 KiB (standard MMC3)
/// - CHR: CHR-ROM for bank selections 0–7 and 10+; CHR-RAM for bank selections 8–9
/// - PRG-RAM: 8 KiB at $6000-$7FFF (standard MMC3)
/// - Mirroring: Programmable via MMC3 ($A000)
/// - IRQ: Standard MMC3 scanline counter (A12 rising-edge)
///
/// CHR bank selection routing:
/// - MMC3 register value 8 or 9 → 1 KiB CHR-RAM bank (2 KiB total)
/// - All other MMC3 register values → CHR-ROM
pub struct Mapper74 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; 2 * 1024], // 2 KiB: bank 8 = [0..1023], bank 9 = [1024..2047]
}

impl Mapper74 {
    const MAPPER_NUMBER: u8 = 74;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const CHR_RAM_FIRST_BANK: usize = 8;
    const CHR_RAM_LAST_BANK: usize = 9;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_ram: [0; 2 * 1024],
        }
    }

    fn is_chr_ram_bank(bank: usize) -> bool {
        bank == Self::CHR_RAM_FIRST_BANK || bank == Self::CHR_RAM_LAST_BANK
    }

    fn chr_ram_index(bank: usize, offset: usize) -> usize {
        (bank - Self::CHR_RAM_FIRST_BANK) * Self::CHR_1K_BANK_SIZE + offset
    }
}

impl Mapper for Mapper74 {
    fn base(&self) -> &BaseMapper {
        &self.inner.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        // Use the raw (unwrapped) register value to determine CHR-RAM vs CHR-ROM:
        // banks 8–9 always map to CHR-RAM regardless of the physical CHR-ROM size.
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)]
        } else {
            let wrapped_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.read_chr_1k_at(wrapped_bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        // Use the raw (unwrapped) register value to determine CHR-RAM vs CHR-ROM.
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)] = value;
        } else {
            // Delegate to inner MMC3 so that CHR-RAM-only cartridges (no CHR-ROM)
            // still allow writes to non-8/9 banks via the BaseMapper CHR-RAM.
            let wrapped_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.write_chr_1k_at(wrapped_bank, offset, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.extend_from_slice(&self.chr_ram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const CHR_RAM_SIZE: usize = 2 * 1024;
        if data.len() >= CHR_RAM_SIZE {
            let (mmc3_data, chr_ram_data) = data.split_at(data.len() - CHR_RAM_SIZE);
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(chr_ram_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two PRG bank count to prevent modulo-wrapping false-passes
    const PRG_BANKS: usize = 6; // 6 × 8KB = 48KB
    // CHR-ROM: 12 × 1KB banks (non-power-of-two, avoids wrapping false-passes)
    // Banks 0..=7 and 10..=11 = CHR-ROM; banks 8..=9 = CHR-RAM
    const CHR_ROM_1K_BANKS: usize = 12;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_ROM_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            74,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 74 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_74_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            74,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 74 must be registered in the factory"
        );
    }

    // --- PRG banking (full MMC3 delegation) ---

    #[test]
    fn prg_fixed_last_bank_at_e000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Fixed-last PRG bank must be the last bank"
        );
    }

    #[test]
    fn prg_bank_select_via_r6() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select = R6, mode 0
        mapper.write_prg(0x8001, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "R6=2 must map $8000 to PRG bank 2"
        );
    }

    // --- CHR-ROM reads (banks outside 8–9) ---

    #[test]
    fn chr_rom_read_for_bank_0() {
        let mut mapper = make_mapper();
        // CHR mode 0 (default): R0 selects 2KB at PPU $0000
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 4); // select 1KB bank 4 (R0 → even bank, so bank 4/5 pair)
        // banked_data fills bank N with byte N%256; bank 4 → byte 4
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR bank 4 (CHR-ROM) must read from CHR-ROM"
        );
    }

    #[test]
    fn chr_rom_read_for_bank_7() {
        let mut mapper = make_mapper();
        // CHR mode 0: R3 → PPU $1400 (1KB at $1400)
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 7); // bank 7 → CHR-ROM
        assert_eq!(
            mapper.read_chr(0x1400),
            7,
            "CHR bank 7 (CHR-ROM) must read from CHR-ROM"
        );
    }

    // --- CHR-RAM reads/writes (banks 8 and 9) ---

    #[test]
    fn chr_ram_bank_8_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // CHR mode 0: R2 → PPU $1000 (1KB starting at $1000)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8); // select bank 8 → CHR-RAM
        mapper.write_chr(0x1000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xAB,
            "CHR bank 8 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_9_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // CHR mode 0: R3 → PPU $1400 (1KB at $1400)
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9); // select bank 9 → CHR-RAM
        mapper.write_chr(0x1400, 0xCD);
        assert_eq!(
            mapper.read_chr(0x1400),
            0xCD,
            "CHR bank 9 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_banks_8_and_9_are_independent() {
        let mut mapper = make_mapper();
        // R2 → bank 8 at PPU $1000; R3 → bank 9 at PPU $1400
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9);

        mapper.write_chr(0x1000, 0x11); // write to bank 8
        mapper.write_chr(0x1400, 0x22); // write to bank 9

        assert_eq!(mapper.read_chr(0x1000), 0x11, "Bank 8 CHR-RAM must be 0x11");
        assert_eq!(mapper.read_chr(0x1400), 0x22, "Bank 9 CHR-RAM must be 0x22");
    }

    #[test]
    fn chr_rom_bank_is_not_writable() {
        let mut mapper = make_mapper();
        // R2 → PPU $1000
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // bank 5 = CHR-ROM
        let original = mapper.read_chr(0x1000); // should be 5 (marker byte)
        mapper.write_chr(0x1000, 0xFF); // write to CHR-ROM → should be ignored
        assert_eq!(
            mapper.read_chr(0x1000),
            original,
            "CHR-ROM bank writes must be silently ignored"
        );
    }

    // --- CHR boundary: banks 8/9 vs neighbours ---

    #[test]
    fn chr_bank_7_not_redirected_to_chr_ram() {
        let mut mapper = make_mapper();
        // R2 → bank 7 at PPU $1000 (bank 7 < 8 → CHR-ROM)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7);
        let rom_val = mapper.read_chr(0x1000); // should be 7 (CHR-ROM marker)

        // Now switch to bank 8 and write
        mapper.write_prg(0x8001, 8);
        mapper.write_chr(0x1000, 0xFF);

        // Switch back to bank 7 — should still read ROM value
        mapper.write_prg(0x8001, 7);
        assert_eq!(
            mapper.read_chr(0x1000),
            rom_val,
            "Bank 7 must still read CHR-ROM after bank-8 CHR-RAM write"
        );
    }

    #[test]
    fn chr_bank_10_not_redirected_to_chr_ram() {
        let mut mapper = make_mapper();
        // R2 → bank 10 at PPU $1000 (bank 10 > 9 → CHR-ROM)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 10);
        let rom_val = mapper.read_chr(0x1000); // should be 10%256=10 (CHR-ROM marker)

        // Write to bank 9 and verify bank 10 is unaffected
        mapper.write_prg(0x8001, 9);
        mapper.write_chr(0x1000, 0xFF);

        mapper.write_prg(0x8001, 10);
        assert_eq!(
            mapper.read_chr(0x1000),
            rom_val,
            "Bank 10 must still read CHR-ROM after bank-9 CHR-RAM write"
        );
    }

    // --- Mirroring delegation ---

    #[test]
    fn mmc3_mirroring_works_through_delegation() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Initial mirroring must match construction argument"
        );
        mapper.write_prg(0xA000, 0x01); // horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "MMC3 mirroring change must propagate through mapper 74"
        );
    }

    // --- IRQ delegation ---

    #[test]
    fn mmc3_irq_works_through_delegation() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 74");
    }

    // --- Save state ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Set R2 → bank 8 (CHR-RAM), write data
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_chr(0x1000, 0x77);
        // Set R6=2 (PRG bank)
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 2);

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // CHR-RAM content must survive round-trip
        mapper2.write_prg(0x8000, 0b0000_0010); // R2
        mapper2.write_prg(0x8001, 8);
        assert_eq!(
            mapper2.read_chr(0x1000),
            0x77,
            "CHR-RAM must survive snapshot round-trip"
        );
        // PRG bank must survive round-trip
        mapper2.write_prg(0x8000, 0b0000_0110); // R6
        assert_eq!(
            mapper2.read_prg(0x8000),
            2,
            "PRG bank must survive snapshot round-trip"
        );
    }
}
