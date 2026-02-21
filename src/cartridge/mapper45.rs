//! Mapper 045 - MMC3 multicart with sequential outer bank registers
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_045>
//!
//! Known Limitations:
//! - IRQ behavior is inherited from the inner MMC3 (see `mmc3.rs` Known Limitations).
//! - Board-specific clone quirks that deviate from the NesDev spec are not modeled.

use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 045 - MMC3-based multicart with 4 sequentially-written outer bank registers
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_045>
/// - PRG-ROM: up to 8 MiB
/// - CHR: up to 8 MiB
///
/// Outer bank registers are written by consecutive writes to $6000–$7FFF.
/// The register pointer auto-increments (mod 4) on each write.
/// Writing to $6001 resets the pointer to 0.
///
/// Register layout:
/// - Reg0: CHR_OR[7:0]  (low 8 bits of CHR bank OR value)
/// - Reg1: PRG_OR[6:0]  (low 7 bits of PRG bank OR value; bit 7 unused)
/// - Reg2: [CCCC MMMM]
///   - CCCC (bits[7:4]): additional CHR_OR bits [11:8] and CHR_AND formula
///   - MMMM (bits[3:0]): determines CHR_AND via shift
///   - CHR_AND = if shift < 8 { 0xFF >> shift } else { 0 }, where shift = 15 - (reg2 & 0x0F)
/// - Reg3: [xLPP PPPP]
///   - Lock bit (bit 6): once set, outer regs are frozen
///   - PRG_AND = !(reg3 & 0x3F) & 0x3F  (inverted lower 6 bits)
///
/// Note: Reg2 bit[6] = CHR_OR bit 10, bit[5] = CHR_OR bit 9 (per NesDev)
/// When reg3 bit6 (lock) is set, writes to $6000 no longer update registers.
pub struct Mapper45 {
    mmc3: MMC3Mapper,
    regs: [u8; 4],
    write_ptr: usize,
    locked: bool,
}

impl Mapper45 {
    const MAPPER_NUMBER: u8 = 45;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            mmc3: MMC3Mapper::new(prg_rom, chr_rom, mirroring),
            regs: [0x00, 0x00, 0x00, 0x00],
            write_ptr: 0,
            locked: false,
        }
    }

    /// Computes PRG_AND from reg3 lower 6 bits (inverted).
    fn prg_and(&self) -> usize {
        (!(self.regs[3] & 0x3F) & 0x3F) as usize
    }

    /// Computes PRG_OR from reg1 lower 7 bits.
    fn prg_or(&self) -> usize {
        (self.regs[1] & 0x7F) as usize
    }

    /// Computes CHR_AND from reg2 lower 4 bits.
    fn chr_and(&self) -> usize {
        let shift = 15usize.saturating_sub((self.regs[2] & 0x0F) as usize);
        if shift < 8 {
            (0xFF >> shift) as usize
        } else {
            0
        }
    }

    /// Computes CHR_OR from reg0 (CHR_OR[7:0]) and reg2 upper nibble (CHR_OR[11:8]).
    fn chr_or(&self) -> usize {
        let lo = self.regs[0] as usize;
        let hi = ((self.regs[2] >> 4) & 0x0F) as usize; // reg2[7:4] → CHR_OR[11:8] (CHR A21:A18)
        lo | (hi << 8)
    }
}

impl Mapper for Mapper45 {
    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let raw = self.mmc3.mapped_prg_bank(addr);
        let bank = (raw & self.prg_and()) | self.prg_or();
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if addr == 0x6001 || (addr & 0x1FFF) == 0x0001 {
                // Any $6001 write resets the register pointer and clears the lock bit.
                self.write_ptr = 0;
                self.locked = false;
            } else if !self.locked {
                self.regs[self.write_ptr] = value;
                if self.write_ptr == 3 && (value & 0x40) != 0 {
                    self.locked = true;
                }
                self.write_ptr = (self.write_ptr + 1) % 4;
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = (raw & self.chr_and()) | self.chr_or();
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = (raw & self.chr_and()) | self.chr_or();
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mmc3.get_mirroring()
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        8 * 1024
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.mmc3.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.mmc3.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.mmc3.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.mmc3.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.extend_from_slice(&self.regs);
        snap.push(self.write_ptr as u8 | if self.locked { 0x80 } else { 0 });
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            let (rest, tail) = data.split_at(data.len() - 5);
            self.regs.copy_from_slice(&tail[0..4]);
            self.write_ptr = (tail[4] & 0x03) as usize;
            self.locked = (tail[4] & 0x80) != 0;
            self.mmc3.restore_registers(rest);
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
    use super::Mapper45;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 128 8KB PRG banks = 1 MiB. Non-power-of-two.
    const PRG_BANKS: usize = 96; // non-pow2, 768 KiB
    const CHR_1K_BANKS: usize = 192; // non-pow2, 192 KiB

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        create_mapper(MapperContext::new(45, prg, chr, NametableLayout::Vertical))
            .expect("Mapper 45 should be implemented")
    }

    fn make_direct() -> Mapper45 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        Mapper45::new(prg, chr, NametableLayout::Vertical)
    }

    // --- Factory ---

    #[test]
    fn mapper_45_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new(
            45,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 45 must be registered in the factory"
        );
    }

    // --- Default state: pass-through MMC3 ---

    #[test]
    fn default_state_passes_through_mmc3_prg() {
        let mut mapper = make_mapper();
        // Default: PRG_AND = !0x00 & 0x3F = 0x3F, PRG_OR = 0
        // Set R6=5 → raw bank 5. (5 & 0x3F) | 0 = 5
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "Default AND/OR with R6=5 must produce bank 5"
        );
    }

    // --- Sequential register writes ---

    #[test]
    fn outer_regs_cycle_0_to_3() {
        let mut m = make_direct();
        // Write 4 values to $6000
        m.write_prg(0x6000, 0x10); // reg0 = 0x10
        m.write_prg(0x6000, 0x20); // reg1 = 0x20
        m.write_prg(0x6000, 0x00); // reg2 = 0x00
        m.write_prg(0x6000, 0x00); // reg3 = 0x00 (no lock)
        // After 4 writes, pointer wraps to 0
        m.write_prg(0x6000, 0x30); // reg0 = 0x30
        assert_eq!(m.regs[0], 0x30, "Reg0 must wrap after 4 writes");
    }

    #[test]
    fn write_to_6001_resets_pointer() {
        let mut m = make_direct();
        m.write_prg(0x6000, 0xAA); // reg0 = 0xAA
        m.write_prg(0x6001, 0x00); // reset pointer
        m.write_prg(0x6000, 0xBB); // reg0 = 0xBB (starts over)
        assert_eq!(m.regs[0], 0xBB, "Reg0 must be 0xBB after pointer reset");
        assert_eq!(
            m.regs[1], 0x00,
            "Reg1 must remain at default after pointer reset"
        );
    }

    // --- PRG bank mapping ---

    #[test]
    fn prg_and_or_applied_to_mmc3_bank() {
        let mut m = make_direct();
        // Set: reg0=0, reg1=0x08 (PRG_OR=8), reg2=0, reg3=0x0F (PRG_AND=!0x0F & 0x3F = 0x30)
        m.write_prg(0x6000, 0x00); // reg0
        m.write_prg(0x6000, 0x08); // reg1: PRG_OR = 8
        m.write_prg(0x6000, 0x00); // reg2
        m.write_prg(0x6000, 0x0F); // reg3: PRG_AND = !0x0F & 0x3F = 0x30
        // Select MMC3 R6=2 → $8000 raw bank = 2. (2 & 0x30) | 0x08 = 0 | 8 = 8
        m.write_prg(0x8000, 0b0000_0110); // R6
        m.write_prg(0x8001, 2);
        assert_eq!(
            m.read_prg(0x8000),
            8,
            "PRG AND/OR: R6=2, AND=0x30, OR=8 → bank 8"
        );
    }

    #[test]
    fn prg_fixed_last_with_and_or() {
        let mut m = make_direct();
        // reg1 = PRG_OR = 0x10, reg3 = 0 (PRG_AND = 0x3F)
        m.write_prg(0x6000, 0x00); // reg0
        m.write_prg(0x6000, 0x10); // reg1: PRG_OR=0x10
        m.write_prg(0x6000, 0x00); // reg2
        m.write_prg(0x6000, 0x00); // reg3: PRG_AND=0x3F
        // R6=0 → raw bank 0. (0 & 0x3F) | 0x10 = 16 — OR actually contributes here
        m.write_prg(0x8000, 0b0000_0110); // R6
        m.write_prg(0x8001, 0); // R6=0
        assert_eq!(
            m.read_prg(0x8000),
            16,
            "PRG R6=0, AND=0x3F, OR=0x10: (0 & 0x3F) | 0x10 = 16"
        );
    }

    // --- CHR bank mapping ---

    #[test]
    fn chr_and_or_applied_to_mmc3_bank() {
        let mut m = make_direct();
        // reg0 = CHR_OR_lo = 0x08, reg2 = 0x0F → shift = 15 - 15 = 0, CHR_AND = 0xFF>>0 = 0xFF
        m.write_prg(0x6000, 0x08); // reg0: CHR_OR[7:0] = 8
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x0F); // reg2: shift = 15-15=0, CHR_AND=0xFF; CHR_OR hi=0
        m.write_prg(0x6000, 0x00); // reg3
        // Set R2=3 → raw CHR bank at $1000 = 3. (3 & 0xFF) | 8 = 11.
        m.write_prg(0x8000, 0b0000_0010); // R2
        m.write_prg(0x8001, 3);
        assert_eq!(
            m.read_chr(0x1000),
            11,
            "CHR AND/OR: R2=3, AND=0xFF, OR=8 → bank 11"
        );
    }

    #[test]
    fn chr_and_zero_masks_all_bits() {
        let mut m = make_direct();
        // reg2 = 0x00 → shift = 15 - 0 = 15 (>=8), CHR_AND = 0
        m.write_prg(0x6000, 0x05); // reg0: CHR_OR = 5
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x00); // reg2: CHR_AND = 0
        m.write_prg(0x6000, 0x00); // reg3
        // R2=7 → raw=7. (7 & 0) | 5 = 5
        m.write_prg(0x8000, 0b0000_0010); // R2
        m.write_prg(0x8001, 7);
        assert_eq!(
            m.read_chr(0x1000),
            5,
            "CHR_AND=0 masks all: result = CHR_OR = 5"
        );
    }

    // --- Lock bit ---

    #[test]
    fn lock_bit_prevents_further_writes() {
        let mut m = make_direct();
        // NesDev spec: reg3 bit 6 (L) is the lock bit.
        m.write_prg(0x6000, 0x00); // reg0
        m.write_prg(0x6000, 0x05); // reg1: PRG_OR=5
        m.write_prg(0x6000, 0x0F); // reg2
        m.write_prg(0x6000, 0x40); // reg3: bit 6 = lock
        assert!(
            m.locked,
            "Lock bit must be set after writing 0x40 (bit 6) to reg3"
        );
        // Try to write reg0 again — should be ignored
        m.write_prg(0x6000, 0xFF);
        assert_eq!(m.regs[0], 0x00, "Reg0 must not change after lock");
        assert_eq!(m.regs[1], 0x05, "Reg1 must not change after lock");
    }

    #[test]
    fn lock_bit_is_bit6_not_bit7() {
        // NesDev spec: bit 6 of reg3 is the lock, NOT bit 7.
        // Brain Series 12-in-1 writes reg3=0xBF (bit7=1, bit6=0) to indicate NO lock.
        let mut m = make_direct();
        m.write_prg(0x6000, 0x00); // reg0
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x00); // reg2
        m.write_prg(0x6000, 0x80); // reg3: bit 7 set, bit 6 clear — must NOT lock
        assert!(
            !m.locked,
            "bit 7 alone must NOT set the lock; only bit 6 locks"
        );
    }

    #[test]
    fn reg3_0xbf_does_not_lock() {
        // Brain Series 12-in-1 menu writes reg3=0xBF (0b1011_1111).
        // bit7=1, bit6=0 → must NOT lock, so sub-game selection writes can proceed.
        let mut m = make_direct();
        m.write_prg(0x6000, 0x00); // reg0
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x00); // reg2
        m.write_prg(0x6000, 0xBF); // reg3: Brain Series menu value
        assert!(!m.locked, "reg3=0xBF must NOT lock the mapper (bit 6 = 0)");
        // Outer reg writes must still work
        m.write_prg(0x6000, 0x42); // reg0 must accept this
        assert_eq!(m.regs[0], 0x42, "reg0 must be writable when not locked");
    }

    #[test]
    fn lock_does_not_prevent_mmc3_writes() {
        let mut m = make_direct();
        // Lock via bit 6
        m.write_prg(0x6000, 0x00);
        m.write_prg(0x6000, 0x00);
        m.write_prg(0x6000, 0x00);
        m.write_prg(0x6000, 0x40);
        // MMC3 writes must still work
        m.write_prg(0x8000, 0b0000_0110); // R6
        m.write_prg(0x8001, 7);
        // PRG_AND=0x3F, PRG_OR=0. R6=7 → (7 & 0x3F) | 0 = 7
        assert_eq!(
            m.read_prg(0x8000),
            7,
            "MMC3 writes must still work after lock"
        );
    }

    // --- Power-on state ---

    #[test]
    fn initial_reg0_is_zero() {
        // NES hardware spec: all outer bank registers reset to 0x00 at power-on.
        // reg0 = 0x00 means CHR_OR[7:0] = 0.
        let m = make_direct();
        assert_eq!(m.regs[0], 0x00, "reg0 must be 0x00 at power-on (not 0xFF)");
    }

    // --- $6001 lock clearing ---

    #[test]
    fn write_to_6001_clears_lock() {
        // NesDev spec: writing any value to $6001 clears the Lock bit,
        // making the next write to $6000 go to register #0.
        let mut m = make_direct();
        // Lock the mapper
        m.write_prg(0x6000, 0x01); // reg0
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x00); // reg2
        m.write_prg(0x6000, 0x40); // reg3: bit 6 = lock
        assert!(m.locked, "must be locked before test");
        // Write to $6001 must clear the lock
        m.write_prg(0x6001, 0x00);
        assert!(!m.locked, "$6001 write must clear the lock bit");
        // After clearing lock, $6000 write must go to reg0
        m.write_prg(0x6000, 0x42);
        assert_eq!(
            m.regs[0], 0x42,
            "reg0 must be writable after $6001 clears lock"
        );
    }

    // --- CHR bank out-of-bounds wrapping ---

    #[test]
    fn chr_bank_wraps_modulo_rom_size() {
        // NES hardware: CHR bank numbers that exceed ROM size must wrap (modulo),
        // not return zero. Uses 7 banks (non-power-of-two) so that
        // the wrapped result (9 % 7 = 2) differs from the out-of-bounds result (0).
        let prg = banked_data(8 * 1024, 8);
        let chr = banked_data(1024, 7); // banks 0-6; bank N filled with byte N
        let mut m = Mapper45::new(prg, chr, NametableLayout::Vertical);
        // CHR_OR = 9, CHR_AND = 0 → all CHR maps to bank 9
        // 9 % 7 = 2 → bank 2 → first byte = 0x02
        m.write_prg(0x6000, 9); // reg0: CHR_OR = 9
        m.write_prg(0x6000, 0); // reg1
        m.write_prg(0x6000, 0); // reg2: CHR_AND = 0, CHR_OR hi = 0
        m.write_prg(0x6000, 0); // reg3: no lock
        assert_eq!(
            m.read_chr(0x0000),
            2,
            "CHR bank 9 must wrap to bank 9%7=2; reads bank-2 marker 0x02"
        );
    }

    // --- chr_or() uses reg2 bit 7 (CHR A21/bank bit 11) ---

    #[test]
    fn chr_or_uses_reg2_bit7_for_bank_bit11() {
        // NesDev spec: reg2[7:4] contributes CHR_OR[11:8] (CHR A21:A18).
        // reg2=0x80 → upper nibble = 0x8 → CHR_OR = 0x800 = 2048, 2048 % 3 = 2
        // → reads bank 2 (marker 0x02).
        let prg = banked_data(8 * 1024, 8);
        let chr = banked_data(1024, 3); // banks 0-2; bank N filled with byte N
        let mut m = Mapper45::new(prg, chr, NametableLayout::Vertical);
        // reg0=0x00, reg2=0x80 (only bit 7 set → CHR_OR[11]=1 → CHR_OR=0x800=2048)
        // CHR_AND = 0 (lower nibble of reg2 = 0) → bank = chr_or = 2048
        // 2048 % 3 = 2 → reads bank 2 (value 0x02)
        m.write_prg(0x6000, 0x00); // reg0: CHR_OR lo = 0
        m.write_prg(0x6000, 0x00); // reg1
        m.write_prg(0x6000, 0x80); // reg2: bit7=1 → CHR_OR[11]=1
        m.write_prg(0x6000, 0x00); // reg3: no lock
        assert_eq!(
            m.read_chr(0x0000),
            2,
            "reg2 bit 7 must set CHR_OR[11]; 0x800 % 3 = 2 → bank 2 marker 0x02"
        );
    }
}
