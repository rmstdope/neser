//! Mapper 219 – MMC3 variant with custom register layout.
//!
//! Specifications:
//! - Primary source: NesDev wiki not available (403/404)
//! - Fallback source: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_219.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Mmc3Variants/MMC3_219.h>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::nintendo::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 219 – Custom MMC3 variant used by unlicensed multicart boards.
///
/// The register layout for `$8000–$9FFF` is completely custom and does not use
/// the standard MMC3 bank-select / bank-data pair. `$A000–$FFFF` is passed
/// through unchanged to the underlying MMC3 logic (mirroring + IRQ counter).
///
/// ## PRG banking (`$8000–$FFFF`): 4 × 8 KB switchable slots
///
/// Power-on state: all four 8 KB slots are fixed to the last four banks.
///
/// Two-step write sequence:
/// 1. **`addr & 0xE003 == 0x8002`**: set `ex_regs[0] = value`, `ex_regs[1] = 0`.
///    `ex_regs[0]` in `0x23..=0x26` identifies the target slot
///    (`0x23` → slot 3 = `$E000`, `0x26` → slot 0 = `$8000`).
/// 2. **`addr & 0xE003 == 0x8001`**: when `ex_regs[0]` is in range, latch the bank.
///    The 4-bit bank number is bit-scrambled from the data byte:
///    `bank = ((v&0x20)>>5) | ((v&0x10)>>3) | ((v&0x08)>>1) | ((v&0x04)<<1)`.
///
/// ## CHR banking (`$0000–$1FFF`): 8 × 1 KB switchable slots
///
/// Power-on state: all eight 1 KB slots at bank 0.
///
/// Two-step write sequence:
/// 1. **`addr & 0xE003 == 0x8000`**: set `ex_regs[0] = 0`, `ex_regs[1] = value`.
///    `ex_regs[1]` is the CHR command selecting which slot or high-bits register
///    the next data write targets.
/// 2. **`addr & 0xE003 == 0x8001`**: process the CHR command:
///    - `ex_regs[1]` ∈ `{0x08,0x0A,0x0E,0x12,0x16,0x1A,0x1E}` → `ex_regs[2] = value << 4`
///      (upper 4 bits of 8-bit CHR bank number).
///    - `0x09` → `chr_pages[0] = ex_regs[2] | (value>>1 & 0x0E)`
///    - `0x0B` → `chr_pages[1] = ex_regs[2] | (value>>1 | 0x01)`
///    - `0x0C,0x0D` → `chr_pages[2] = ex_regs[2] | (value>>1 & 0x0E)`
///    - `0x0F` → `chr_pages[3] = ex_regs[2] | (value>>1 | 0x01)`
///    - `0x10,0x11` → `chr_pages[4] = ex_regs[2] | (value>>1 & 0x0F)`
///    - `0x14,0x15` → `chr_pages[5] = ex_regs[2] | (value>>1 & 0x0F)`
///    - `0x18,0x19` → `chr_pages[6] = ex_regs[2] | (value>>1 & 0x0F)`
///    - `0x1C,0x1D` → `chr_pages[7] = ex_regs[2] | (value>>1 & 0x0F)`
///
/// ## Mirroring / IRQ
///
/// Writes to `$A000–$FFFF` are forwarded unchanged to the MMC3 core, providing
/// standard H/V mirroring (`$A000`) and scanline-timed IRQ (`$C000–$E001`).
pub struct Mapper219 {
    pub(crate) mmc3: MMC3Mapper,
    /// 4 × 8 KB PRG page selections.  Index 0 = `$8000`, index 3 = `$E000`.
    prg_pages: [usize; 4],
    /// 8 × 1 KB CHR page selections.  Index 0 = PPU `$0000`, index 7 = PPU `$1C00`.
    chr_pages: [usize; 8],
    /// `[0]` = bank-cmd (written by `$8002`), `[1]` = chr-cmd (written by `$8000`),
    /// `[2]` = upper CHR bits (upper nibble of 8-bit CHR bank number).
    ex_regs: [u8; 3],
}

impl Mapper219 {
    const MAPPER_NUMBER: u16 = 219;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KB

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_bank_count = ctx.prg_rom.len() / Self::PRG_BANK_SIZE;
        // Power-on: map the last 4 banks across the four 8 KB slots.
        let prg_pages = [
            prg_bank_count.saturating_sub(4),
            prg_bank_count.saturating_sub(3),
            prg_bank_count.saturating_sub(2),
            prg_bank_count.saturating_sub(1),
        ];
        let mmc3 = MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false);
        Self {
            mmc3,
            prg_pages,
            chr_pages: [0; 8],
            ex_regs: [0; 3],
        }
    }

    /// Decode the 4-bit PRG bank number from a data byte.
    ///
    /// The hardware reverses the bit order of bits `[5:2]`, placing them at
    /// positions `[3:0]` of the resulting bank number.
    fn scramble_prg_bank(value: u8) -> usize {
        (((value & 0x20) >> 5)
            | ((value & 0x10) >> 3)
            | ((value & 0x08) >> 1)
            | ((value & 0x04) << 1)) as usize
    }

    fn prg_page_and_offset(addr: u16) -> (usize, usize) {
        let a = (addr as usize).wrapping_sub(0x8000);
        (a / Self::PRG_BANK_SIZE, a & (Self::PRG_BANK_SIZE - 1))
    }

    fn chr_slot_and_offset(addr: u16) -> (usize, usize) {
        let a = (addr & 0x1FFF) as usize;
        (a / Self::CHR_BANK_SIZE, a & (Self::CHR_BANK_SIZE - 1))
    }

    /// Handle a write to `$8001` when `ex_regs[0]` is in the PRG-slot range.
    fn write_prg_bank(&mut self, value: u8) {
        let bank = Self::scramble_prg_bank(value);
        let slot = (0x26u8.wrapping_sub(self.ex_regs[0])) as usize;
        self.prg_pages[slot] = bank;
    }

    /// Handle a write to `$8001` for CHR banking (based on `ex_regs[1]`).
    fn write_chr_bank(&mut self, value: u8) {
        let hi = self.ex_regs[2] as usize;
        match self.ex_regs[1] {
            0x08 | 0x0A | 0x0E | 0x12 | 0x16 | 0x1A | 0x1E => {
                self.ex_regs[2] = value << 4;
            }
            0x09 => self.chr_pages[0] = hi | (value >> 1 & 0x0E) as usize,
            0x0B => self.chr_pages[1] = hi | (value >> 1 | 0x01) as usize,
            0x0C | 0x0D => self.chr_pages[2] = hi | (value >> 1 & 0x0E) as usize,
            0x0F => self.chr_pages[3] = hi | (value >> 1 | 0x01) as usize,
            0x10 | 0x11 => self.chr_pages[4] = hi | (value >> 1 & 0x0F) as usize,
            0x14 | 0x15 => self.chr_pages[5] = hi | (value >> 1 & 0x0F) as usize,
            0x18 | 0x19 => self.chr_pages[6] = hi | (value >> 1 & 0x0F) as usize,
            0x1C | 0x1D => self.chr_pages[7] = hi | (value >> 1 & 0x0F) as usize,
            _ => {}
        }
    }
}

impl Mapper for Mapper219 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let (page, offset) = Self::prg_page_and_offset(addr);
                self.mmc3.read_prg_at_bank(self.prg_pages[page], offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.mmc3.write_prg(addr, value),
            0x8000..=0x9FFF => match addr & 0xE003 {
                0x8000 => {
                    self.ex_regs[0] = 0;
                    self.ex_regs[1] = value;
                }
                0x8001 => {
                    if self.ex_regs[0] >= 0x23 && self.ex_regs[0] <= 0x26 {
                        self.write_prg_bank(value);
                    }
                    self.write_chr_bank(value);
                }
                0x8002 => {
                    self.ex_regs[0] = value;
                    self.ex_regs[1] = 0;
                }
                _ => {}
            },
            0xA000..=0xFFFF => self.mmc3.write_prg(addr, value),
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let (slot, offset) = Self::chr_slot_and_offset(addr);
        self.mmc3.read_chr_1k_at(self.chr_pages[slot], offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let (slot, offset) = Self::chr_slot_and_offset(addr);
        let count = self.mmc3.chr_bank_count_1k();
        let bank = if count > 0 {
            self.chr_pages[slot] % count
        } else {
            0
        };
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        // Append: prg_pages (4 × u8), chr_pages (8 × u8), ex_regs (3 × u8) = 15 bytes
        for &p in &self.prg_pages {
            snap.push(p as u8);
        }
        for &c in &self.chr_pages {
            snap.push(c as u8);
        }
        snap.extend_from_slice(&self.ex_regs);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const MMC3_SNAPSHOT_SIZE: usize = 16;
        const EXTRA_BYTES: usize = 4 + 8 + 3; // prg_pages + chr_pages + ex_regs

        if data.len() < MMC3_SNAPSHOT_SIZE + EXTRA_BYTES {
            self.mmc3.restore_registers(data);
            return;
        }

        let (mmc3_data, extra) = data.split_at(data.len() - EXTRA_BYTES);
        self.mmc3.restore_registers(mmc3_data);

        for (i, &b) in extra[0..4].iter().enumerate() {
            self.prg_pages[i] = b as usize;
        }
        for (i, &b) in extra[4..12].iter().enumerate() {
            self.chr_pages[i] = b as usize;
        }
        self.ex_regs.copy_from_slice(&extra[12..15]);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 8; // 64 KB PRG
    const CHR_1K_BANKS: usize = 16; // 16 KB CHR

    fn make_mapper(prg_banks: usize, chr_banks: usize) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            219,
            banked_data(8 * 1024, prg_banks),
            banked_data(1024, chr_banks),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 219 should be registered")
    }

    // -------------------------------------------------------------------------
    // Factory
    // -------------------------------------------------------------------------

    #[test]
    fn test_factory_creates_mapper_219() {
        let result = create_mapper(MapperContext::new_for_test(
            219,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 219 must be registered in factory");
        assert_eq!(result.unwrap().mapper_number(), 219);
    }

    // -------------------------------------------------------------------------
    // Power-on state
    // -------------------------------------------------------------------------

    /// At power-on the 4 slots are mapped to the last 4 banks.
    /// banked_data: byte at bank N, offset 0 == N as u8.
    #[test]
    fn test_power_on_prg_last_four_banks() {
        let mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // 8 banks (0–7): slots map to banks 4,5,6,7
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "slot 0 ($8000) should be bank 4"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "slot 1 ($A000) should be bank 5"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "slot 2 ($C000) should be bank 6"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "slot 3 ($E000) should be bank 7"
        );
    }

    /// At power-on all CHR pages are at bank 0.
    #[test]
    fn test_power_on_chr_all_bank_zero() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // banked_data: byte 0 of bank 0 == 0
        for ppu_addr in [
            0x0000u16, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00,
        ] {
            assert_eq!(
                mapper.read_chr(ppu_addr),
                0,
                "CHR at PPU ${:04X} should read bank 0 byte 0",
                ppu_addr
            );
        }
    }

    // -------------------------------------------------------------------------
    // PRG banking
    // -------------------------------------------------------------------------

    /// $8002 write latches ex_regs[0]; subsequent $8001 selects the PRG slot.
    #[test]
    fn test_prg_slot3_select_via_ex_reg_0x23() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // Slot 3 ($E000) = 0x26 – 0x23 = 3
        // Write bank 2: scramble(0x08) = ((0x08)>>1) = 4?
        // Let's pick value 0x08: bits 5,4,3,2 = 0,0,1,0 → bank = (1>>0)|(0>>1)|(0>>2)|(0<<1)
        // scramble(0x08) = ((0&0x20)>>5)|((0&0x10)>>3)|((8&0x08)>>1)|((8&0x04)<<1)
        //                = 0 | 0 | (8>>1) | 0 = 4
        // So bank 4 is selected. banked_data byte 0 of bank 4 == 4.
        mapper.write_prg(0x8002, 0x23); // set ex_regs[0] = 0x23
        mapper.write_prg(0x8001, 0x08); // set slot 3 to bank 4
        assert_eq!(mapper.read_prg(0xE000), 4, "slot 3 should now be bank 4");
    }

    #[test]
    fn test_prg_slot2_select_via_ex_reg_0x24() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // Slot 2 ($C000) = 0x26 – 0x24 = 2
        // scramble(0x10) = ((0x10&0x10)>>3) = 2 → bank 2
        mapper.write_prg(0x8002, 0x24);
        mapper.write_prg(0x8001, 0x10);
        assert_eq!(mapper.read_prg(0xC000), 2, "slot 2 should be bank 2");
    }

    #[test]
    fn test_prg_slot1_select_via_ex_reg_0x25() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // Slot 1 ($A000) = 0x26 – 0x25 = 1
        // scramble(0x20) = ((0x20&0x20)>>5) = 1 → bank 1
        mapper.write_prg(0x8002, 0x25);
        mapper.write_prg(0x8001, 0x20);
        assert_eq!(mapper.read_prg(0xA000), 1, "slot 1 should be bank 1");
    }

    #[test]
    fn test_prg_slot0_select_via_ex_reg_0x26() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // Slot 0 ($8000) = 0x26 – 0x26 = 0
        // scramble(0x04) = ((0x04<<1)) = 8 % 8 = 0 → bank 0
        mapper.write_prg(0x8002, 0x26);
        mapper.write_prg(0x8001, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 0, "slot 0 should be bank 0");
    }

    /// ex_regs[0] values outside 0x23–0x26 should NOT change PRG slots.
    #[test]
    fn test_prg_no_change_when_ex_reg_out_of_range() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        let before_e000 = mapper.read_prg(0xE000); // bank 7
        mapper.write_prg(0x8002, 0x22); // out of range
        mapper.write_prg(0x8001, 0x08);
        assert_eq!(mapper.read_prg(0xE000), before_e000, "PRG slot 3 unchanged");
    }

    /// PRG bit-scramble: value 0x3C has bits 5,4,3,2 = 1,1,1,1 → bank = 1+2+4+8 = 15.
    #[test]
    fn test_prg_bank_scramble_all_bits() {
        // Need at least 16 banks to avoid wrap
        let mut mapper = make_mapper(16, CHR_1K_BANKS);
        mapper.write_prg(0x8002, 0x23); // target slot 3
        mapper.write_prg(0x8001, 0x3C); // scramble → bank 15
        // banked_data: byte 0 of bank 15 == 15
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    /// Multiple writes to $8001 with the same ex_regs[0] update the slot each time.
    #[test]
    fn test_prg_slot_can_be_updated_multiple_times() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        mapper.write_prg(0x8002, 0x23);
        // scramble(0x08) = 4
        mapper.write_prg(0x8001, 0x08);
        assert_eq!(mapper.read_prg(0xE000), 4);
        // scramble(0x20) = 1
        mapper.write_prg(0x8001, 0x20);
        assert_eq!(mapper.read_prg(0xE000), 1);
    }

    // -------------------------------------------------------------------------
    // CHR banking
    // -------------------------------------------------------------------------

    /// Upper CHR bits set via ex_regs[1] = 0x08 command.
    #[test]
    fn test_chr_upper_bits_set_by_0x08_command() {
        // 256 × 1 KB CHR to test upper bits
        let mut mapper = make_mapper(PRG_8K_BANKS, 256);
        // Set upper bits: ex_regs[2] = 0x02 << 4 = 0x20
        mapper.write_prg(0x8000, 0x08); // ex_regs[1] = 0x08
        mapper.write_prg(0x8001, 0x02); // ex_regs[2] = 0x02 << 4 = 0x20

        // Then select CHR page 0 using command 0x09 with value=0x04:
        // chr_pages[0] = 0x20 | ((0x04>>1) & 0x0E) = 0x20 | (0x02 & 0x0E) = 0x20 | 0x02 = 34
        mapper.write_prg(0x8000, 0x09); // ex_regs[1] = 0x09
        mapper.write_prg(0x8001, 0x04); // chr_pages[0] = 0x22 = 34
        // banked_data byte 0 of bank 34 == 34
        assert_eq!(mapper.read_chr(0x0000), 34);
    }

    /// CHR page 0 selection (command 0x09): even bank via `value>>1 & 0x0E`.
    #[test]
    fn test_chr_page0_select_command_0x09() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // ex_regs[2] = 0; value = 0x06: chr_pages[0] = 0 | (3 & 0x0E) = 2
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x06);
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR slot 0 should be bank 2");
    }

    /// CHR page 1 selection (command 0x0B): forces bit 0 = 1.
    #[test]
    fn test_chr_page1_select_command_0x0b_forces_odd_bank() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // value = 0x04: chr_pages[1] = 0 | (2 | 1) = 3
        mapper.write_prg(0x8000, 0x0B);
        mapper.write_prg(0x8001, 0x04);
        assert_eq!(
            mapper.read_chr(0x0400),
            3,
            "CHR slot 1 should be bank 3 (forced odd)"
        );
    }

    /// CHR page 1: even value still gets bit 0 forced to 1.
    #[test]
    fn test_chr_page1_even_value_still_odd() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // value = 0x00: chr_pages[1] = 0 | (0 | 1) = 1
        mapper.write_prg(0x8000, 0x0B);
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(mapper.read_chr(0x0400), 1, "CHR slot 1 bit 0 always 1");
    }

    /// CHR page 2 selection (command 0x0C).
    #[test]
    fn test_chr_page2_select_command_0x0c() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // value = 0x08: chr_pages[2] = 0 | (4 & 0x0E) = 4
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0x8001, 0x08);
        assert_eq!(mapper.read_chr(0x0800), 4, "CHR slot 2 should be bank 4");
    }

    /// CHR page 3 selection (command 0x0F): forces bit 0 = 1.
    #[test]
    fn test_chr_page3_select_command_0x0f_forces_odd() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // value = 0x08: chr_pages[3] = 0 | (4 | 1) = 5
        mapper.write_prg(0x8000, 0x0F);
        mapper.write_prg(0x8001, 0x08);
        assert_eq!(
            mapper.read_chr(0x0C00),
            5,
            "CHR slot 3 should be bank 5 (forced odd)"
        );
    }

    /// CHR page 4 selection (command 0x10).
    #[test]
    fn test_chr_page4_select_command_0x10() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // value = 0x0A: chr_pages[4] = 0 | (5 & 0x0F) = 5
        mapper.write_prg(0x8000, 0x10);
        mapper.write_prg(0x8001, 0x0A);
        assert_eq!(mapper.read_chr(0x1000), 5, "CHR slot 4 should be bank 5");
    }

    /// CHR page 5 selection (command 0x15).
    #[test]
    fn test_chr_page5_select_command_0x15() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        mapper.write_prg(0x8000, 0x15);
        mapper.write_prg(0x8001, 0x0C);
        // chr_pages[5] = 0 | (6 & 0x0F) = 6
        assert_eq!(mapper.read_chr(0x1400), 6, "CHR slot 5 should be bank 6");
    }

    /// CHR page 6 selection (command 0x19).
    #[test]
    fn test_chr_page6_select_command_0x19() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        mapper.write_prg(0x8000, 0x19);
        mapper.write_prg(0x8001, 0x0E);
        // chr_pages[6] = 0 | (7 & 0x0F) = 7
        assert_eq!(mapper.read_chr(0x1800), 7, "CHR slot 6 should be bank 7");
    }

    /// CHR page 7 selection (command 0x1D).
    #[test]
    fn test_chr_page7_select_command_0x1d() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        mapper.write_prg(0x8000, 0x1D);
        mapper.write_prg(0x8001, 0x1E);
        // chr_pages[7] = 0 | (15 & 0x0F) = 15 % 16 = 15
        assert_eq!(mapper.read_chr(0x1C00), 15, "CHR slot 7 should be bank 15");
    }

    // -------------------------------------------------------------------------
    // $8000 clears ex_regs[0]; $8002 clears ex_regs[1]
    // -------------------------------------------------------------------------

    /// Writing to $8000 should clear ex_regs[0] so a pending PRG command is cancelled.
    #[test]
    fn test_write_8000_clears_pending_prg_command() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        let before = mapper.read_prg(0xE000); // bank 7
        mapper.write_prg(0x8002, 0x23); // set PRG cmd for slot 3
        mapper.write_prg(0x8000, 0x09); // $8000 write overrides: ex_regs[0]=0
        mapper.write_prg(0x8001, 0x08); // ex_regs[0]==0 → no PRG update
        // PRG slot 3 unchanged
        assert_eq!(mapper.read_prg(0xE000), before);
    }

    // -------------------------------------------------------------------------
    // Mirroring (pass-through to MMC3 via $A001)
    // -------------------------------------------------------------------------

    #[test]
    fn test_mirroring_horizontal_via_a000() {
        use crate::nes::cartridge::NametableLayout;
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // MMC3 $A000 bit 0 = 1 → horizontal mirroring
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Horizontal,
            "mirroring should be horizontal after $A000=1"
        );
    }

    #[test]
    fn test_mirroring_vertical_via_a000() {
        use crate::nes::cartridge::NametableLayout;
        let mut mapper = create_mapper(MapperContext::new_for_test(
            219,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Horizontal,
        ))
        .unwrap();
        // MMC3 $A000 bit 0 = 0 → vertical mirroring
        mapper.write_prg(0xA000, 0x00);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical after $A000=0"
        );
    }

    // -------------------------------------------------------------------------
    // mmc3_delegate
    // -------------------------------------------------------------------------

    #[test]
    fn test_mmc3_delegate_is_some() {
        let ctx = MapperContext::new_for_test(
            219,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        );
        let mut mapper = Mapper219::new(ctx);
        assert!(mapper.mmc3_delegate().is_some());
        assert!(mapper.mmc3_delegate_mut().is_some());
    }

    // -------------------------------------------------------------------------
    // Registers snapshot / restore
    // -------------------------------------------------------------------------

    #[test]
    fn test_snapshot_round_trip() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        // Set up some non-default state
        mapper.write_prg(0x8002, 0x23);
        mapper.write_prg(0x8001, 0x08); // slot 3 → bank 4
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x06); // chr slot 0 → bank 2

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        mapper2.restore_registers(&snap);

        assert_eq!(mapper2.read_prg(0xE000), mapper.read_prg(0xE000));
        assert_eq!(mapper2.read_chr(0x0000), mapper.read_chr(0x0000));
    }

    /// A truncated snapshot (e.g. legacy format) should not panic.
    #[test]
    fn test_truncated_snapshot_does_not_panic() {
        let mut mapper = make_mapper(PRG_8K_BANKS, CHR_1K_BANKS);
        let snap = mapper.registers_snapshot();
        let truncated = &snap[..snap.len().saturating_sub(5)];
        mapper.restore_registers(truncated); // must not panic
    }
}
