//! Mapper 252 – Waixing VRC4e clone with CHR-RAM at banks 6–7
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_252>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Waixing/Waixing252.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 252 is used for Waixing's Chinese localization of 三国志: 中原の覇者
//! (*Sangokushi: Chūgen no Hasha*).  It uses a clone of the Konami VRC4 with:
//! - CPU A2 → chip A0, CPU A3 → chip A1 (VRC4e pinout, same as Mapper 23 submapper 2).
//! - 8 KiB of PRG-RAM at CPU `$6000–$7FFF`.
//! - VRC4e cycle-counting IRQ.
//! - CHR bank selections 6 and 7 → 1 KiB CHR-RAM banks (all others → CHR-ROM).
//!
//! Mapper 253 is almost identical, but uses CHR-RAM at banks 4–5 instead of 6–7.
//!
//! ## CHR Bank Routing
//!
//! | VRC4e bank value | Backing                  |
//! |-----------------|--------------------------|
//! | 0–5             | CHR-ROM                  |
//! | 6               | CHR-RAM (1 KiB, bank 0)  |
//! | 7               | CHR-RAM (1 KiB, bank 1)  |
//! | 8+              | CHR-ROM                  |

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::vrc2_vrc4::Vrc2Vrc4Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 252 – Waixing VRC4e clone with CHR-RAM at banks 6–7.
///
/// See the module-level documentation for hardware details.
pub struct Mapper252 {
    pub(crate) inner: Vrc2Vrc4Mapper,
    chr_ram: [u8; 2 * 1024],
}

impl Mapper252 {
    const MAPPER_NUMBER: u16 = 252;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const CHR_RAM_FIRST_BANK: u16 = 6;
    const CHR_RAM_LAST_BANK: u16 = 7;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        // Route as VRC4e (mapper 23, submapper 2) for correct address decoding.
        let vrc4e_ctx = crate::nes::cartridge::mapper::MapperContext {
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

impl Mapper for Mapper252 {
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

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
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
        Self::MAPPER_NUMBER
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
        } else {
            self.inner.restore_registers(data);
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
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 6; // 6 × 8KB
    // CHR-ROM banks 0–5 (banks 6–7 are CHR-RAM)
    const CHR_ROM_1K_BANKS: usize = 6;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_ROM_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            252,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 252 should be registered")
    }

    #[test]
    fn mapper_252_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            252,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 252 must be registered in the factory"
        );
    }

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
        mapper.write_prg(0x8000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Writing 2 to $8000 must map $8000 window to PRG bank 2"
        );
    }

    #[test]
    fn chr_rom_read_for_bank_0() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 (PPU $0000) to bank 0 via $B000 low nibble
        mapper.write_prg(0xB000, 0);
        assert_eq!(mapper.read_chr(0x0000), 0, "Bank 0 must read from CHR-ROM");
    }

    #[test]
    fn chr_rom_read_for_bank_5() {
        let mut mapper = make_mapper();
        // Bank 5 is last CHR-ROM bank
        mapper.write_prg(0xB000, 5);
        assert_eq!(mapper.read_chr(0x0000), 5, "Bank 5 must read from CHR-ROM");
    }

    #[test]
    fn chr_ram_bank_6_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 → bank 6 (first CHR-RAM bank) via $B000 low nibble
        mapper.write_prg(0xB000, 6);
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR bank 6 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_7_is_writable_and_readable() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 → bank 7 (second CHR-RAM bank) via $B000 low nibble
        mapper.write_prg(0xB000, 7);
        mapper.write_chr(0x0000, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xCD,
            "CHR bank 7 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_banks_6_and_7_are_independent() {
        let mut mapper = make_mapper();
        // Slot 0 (PPU $0000) → bank 6; slot 1 (PPU $0400) → bank 7
        mapper.write_prg(0xB000, 6);
        mapper.write_prg(0xB008, 7); // chip pos 2, A3=1
        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x0400, 0x22);
        assert_eq!(mapper.read_chr(0x0000), 0x11, "Bank 6 CHR-RAM must be 0x11");
        assert_eq!(mapper.read_chr(0x0400), 0x22, "Bank 7 CHR-RAM must be 0x22");
    }

    #[test]
    fn chr_rom_bank_5_not_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB000, 5);
        let original = mapper.read_chr(0x0000);
        mapper.write_chr(0x0000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            original,
            "Writes to CHR-ROM bank must be silently ignored"
        );
    }

    #[test]
    fn chr_ram_differs_from_chr_rom_boundary_at_bank6() {
        let mut mapper = make_mapper();
        // Bank 5 (CHR-ROM): write should be ignored
        mapper.write_prg(0xB000, 5);
        mapper.write_chr(0x0000, 0xFF);
        let rom_val = mapper.read_chr(0x0000);

        // Bank 6 (CHR-RAM): should be writable
        mapper.write_prg(0xB000, 6);
        mapper.write_chr(0x0000, 0xEE);
        assert_eq!(mapper.read_chr(0x0000), 0xEE);

        // Back to bank 5: still original CHR-ROM value
        mapper.write_prg(0xB000, 5);
        assert_eq!(mapper.read_chr(0x0000), rom_val);
    }

    #[test]
    fn mirroring_defaults_to_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn irq_pending_false_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn registers_snapshot_round_trips_chr_ram() {
        let mut mapper = make_mapper();
        // Set CHR slot 0 → bank 6 (CHR-RAM), write marker
        mapper.write_prg(0xB000, 6);
        mapper.write_chr(0x0000, 0x77);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        mapper2.write_prg(0xB000, 6);
        assert_eq!(
            mapper2.read_chr(0x0000),
            0x77,
            "CHR-RAM content must survive snapshot round-trip"
        );
    }
}
