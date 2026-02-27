//! Mapper 037 - ZZ board MMC3 multicart (Super Mario Bros + Tetris + Nintendo World Cup)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_037>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 037 - ZZ board MMC3-based 3-in-1 multicart
///
/// Hardware: MMC3 inner mapper + outer 3-bit register at $6000-$7FFF
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_037>
/// - PRG-ROM: 256 KiB total (addressed via outer register + MMC3)
/// - CHR: 256 KiB total (upper half selected by Q2)
/// - No PRG-RAM ($6000-$7FFF is the outer block register only)
///
/// Outer register ($6000-$7FFF): [.... .Q2Q1Q0]
/// - Writable only when MMC3's PRG-RAM is enabled and write-protect is clear ($A001)
/// - Power-on state: Q = 0
///
/// PRG bank mapping (8KB banks):
/// - A16 = (Q1 & Q0) | (Q2 & M16)  where M16 = bit 4 of MMC3's raw PRG bank
/// - A17 = Q2
/// - outer_block = (A17 << 1) | A16
/// - final_bank  = (outer_block << 4) | (mmc3_bank & 0x0F)
///
/// CHR 1KB bank mapping:
/// - final_bank = (Q2 << 7) | (mmc3_1k_bank & 0x7F)
///
/// Known games: Super Mario Bros, Tetris, Nintendo World Cup
pub struct Mapper37 {
    inner: MMC3Mapper,
    outer_reg: u8, // bits [2:0] = Q[2:0]
}

impl Mapper37 {
    const MAPPER_NUMBER: u8 = 37;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB (same as MMC3)
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB (same as MMC3)
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            outer_reg: 0,
        }
    }

    /// Adjust the MMC3's raw 8KB PRG bank using the outer register.
    ///
    /// PRG A16 = (Q1 & Q0) | (Q2 & M16)
    /// PRG A17 = Q2
    fn adjust_prg_bank(&self, mmc3_bank: usize) -> usize {
        let q = self.outer_reg as usize;
        let q0 = q & 1;
        let q1 = (q >> 1) & 1;
        let q2 = (q >> 2) & 1;
        let m16 = (mmc3_bank >> 4) & 1;
        let a16 = (q1 & q0) | (q2 & m16);
        let a17 = q2;
        let outer_block = (a17 << 1) | a16;
        (outer_block << 4) | (mmc3_bank & 0x0F)
    }

    /// Adjust the MMC3's raw 1KB CHR bank using the outer register.
    ///
    /// CHR A17 = Q2
    fn adjust_chr_bank(&self, mmc3_1kb_bank: usize) -> usize {
        let q2 = (self.outer_reg as usize >> 2) & 1;
        (q2 << 7) | (mmc3_1kb_bank & 0x7F)
    }
}

impl Mapper for Mapper37 {
    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0; // No PRG-RAM; $6000-$7FFF is the outer register only
        }
        let raw_bank = self.inner.mapped_prg_bank(addr);
        let final_bank = self.adjust_prg_bank(raw_bank);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.inner.read_prg_at_bank(final_bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.inner.is_prg_ram_writable() {
                self.outer_reg = value & 0x07;
            }
        } else {
            self.inner.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw_bank = self.inner.mapped_chr_1k_bank(addr);
        let final_bank = self.adjust_chr_bank(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.inner.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw_bank = self.inner.mapped_chr_1k_bank(addr);
        let final_bank = self.adjust_chr_bank(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.inner.write_chr_1k_at(final_bank, offset, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.inner.get_mirroring()
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0 // No PRG-RAM; $6000-$7FFF is the outer block register
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.inner.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.inner.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.inner.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.push(self.outer_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer_reg, mmc3_data)) = data.split_last() {
            self.outer_reg = outer_reg & 0x07;
            self.inner.restore_registers(mmc3_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
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

    // 48 PRG banks × 8 KiB = 384 KiB (non-power-of-two avoids modulo false-positives)
    // outer_block 0 → banks 0-15, block 1 → banks 16-31, block 2 → banks 32-47
    const PRG_BANKS: usize = 48;
    // 256 CHR 1KB banks = 256 KiB (Q2=0 → banks 0-127, Q2=1 → banks 128-255)
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            37,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 37 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_37_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            37,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 37 must be registered in the factory"
        );
    }

    // --- Power-on / PRG block 0 ---

    /// At power-on Q=0: outer_block=0, fixed-last PRG = bank 15 (first 128 KiB)
    ///
    /// With 48 PRG banks, MMC3 fixed-last = 47 (0x2F).
    /// Q=0: a16=0, a17=0 → outer_block=0 → final = (0<<4)|(0x2F & 0xF) = 15
    #[test]
    fn power_on_outer_reg_is_0_prg_from_first_64kb() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Power-on Q=0: fixed-last PRG must be bank 15 (first 128 KiB block)"
        );
    }

    /// Q=0, MMC3 R6=3 → $8000 reads bank 3 (within outer_block 0)
    #[test]
    fn prg_banking_with_mmc3_registers_within_first_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110); // select MMC3 register R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        // Q=0: final = (0<<4)|(3&0xF) = 3
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Q=0, R6=3: $8000 must read bank 3"
        );
    }

    /// Q=3 (0b011): a16=(Q1&Q0)|(Q2&M16)=(1&1)|(0&M16)=1, a17=0 → outer_block=1
    /// fixed-last 47 → final = (1<<4)|(0x2F & 0xF) = 16+15 = 31
    #[test]
    fn outer_reg_q3_selects_second_64kb_prg_block() {
        let mut mapper = make_mapper();
        // Enable PRG-RAM write ($A001=0x80) so outer register is writable
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x03); // Q=3
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Q=3: fixed-last PRG must be bank 31 (second 128 KiB block)"
        );
    }

    /// Q=4 (0b100): Q2=1, Q1=0, Q0=0.
    /// fixed-last=47 (0x2F): M16=(47>>4)&1 = 2&1 = 0
    /// a16=(0&0)|(1&0)=0, a17=1 → outer_block=2
    /// final = (2<<4)|(0x2F & 0xF) = 32+15 = 47
    #[test]
    fn outer_reg_q4_q2_set_selects_third_128kb_prg_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x04); // Q=4
        assert_eq!(
            mapper.read_prg(0xE000),
            47,
            "Q=4: fixed-last PRG must be bank 47 (third 128 KiB block)"
        );
    }

    // --- CHR banking ---

    /// Q=0 (Q2=0): CHR stays in first 128 KiB. R2=5 → final CHR = (0<<7)|(5&0x7F) = 5
    /// Q=4 (Q2=1): CHR shifts to second 128 KiB. R2=5 → final CHR = (1<<7)|(5&0x7F) = 133
    #[test]
    fn chr_a17_follows_q2() {
        let mut mapper = make_mapper();
        // Q=0: CHR bank at $1000 (R2=5) should be bank 5
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 5); // R2=5
        assert_eq!(
            mapper.read_chr(0x1000),
            5,
            "Q=0 (Q2=0): CHR R2=5 must map to bank 5 (first 128 KiB)"
        );

        // Switch to Q=4 (Q2=1): same R2=5 but shifted to second 128 KiB
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x04); // Q=4
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "Q=4 (Q2=1): CHR R2=5 must map to bank 133 (second 128 KiB)"
        );
    }

    // --- Outer register write protection ---

    /// Outer register write at $6000 is only accepted when MMC3 PRG-RAM is enabled
    /// and write-protect is clear ($A001 bit7=1, bit6=0).
    #[test]
    fn outer_reg_write_requires_prg_ram_write_enable() {
        let mut mapper = make_mapper();
        // At power-on PRG-RAM is writable; set Q=3 to move to block 1
        mapper.write_prg(0x6000, 0x03); // Q=3 accepted (power-on state is writable)
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Q=3 accepted at power-on: fixed-last must be 31"
        );

        // Disable PRG-RAM write-enable ($A001=0xC0: enabled but write-protected)
        mapper.write_prg(0xA001, 0xC0);
        mapper.write_prg(0x6000, 0x00); // attempt to reset Q → must be rejected
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Q write must be rejected when PRG-RAM is write-protected (outer_reg stays 3)"
        );

        // Fully disable PRG-RAM ($A001=0x00)
        mapper.write_prg(0xA001, 0x00);
        mapper.write_prg(0x6000, 0x00); // attempt → must be rejected
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Q write must be rejected when PRG-RAM is disabled"
        );
    }

    // --- No actual PRG-RAM at $6000 ---

    /// Reads from $6000-$7FFF must return 0 (no PRG-RAM window; only outer register).
    #[test]
    fn no_actual_prg_ram_at_6000() {
        let mut mapper = make_mapper();
        // Write a recognisable value; the outer register stores it but the address space is silent
        mapper.write_prg(0x6000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "$6000 read must return 0 (no PRG-RAM)"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0,
            "$7FFF read must return 0 (no PRG-RAM)"
        );
    }

    // --- IRQ delegation ---

    #[test]
    fn mmc3_irq_delegated() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // IRQ latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        // Two A12 rising edges with 3 CPU cycles low between each
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 37");
    }

    // --- Snapshot round-trip ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Set Q=3 (requires PRG-RAM writable at power-on)
        mapper.write_prg(0x6000, 0x03);
        let snap = mapper.registers_snapshot();

        // Restore into a fresh mapper and verify Q=3 behaviour
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        let mut mapper2 = create_mapper(MapperContext::new_for_test(
            37,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 37 must be creatable");
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0xE000),
            31,
            "After restore with Q=3: fixed-last PRG must be 31"
        );
    }
}
