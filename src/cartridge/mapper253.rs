//! Mapper 253 - VRC4e clone with CHR-RAM at banks 4–5 (Waixing)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_253>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::vrc2_vrc4::Vrc2Vrc4Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 253 – VRC4e clone (Mapper 23 submapper 2) with CHR-RAM at banks 4–5.
///
/// Hardware: Waixing board, used for *Dragon Ball Z: Kyōshū! Saiya-jin* (Chinese localization)
///
/// Specifications:
/// - Inner mapper: VRC4e (CPU A2→chip A0, CPU A3→chip A1; same wiring as Mapper 23 submapper 2)
/// - PRG-ROM: Up to 512 KiB (VRC4e banking)
/// - CHR: CHR-ROM for bank selections 0–3 and 6+; CHR-RAM for bank selections 4–5
/// - PRG-RAM: 8 KiB at $6000–$7FFF
/// - Mirroring: Programmable via VRC4e ($9000)
/// - IRQ: VRC4e cycle-counting IRQ
///
/// CHR bank selection routing:
/// - VRC4e register value 4 or 5 → 1 KiB CHR-RAM bank (2 KiB total)
/// - All other VRC4e register values → CHR-ROM
pub struct Mapper253 {
    pub(crate) inner: Vrc2Vrc4Mapper,
    chr_ram: [u8; 2 * 1024],
}

impl Mapper253 {
    const MAPPER_NUMBER: u8 = 253;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const CHR_RAM_FIRST_BANK: u16 = 4;
    const CHR_RAM_LAST_BANK: u16 = 5;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        // Route as VRC4e (mapper 23, submapper 2) for correct address decoding.
        // Mapper 253 uses the same VRC4e pin wiring (CPU A2→chip A0, CPU A3→chip A1).
        let vrc4e_ctx = super::mapper::MapperContext {
            mapper: 23,
            submapper: 2,
            ..ctx
        };
        Self {
            inner: Vrc2Vrc4Mapper::new(vrc4e_ctx),
            chr_ram: [0; 2 * 1024],
        }
    }

    fn is_chr_ram_bank(raw_bank: u16) -> bool {
        raw_bank == Self::CHR_RAM_FIRST_BANK || raw_bank == Self::CHR_RAM_LAST_BANK
    }

    fn chr_ram_index(raw_bank: u16, offset: usize) -> usize {
        (raw_bank - Self::CHR_RAM_FIRST_BANK) as usize * Self::CHR_1K_BANK_SIZE + offset
    }
}

impl Mapper for Mapper253 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
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
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)]
        } else {
            self.inner.base().read_chr(ppu_addr)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)] = value;
        }
        // Writes to CHR-ROM slots are silently ignored.
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
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
            let (vrc4_data, chr_ram_data) = data.split_at(data.len() - CHR_RAM_SIZE);
            self.inner.restore_registers(vrc4_data);
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
    const PRG_BANKS: usize = 6; // 6 × 8KB = 48 KB
    // Non-power-of-two CHR-ROM bank count (banks 0–3 and 6–7 = CHR-ROM; 4–5 = CHR-RAM)
    const CHR_ROM_1K_BANKS: usize = 6; // 6 × 1KB (banks 0–3 are CHR-ROM, 4–5 are CHR-RAM)

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_ROM_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            253,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 253 should be registered")
    }

    // --- Factory ---

    #[test]
    fn mapper_253_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            253,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 253 must be registered in the factory"
        );
    }

    // --- PRG banking (VRC4e delegation) ---

    #[test]
    fn prg_fixed_last_bank_at_e000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Fixed last PRG bank must be at $E000"
        );
    }

    #[test]
    fn prg_bank_select_via_8000() {
        let mut mapper = make_mapper();
        // VRC4e: register $8000 = PRG bank 0 select; uses A2→chip A0, A3→chip A1
        mapper.write_prg(0x8000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Writing 2 to $8000 must map $8000 window to PRG bank 2"
        );
    }

    // --- CHR-ROM reads (banks outside 4–5) ---
    //
    // VRC4e address decoding: CPU A2→chip A0, CPU A3→chip A1.
    // For the CHR bank registers ($Bxxx–$Exxx), the normalised chip register positions are:
    //   pos 0 (low nibble of even bank):  CPU addr $xX00  (A2=0, A3=0)
    //   pos 1 (high nibble of even bank): CPU addr $xX04  (A2=1, A3=0)
    //   pos 2 (low nibble of odd bank):   CPU addr $xX08  (A3=1, A2=0)
    //   pos 3 (high nibble of odd bank):  CPU addr $xX0C  (A2=1, A3=1)
    // Bank values 0–5 fit in 4 bits so only the low-nibble write is required.

    #[test]
    fn chr_rom_read_for_bank_0() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 (PPU $0000) to bank 0 via $B000 low nibble
        mapper.write_prg(0xB000, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 bank 0 (CHR-ROM) must read from CHR-ROM"
        );
    }

    #[test]
    fn chr_rom_read_for_bank_3() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 (PPU $0000) to bank 3 (last CHR-ROM bank)
        mapper.write_prg(0xB000, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR slot with bank 3 (CHR-ROM) must read from CHR-ROM"
        );
    }

    // --- CHR-RAM reads/writes (banks 4 and 5) ---

    #[test]
    fn chr_ram_bank_4_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 (PPU $0000) to bank 4 (CHR-RAM) via $B000 low nibble
        mapper.write_prg(0xB000, 4);
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR bank 4 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_5_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 (PPU $0000) to bank 5 (CHR-RAM) via $B000 low nibble
        mapper.write_prg(0xB000, 5);
        mapper.write_chr(0x0000, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xCD,
            "CHR bank 5 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_banks_4_and_5_are_independent() {
        let mut mapper = make_mapper();
        // CHR slot 0 (PPU $0000) → bank 4: low nibble at $B000
        mapper.write_prg(0xB000, 4);
        // CHR slot 1 (PPU $0400) → bank 5: low nibble at $B008 (chip pos 2, A3=1)
        mapper.write_prg(0xB008, 5);

        mapper.write_chr(0x0000, 0x11); // write to bank 4
        mapper.write_chr(0x0400, 0x22); // write to bank 5

        assert_eq!(mapper.read_chr(0x0000), 0x11, "Bank 4 CHR-RAM must be 0x11");
        assert_eq!(mapper.read_chr(0x0400), 0x22, "Bank 5 CHR-RAM must be 0x22");
    }

    #[test]
    fn chr_rom_bank_3_not_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 3);
        let original = mapper.read_chr(0x0000);
        mapper.write_chr(0x0000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            original,
            "Writes to CHR-ROM bank must be silently ignored"
        );
    }

    // --- CHR boundary: bank 3 vs 4 ---

    #[test]
    fn chr_bank_3_not_redirected_to_chr_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 3);
        let rom_val = mapper.read_chr(0x0000);

        // Switch to bank 4 (CHR-RAM) and write
        mapper.write_prg(0xB000, 4);
        mapper.write_chr(0x0000, 0xFF);

        // Switch back to bank 3 — should still read CHR-ROM value
        mapper.write_prg(0xB000, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            rom_val,
            "Bank 3 must still read CHR-ROM after bank-4 CHR-RAM write"
        );
    }

    // --- Mirroring (VRC4e 4-mode) ---

    #[test]
    fn mirroring_defaults_to_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn vrc4e_mirroring_register_changes_mirroring() {
        let mut mapper = make_mapper();
        // VRC4e $9000 mirroring: bit0=0 → vertical, bit0=1 → horizontal; VRC4e supports
        // 2-bit mirroring (Vrc4eOnly). Write 0x01 for horizontal.
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring register 0x01 must select horizontal"
        );
    }

    // --- IRQ via VRC4e ---

    #[test]
    fn irq_pending_false_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    // --- Save state ---

    #[test]
    fn registers_snapshot_round_trips_chr_ram() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 → bank 4 (CHR-RAM) via $B000 low nibble, write marker
        mapper.write_prg(0xB000, 4);
        mapper.write_chr(0x0000, 0x77);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        mapper2.write_prg(0xB000, 4);
        assert_eq!(
            mapper2.read_chr(0x0000),
            0x77,
            "CHR-RAM content must survive snapshot round-trip"
        );
    }
}
