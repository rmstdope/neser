//! Mapper 012 - SL-5020B (MMC3 + CHR A18 extension)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_012>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 012 - SL-5020B (MMC3 with CHR A18 outer bank extension)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_012>
/// - Inner mapper: Full MMC3 (all registers and behavior)
/// - PRG-ROM: Up to 512 KiB (standard MMC3, no outer extension)
/// - CHR: Up to 512 KiB (256 KiB MMC3 window × 2 via A18 outer bit)
/// - PRG-RAM: 8 KiB at $6000-$7FFF (standard MMC3)
/// - Mirroring: Programmable via MMC3 ($A000)
///
/// Outer register (CPU $4132, detected by addr & 0xE100 == 0x4100):
/// - Bit 0: CHR A18 for PPU $0000-$0FFF (low CHR half)
/// - Bit 4: CHR A18 for PPU $1000-$1FFF (high CHR half)
/// - Read returns bit 0 only
/// - Reset value: 0
///
/// CHR bank resolution:
/// - `final_bank = (outer_bit << 8) | mmc3_1k_bank`
/// - This extends the MMC3's 256 KiB CHR window to 512 KiB.
pub struct Mapper12 {
    pub(crate) inner: MMC3Mapper,
    chr_ext_lo: bool, // bit 0: CHR A18 for PPU $0000-$0FFF
    chr_ext_hi: bool, // bit 4: CHR A18 for PPU $1000-$1FFF
}

impl Mapper12 {
    const MAPPER_NUMBER: u8 = 12;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_ext_lo: false,
            chr_ext_hi: false,
        }
    }

    fn is_outer_reg(addr: u16) -> bool {
        addr & 0xE100 == 0x4100
    }
}

impl Mapper for Mapper12 {
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
        if Self::is_outer_reg(addr) {
            return self.chr_ext_lo as u8;
        }
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_outer_reg(addr) {
            return self.chr_ext_lo as u8;
        }
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_outer_reg(addr) {
            self.chr_ext_lo = value & 0x01 != 0;
            self.chr_ext_hi = value & 0x10 != 0;
        } else {
            self.inner.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let a12 = (ppu_addr >> 12) & 1 != 0;
        let mmc3_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
        let ext_bit = if a12 {
            self.chr_ext_hi
        } else {
            self.chr_ext_lo
        };
        let final_bank = ((ext_bit as usize) << 8) | mmc3_bank;
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        self.inner.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let a12 = (ppu_addr >> 12) & 1 != 0;
        let mmc3_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
        let ext_bit = if a12 {
            self.chr_ext_hi
        } else {
            self.chr_ext_lo
        };
        let final_bank = ((ext_bit as usize) << 8) | mmc3_bank;
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        self.inner.write_chr_1k_at(final_bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn wram_size(&self) -> usize {
        8 * 1024
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        let outer = (self.chr_ext_lo as u8) | ((self.chr_ext_hi as u8) << 4);
        snap.push(outer);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer, mmc3_data)) = data.split_last() {
            self.chr_ext_lo = outer & 0x01 != 0;
            self.chr_ext_hi = outer & 0x10 != 0;
            self.inner.restore_registers(mmc3_data);
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
    use crate::nes::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    // 32 PRG 8KB banks = 256 KiB PRG
    const PRG_BANKS: usize = 32;
    // 512 CHR 1KB banks = 512 KiB CHR (required for mapper 12's A18 extension)
    const CHR_1K_BANKS: usize = 512;

    /// Creates a mapper with plain lower-byte CHR markers (bank N → byte N%256).
    /// Use for PRG tests and basic CHR banking tests within the lower 256 KiB.
    fn make_mapper() -> Box<dyn Mapper> {
        make_mapper_with_chr(banked_data(1024, CHR_1K_BANKS))
    }

    /// Creates a mapper with upper-byte CHR markers (banks 0–255 → byte 0,
    /// banks 256–511 → byte 1).  Use for outer-register extension tests.
    fn make_mapper_ext() -> Box<dyn Mapper> {
        make_mapper_with_chr(banked_data_with_upper_marker(1024, CHR_1K_BANKS))
    }

    fn make_mapper_with_chr(chr: Vec<u8>) -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        create_mapper(MapperContext::new_for_test(
            12,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 12 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_12_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            12,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 12 must be registered in the factory"
        );
    }

    // --- PRG banking (full MMC3 delegation) ---

    #[test]
    fn fixed_e000_bank_stable_after_r6_switch_16_banks() {
        // Reproduce the ROM test: 16 PRG banks (128KB), not 32
        let prg = banked_data(8 * 1024, 16);
        let chr = banked_data(1024, CHR_1K_BANKS);
        let mut mapper = create_mapper(MapperContext::new_for_test(
            12,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .unwrap();

        // Fixed-last bank ($E000) = bank 15 (last of 16)
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Fixed-last PRG bank must be 15 for 16 banks"
        );

        let before = mapper.read_prg(0xFFFC);

        // Switch R6 to bank 3
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select = R6
        mapper.write_prg(0x8001, 3);

        let after = mapper.read_prg(0xFFFC);
        assert_eq!(
            before, after,
            "Fixed $E000 bank must not change after R6 switch"
        );
    }

    #[test]
    fn all_mmc3_prg_banking_works() {
        let mut mapper = make_mapper();
        // Fixed-last bank ($E000) = bank 31 (last of 32)
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Fixed-last PRG bank must be 31"
        );
        // Select R6=3 → $8000 reads bank 3
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select = R6, mode 0
        mapper.write_prg(0x8001, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "R6=3 must map $8000 to PRG bank 3"
        );
    }

    // --- CHR banking within lower 256 KiB (outer reg = 0) ---

    #[test]
    fn all_mmc3_chr_banking_works_in_lower_256kb() {
        let mut mapper = make_mapper(); // banked_data: bank N → byte N%256
        // CHR mode 0 (default): R2 → PPU $1000
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "R2=5 must map PPU $1000 to CHR bank 5 (lower 256 KiB)"
        );
        // Changing R2 must produce a different bank
        mapper.write_prg(0x8001, 10);
        assert_eq!(
            mapper.read_chr(0x1000),
            10,
            "R2=10 must map PPU $1000 to CHR bank 10"
        );
    }

    // --- Outer register bit 0: extends CHR low half ($0000-$0FFF) to upper 256 KiB ---

    #[test]
    fn outer_reg_bit0_extends_chr_low_half_to_upper_256kb() {
        // Use upper-marker CHR: lower 256 KiB → marker 0; upper 256 KiB → marker 1
        let mut mapper = make_mapper_ext();
        // CHR mode 0: R0 is 2 KB at PPU $0000 (1 KB banks R0/R0+1 at $0000/$0400).
        // Set R0=4; PPU $0000 uses 1 KB bank 4 (A12=0 → chr_ext_lo).
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 4);

        // Without outer extension: bank 4 is in lower 256 KiB → marker 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Low half without extension: bank 4 must have upper-marker 0"
        );

        // Set outer bit0=1 (addr & 0xE100 == 0x4100 → e.g. $4132)
        mapper.write_prg(0x4132, 0x01);

        // With bit0=1: final bank = (1<<8)|4 = 260 → upper 256 KiB → marker 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Low half with bit0=1: final bank 260 must have upper-marker 1"
        );
    }

    // --- Outer register bit 4: extends CHR high half ($1000-$1FFF) to upper 256 KiB ---

    #[test]
    fn outer_reg_bit4_extends_chr_high_half_to_upper_256kb() {
        let mut mapper = make_mapper_ext();
        // CHR mode 0: R2=5 → PPU $1000 = 1 KB bank 5 (A12=1 → chr_ext_hi)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);

        // Without ext: bank 5 → lower 256 KiB → marker 0
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "High half without extension: bank 5 must have upper-marker 0"
        );

        // Set outer bit4=1: write 0x10 to $4132
        mapper.write_prg(0x4132, 0x10);

        // With bit4=1: final bank = (1<<8)|5 = 261 → upper 256 KiB → marker 1
        assert_eq!(
            mapper.read_chr(0x1000),
            1,
            "High half with bit4=1: final bank 261 must have upper-marker 1"
        );
    }

    // --- Bit 0 and bit 4 are independent ---

    #[test]
    fn outer_reg_bit0_and_bit4_are_independent() {
        let mut mapper = make_mapper_ext();
        // Set R0=4 (2 KB at PPU $0000) and R2=6 (1 KB at PPU $1000)
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 6);

        // Set bit0=1, bit4=0: write 0x01
        mapper.write_prg(0x4132, 0x01);

        // Low half (A12=0): bit0=1 → upper 256 KiB → marker 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "With bit0=1: low half must be in upper 256 KiB (marker 1)"
        );
        // High half (A12=1): bit4=0 → lower 256 KiB → marker 0
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "With bit4=0: high half must be in lower 256 KiB (marker 0)"
        );
    }

    // --- Outer register readback ---

    #[test]
    fn outer_reg_is_readable_at_4132() {
        let mut mapper = make_mapper();

        // Default: outer reg = 0
        assert_eq!(
            mapper.read_prg(0x4132),
            0,
            "Default outer reg must read as 0"
        );

        // Write bit0=1
        mapper.write_prg(0x4132, 0x01);
        assert_eq!(
            mapper.read_prg(0x4132),
            1,
            "After writing bit0=1, read-back must return 1"
        );

        // Write bit4=1 only (bit0=0): readback must return bit0=0
        mapper.write_prg(0x4132, 0x10);
        assert_eq!(
            mapper.read_prg(0x4132),
            0,
            "With only bit4=1 set, bit0 read-back must be 0"
        );
    }

    // --- IRQ delegation ---

    #[test]
    fn mmc3_irq_works_through_delegation() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        // Two A12 rising edges, each preceded by ≥3 CPU cycles of A12 low
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 12");
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
        // Write $A000 bit0=1 → Horizontal
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "MMC3 mirroring change must propagate through mapper 12"
        );
    }

    // --- Save state ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Set outer register: bit0=1, bit4=1
        mapper.write_prg(0x4132, 0x11);
        // Set R6=3 via MMC3
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3);

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // PRG bank must be restored
        assert_eq!(
            mapper2.read_prg(0x8000),
            3,
            "R6=3 PRG bank state must survive snapshot round-trip"
        );
        // Outer reg bit0 must be restored (readable at $4132)
        assert_eq!(
            mapper2.read_prg(0x4132),
            1,
            "Outer reg bit0 must survive snapshot round-trip"
        );
    }
}
