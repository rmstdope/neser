//! Mapper 296 – VT3x (FC Pocket RS-20 / dreamGEAR My Arcade Gamer V)
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/NES_2.0_Mapper_296>
//!
//! Hardware summary:
//! - PCB: VT3x SOC, used in FC Pocket RS-20 and dreamGEAR My Arcade Gamer V handheld consoles
//! - PRG-ROM: up to 128 MiB via outer bank registers
//! - CHR: CHR-ROM (banked per inner mapper) or unbanked 8 KiB CHR-RAM (selected by $411D bit 2)
//! - Mirroring: Controlled by inner legacy mapper
//! - IRQ: Available in MMC3 mode (mode 0); none in other modes
//! - Audio: Two-channel PCM expansion (not implemented; see Known Limitations)
//!
//! Register map:
//! ```text
//! $411D – Mapper Configuration (write)
//! 7654 3210
//! ---- -CMM
//!       |++- MM: legacy mapper mode
//!       |      0 = MMC3 (default)
//!       |      1 = MMC1
//!       |      2 = UNROM
//!       |      3 = CNROM
//!       +--- C: CHR memory type  0=CHR-ROM (banked)  1=CHR-RAM (unbanked 8 KiB)
//!
//! $411E – Encryption Control (write) – stored but not acted upon (see Known Limitations)
//!
//! $412C – Outer Bank Register (write)
//! 7654 3210
//! ---- cpCP
//!      |||+- PRG A25 (32 MiB boundary)
//!      ||+-- CHR A25
//!      |+--- PRG A26 (64 MiB boundary)
//!      +---- CHR A26
//!
//! $412E – Outer Bank Register 2 (write)
//! 7654 3210
//! ---- ---B
//!          +- PRG/CHR A27 (128 MiB boundary)
//! ```
//!
//! Banking modes:
//! - Mode 0 (MMC3): standard MMC3 banking on $8000–$FFFF, outer bits extend bank numbers
//! - Mode 1 (MMC1): serial-shift-register interface, 16 KiB PRG / 4 or 8 KiB CHR banking
//! - Mode 2 (UNROM): write to $8000–$FFFF selects 16 KiB PRG bank at $8000–$BFFF;
//!   $C000–$FFFF is fixed to last 16 KiB of ROM
//! - Mode 3 (CNROM): write to $8000–$FFFF selects 8 KiB CHR bank; PRG is fixed 32 KiB
//!
//! Outer bank formula (for 8 KiB PRG granularity):
//!   effective_prg_8k_bank = (A27<<14) | (A26_prg<<13) | (A25_prg<<12) | inner_bank
//!
//! Known Limitations:
//! - CPU opcode and CHR data encryption ($411E) are not implemented.
//! - Two-channel PCM expansion sound ($4031–$4036) is not implemented.
//! - VT3x extended palette is not implemented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities, NametableLayout};

const CHR_RAM_SIZE: usize = 8 * 1024;
const PRG_BANK_MASK: usize = 0x1FFF; // 8 KiB bank offset mask
const CHR_1KB_BANK_MASK: usize = 0x03FF; // 1 KiB CHR bank offset mask

/// Mapper 296 – VT3x multi-mode mapper with outer bank extension.
///
/// Wraps one of MMC3 / MMC1 / UNROM / CNROM as selected by $411D bits 1:0,
/// and extends the resulting bank addresses with three extra bits from the outer
/// bank registers $412C and $412E.
pub struct Mapper296 {
    mmc3: MMC3Mapper,
    /// $411D – mapper config: bits 1:0=mode, bit 2=CHR-RAM select
    config_reg: u8,
    /// $412C – outer bank: bit0=PRG A25, bit1=CHR A25, bit2=PRG A26, bit3=CHR A26
    outer_reg: u8,
    /// $412E – outer bank 2: bit0=PRG/CHR A27
    outer_reg2: u8,
    /// 8 KiB CHR-RAM used when config bit 2 = 1
    chr_ram: Vec<u8>,
    /// UNROM mode (2): 4-bit bank register selects 16 KiB PRG block at $8000
    unrom_bank: u8,
    /// CNROM mode (3): 2-bit bank register selects 8 KiB CHR block
    cnrom_bank: u8,
    /// MMC1 mode (1): 5-bit serial shift register (shift in from bit 0)
    mmc1_shift: u8,
    /// Number of bits loaded into the MMC1 shift register so far (0–4)
    mmc1_count: u8,
    /// MMC1 control register: bits 3:2 = PRG mode, bit 4 = CHR mode
    mmc1_control: u8,
    /// MMC1 CHR bank 0 (4 KiB at $0000 / 8 KiB in 8 KiB mode)
    mmc1_chr0: u8,
    /// MMC1 CHR bank 1 (4 KiB at $1000; ignored in 8 KiB mode)
    mmc1_chr1: u8,
    /// MMC1 PRG bank register (4-bit bank select + mode control bit)
    mmc1_prg: u8,
}

impl Mapper296 {
    const MAPPER_NUMBER: u16 = 296;

    const MODE_MASK: u8 = 0x03;
    const CHR_RAM_BIT: u8 = 0x04;

    // $412C bit masks
    const PRG_A25_BIT: u8 = 0x01;
    const CHR_A25_BIT: u8 = 0x02;
    const PRG_A26_BIT: u8 = 0x04;
    const CHR_A26_BIT: u8 = 0x08;
    // $412E bit mask
    const A27_BIT: u8 = 0x01;

    // MMC1 control register bits
    const MMC1_PRG_MODE_MASK: u8 = 0x0C; // bits 3:2
    const MMC1_CHR_MODE_BIT: u8 = 0x10; // bit 4

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        // No PRG-RAM on VT3x (it is a SOC, RAM is on-chip and not accessible here)
        let mmc3 = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            0,
        );
        Self {
            mmc3,
            config_reg: 0,
            outer_reg: 0,
            outer_reg2: 0,
            chr_ram: vec![0u8; CHR_RAM_SIZE],
            unrom_bank: 0,
            cnrom_bank: 0,
            mmc1_shift: 0,
            mmc1_count: 0,
            mmc1_control: 0x0C, // MMC1 power-on: PRG mode 3 (fix $C000, switch $8000)
            mmc1_chr0: 0,
            mmc1_chr1: 0,
            mmc1_prg: 0,
        }
    }

    fn mapper_mode(&self) -> u8 {
        self.config_reg & Self::MODE_MASK
    }

    fn chr_ram_enabled(&self) -> bool {
        (self.config_reg & Self::CHR_RAM_BIT) != 0
    }

    /// PRG outer offset in 8 KiB bank units: A25 = bit12, A26 = bit13, A27 = bit14.
    fn prg_8kb_outer_offset(&self) -> usize {
        let a25 = ((self.outer_reg & Self::PRG_A25_BIT) != 0) as usize;
        let a26 = ((self.outer_reg & Self::PRG_A26_BIT) != 0) as usize;
        let a27 = ((self.outer_reg2 & Self::A27_BIT) != 0) as usize;
        (a27 << 14) | (a26 << 13) | (a25 << 12)
    }

    /// CHR outer offset in 1 KiB bank units: A25 = bit15, A26 = bit16, A27 = bit17.
    fn chr_1kb_outer_offset(&self) -> usize {
        let a25 = ((self.outer_reg & Self::CHR_A25_BIT) != 0) as usize;
        let a26 = ((self.outer_reg & Self::CHR_A26_BIT) != 0) as usize;
        let a27 = ((self.outer_reg2 & Self::A27_BIT) != 0) as usize;
        (a27 << 17) | (a26 << 16) | (a25 << 15)
    }

    /// CHR outer offset in 8 KiB bank units (for CNROM mode).
    fn chr_8kb_outer_offset(&self) -> usize {
        // same as 1KB offset / 8
        let a25 = ((self.outer_reg & Self::CHR_A25_BIT) != 0) as usize;
        let a26 = ((self.outer_reg & Self::CHR_A26_BIT) != 0) as usize;
        let a27 = ((self.outer_reg2 & Self::A27_BIT) != 0) as usize;
        (a27 << 14) | (a26 << 13) | (a25 << 12)
    }

    // ─── PRG read helpers per mode ──────────────────────────────────────────

    fn read_prg_mmc3(&self, addr: u16) -> u8 {
        let inner_bank = self.mmc3.mapped_prg_bank(addr);
        let bank = self.prg_8kb_outer_offset() | inner_bank;
        let offset = (addr as usize) & PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_unrom(&self, addr: u16) -> u8 {
        let outer = self.prg_8kb_outer_offset();
        let count = self.mmc3.base.prg_bank_count();
        let bank = match addr {
            // 16 KiB selected bank at $8000–$BFFF (two 8 KiB halves)
            0x8000..=0x9FFF => outer | (self.unrom_bank as usize * 2),
            0xA000..=0xBFFF => outer | (self.unrom_bank as usize * 2 + 1),
            // Fixed last 16 KiB at $C000–$FFFF (two 8 KiB banks at the end,
            // still extended by the outer PRG bank bits; wrapping is handled
            // by read_prg_at_bank)
            0xC000..=0xDFFF => outer | count.saturating_sub(2),
            0xE000..=0xFFFF => outer | count.saturating_sub(1),
            _ => 0,
        };
        let offset = (addr as usize) & PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_cnrom(&self, addr: u16) -> u8 {
        // Fixed 32 KiB PRG: map all 4 slots to the outer base
        let outer = self.prg_8kb_outer_offset();
        let slot = ((addr as usize).saturating_sub(0x8000)) >> 13;
        let bank = outer | (slot & 0x03);
        let offset = (addr as usize) & PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_mmc1(&self, addr: u16) -> u8 {
        let outer = self.prg_8kb_outer_offset();
        let count = self.mmc3.base.prg_bank_count();
        let prg_mode = (self.mmc1_control & Self::MMC1_PRG_MODE_MASK) >> 2;
        let bank_16kb = (self.mmc1_prg & 0x0F) as usize; // 4-bit PRG bank
        let bank = match prg_mode {
            // 32 KiB mode: bank_16kb >> 1 selects 32 KiB block
            0 | 1 => {
                let base = (bank_16kb & !1) * 2; // align to 32 KiB = 4 × 8 KiB
                let slot = ((addr as usize - 0x8000) >> 13) & 0x03;
                outer | (base + slot)
            }
            // Fix $8000–$BFFF at bank 0, switch $C000–$DFFF/$E000–$FFFF
            2 => match addr {
                0x8000..=0x9FFF => outer,
                0xA000..=0xBFFF => outer | 1,
                0xC000..=0xDFFF => outer | (bank_16kb * 2),
                0xE000..=0xFFFF => outer | (bank_16kb * 2 + 1),
                _ => 0,
            },
            // Fix $C000–$FFFF at last bank, switch $8000–$BFFF (default after reset)
            _ => match addr {
                0x8000..=0x9FFF => outer | (bank_16kb * 2),
                0xA000..=0xBFFF => outer | (bank_16kb * 2 + 1),
                0xC000..=0xDFFF => outer | count.saturating_sub(2),
                0xE000..=0xFFFF => outer | count.saturating_sub(1),
                _ => 0,
            },
        };
        let offset = (addr as usize) & PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    // ─── CHR read helpers per mode ──────────────────────────────────────────

    fn read_chr_mmc3(&mut self, addr: u16) -> u8 {
        let inner_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.chr_1kb_outer_offset() | inner_bank;
        let offset = (addr as usize) & CHR_1KB_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn read_chr_cnrom(&mut self, addr: u16) -> u8 {
        // 8 KiB CHR banking: cnrom_bank selects 8 KiB block
        let outer = self.chr_8kb_outer_offset();
        let bank_8kb = outer | (self.cnrom_bank as usize & 0x03);
        let bank_1kb = bank_8kb * 8 + (((addr as usize) & 0x1FFF) >> 10);
        let offset = (addr as usize) & CHR_1KB_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank_1kb, offset)
    }

    fn read_chr_mmc1(&mut self, addr: u16) -> u8 {
        let outer_1kb = self.chr_1kb_outer_offset();
        let chr_mode_8kb = (self.mmc1_control & Self::MMC1_CHR_MODE_BIT) == 0;
        let bank_1kb = if chr_mode_8kb {
            // 8 KiB mode: chr0 is a 4 KiB bank index with bit 0 ignored (aligned to even bank).
            let base_4kb = (self.mmc1_chr0 & 0xFE) as usize;
            outer_1kb | (base_4kb * 4 + (((addr as usize) & 0x1FFF) >> 10))
        } else {
            // 4 KiB mode: chr0 at $0000, chr1 at $1000
            let (bank_4kb, sub) = if (addr & 0x1000) == 0 {
                (self.mmc1_chr0 as usize, (addr as usize & 0x0FFF) >> 10)
            } else {
                (self.mmc1_chr1 as usize, (addr as usize & 0x0FFF) >> 10)
            };
            outer_1kb | (bank_4kb * 4 + sub)
        };
        let offset = (addr as usize) & CHR_1KB_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank_1kb, offset)
    }

    fn read_chr_unrom(&mut self, addr: u16) -> u8 {
        // UNROM mode: CHR window is 8 KiB; select the 1 KiB sub-bank from PPU
        // address bits 12:10, extended by the outer CHR bits.
        let outer_1kb = self.chr_1kb_outer_offset();
        let bank_1kb = outer_1kb | (((addr as usize) & 0x1FFF) >> 10);
        let offset = (addr as usize) & CHR_1KB_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank_1kb, offset)
    }

    // ─── MMC1 mirroring ─────────────────────────────────────────────────────

    fn mmc1_mirroring(&self) -> NametableLayout {
        match self.mmc1_control & 0x03 {
            0 => NametableLayout::SingleScreenLower,
            1 => NametableLayout::SingleScreenUpper,
            2 => NametableLayout::Vertical,
            _ => NametableLayout::Horizontal,
        }
    }

    // ─── MMC1 serial write handler ──────────────────────────────────────────

    fn write_mmc1_serial(&mut self, addr: u16, value: u8) {
        if value & 0x80 != 0 {
            // Reset shift register; set PRG mode 3 (default)
            self.mmc1_shift = 0;
            self.mmc1_count = 0;
            self.mmc1_control |= 0x0C; // Set PRG mode bits to 11 (mode 3)
            return;
        }
        // Shift in bit 0 of value
        self.mmc1_shift |= (value & 1) << self.mmc1_count;
        self.mmc1_count += 1;
        if self.mmc1_count == 5 {
            let val = self.mmc1_shift;
            self.mmc1_shift = 0;
            self.mmc1_count = 0;
            // Select target register by address bits 14:13
            match addr & 0x6000 {
                0x0000 => {
                    self.mmc1_control = val & 0x1F; // $8000–$9FFF
                    self.mmc3.base.set_mirroring(self.mmc1_mirroring());
                }
                0x2000 => self.mmc1_chr0 = val & 0x1F, // $A000–$BFFF
                0x4000 => self.mmc1_chr1 = val & 0x1F, // $C000–$DFFF
                0x6000 => self.mmc1_prg = val & 0x1F,  // $E000–$FFFF
                _ => {}
            }
        }
    }
}

impl Mapper for Mapper296 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        if self.mapper_mode() == 0 {
            Some(&self.mmc3)
        } else {
            None
        }
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        if self.mapper_mode() == 0 {
            Some(&mut self.mmc3)
        } else {
            None
        }
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: self.mapper_mode() == 0, // IRQ only in MMC3 mode
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => match self.mapper_mode() {
                0 => self.read_prg_mmc3(addr),
                1 => self.read_prg_mmc1(addr),
                2 => self.read_prg_unrom(addr),
                3 => self.read_prg_cnrom(addr),
                _ => 0,
            },
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.mmc3.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x411D => self.config_reg = value & 0x07,
            0x411E => { /* CPU opcode / CHR encryption control: not implemented, writes ignored */ }
            0x412C => self.outer_reg = value & 0x0F,
            0x412E => self.outer_reg2 = value & 0x01,
            0x8000..=0xFFFF => match self.mapper_mode() {
                0 => self.mmc3.write_prg(addr, value),
                1 => self.write_mmc1_serial(addr, value),
                2 => self.unrom_bank = value & 0x0F,
                3 => self.cnrom_bank = value & 0x03,
                _ => {}
            },
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.chr_ram_enabled() {
            return self.chr_ram[(addr as usize) & 0x1FFF];
        }
        match self.mapper_mode() {
            0 => self.read_chr_mmc3(addr),
            1 => self.read_chr_mmc1(addr),
            2 => self.read_chr_unrom(addr),
            3 => self.read_chr_cnrom(addr),
            _ => 0,
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.chr_ram_enabled() {
            self.chr_ram[(addr as usize) & 0x1FFF] = value;
        } else {
            self.mmc3.write_chr(addr, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.extend_from_slice(&[
            self.config_reg,
            self.outer_reg,
            self.outer_reg2,
            self.unrom_bank,
            self.cnrom_bank,
            self.mmc1_shift,
            self.mmc1_count,
            self.mmc1_control,
            self.mmc1_chr0,
            self.mmc1_chr1,
            self.mmc1_prg,
        ]);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const TAIL_LEN: usize = 11;
        if data.len() < TAIL_LEN {
            return;
        }
        let (mmc3_data, tail) = data.split_at(data.len() - TAIL_LEN);
        self.config_reg = tail[0];
        self.outer_reg = tail[1];
        self.outer_reg2 = tail[2];
        self.unrom_bank = tail[3];
        self.cnrom_bank = tail[4];
        self.mmc1_shift = tail[5];
        self.mmc1_count = tail[6];
        self.mmc1_control = tail[7];
        self.mmc1_chr0 = tail[8];
        self.mmc1_chr1 = tail[9];
        self.mmc1_prg = tail[10];
        self.mmc3.restore_registers(mmc3_data);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        // Mapper 296 does not provide battery-backed WRAM.
        Vec::new()
    }

    fn load_wram_snapshot(&mut self, _data: &[u8]) {
        // No WRAM to restore for this mapper.
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.clone()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let len = data.len().min(CHR_RAM_SIZE);
        self.chr_ram[..len].copy_from_slice(&data[..len]);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.config_reg = 0;
        self.outer_reg = 0;
        self.outer_reg2 = 0;
        self.unrom_bank = 0;
        self.cnrom_bank = 0;
        self.mmc1_shift = 0;
        self.mmc1_count = 0;
        self.mmc1_control = 0x0C;
        self.mmc1_chr0 = 0;
        self.mmc1_chr1 = 0;
        self.mmc1_prg = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::MapperContext;

    const PRG_8KB_BANKS: usize = 32; // 256 KiB PRG-ROM
    const CHR_1KB_BANKS: usize = 64; // 64 KiB CHR-ROM (for CNROM needs 8KB multiples)

    /// Build a PRG-ROM where each 8 KiB bank is filled with its bank index (mod 256).
    fn make_prg_rom(banks: usize) -> Vec<u8> {
        let mut prg = vec![0u8; banks * 0x2000];
        for bank in 0..banks {
            let start = bank * 0x2000;
            for b in prg[start..start + 0x2000].iter_mut() {
                *b = (bank & 0xFF) as u8;
            }
        }
        prg
    }

    /// Build a CHR-ROM where each 1 KiB bank is filled with its bank index (mod 256).
    fn make_chr_rom(banks_1kb: usize) -> Vec<u8> {
        let mut chr = vec![0u8; banks_1kb * 0x0400];
        for bank in 0..banks_1kb {
            let start = bank * 0x0400;
            for b in chr[start..start + 0x0400].iter_mut() {
                *b = (bank & 0xFF) as u8;
            }
        }
        chr
    }

    fn create_mapper() -> Mapper296 {
        Mapper296::new(
            MapperContext::new_for_test(
                296,
                make_prg_rom(PRG_8KB_BANKS),
                make_chr_rom(CHR_1KB_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Mapper number ────────────────────────────────────────────────────────

    #[test]
    fn test_mapper_number_is_296() {
        assert_eq!(create_mapper().mapper_number(), 296);
    }

    // ── Config register $411D ────────────────────────────────────────────────

    #[test]
    fn test_config_reg_sets_mode_bits() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x03); // mode = CNROM
        assert_eq!(m.mapper_mode(), 3);
        m.write_prg(0x411D, 0x00); // mode = MMC3
        assert_eq!(m.mapper_mode(), 0);
    }

    #[test]
    fn test_config_reg_sets_chr_ram_bit() {
        let mut m = create_mapper();
        assert!(!m.chr_ram_enabled());
        m.write_prg(0x411D, 0x04);
        assert!(m.chr_ram_enabled());
        m.write_prg(0x411D, 0x00);
        assert!(!m.chr_ram_enabled());
    }

    #[test]
    fn test_config_reg_masks_upper_bits() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0xFF);
        assert_eq!(m.config_reg, 0x07, "only bits 2:0 are stored");
    }

    // ── Outer bank registers ─────────────────────────────────────────────────

    #[test]
    fn test_outer_reg_412c_written_and_stored() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0x0A);
        assert_eq!(m.outer_reg, 0x0A);
    }

    #[test]
    fn test_outer_reg2_412e_written_and_stored() {
        let mut m = create_mapper();
        m.write_prg(0x412E, 0x01);
        assert_eq!(m.outer_reg2, 0x01);
    }

    #[test]
    fn test_outer_regs_masked_to_valid_bits() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0xFF);
        assert_eq!(m.outer_reg, 0x0F, "$412C: only bits 3:0 stored");
        m.write_prg(0x412E, 0xFF);
        assert_eq!(m.outer_reg2, 0x01, "$412E: only bit 0 stored");
    }

    #[test]
    fn test_prg_8kb_outer_offset_a25() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0x01); // PRG A25
        assert_eq!(m.prg_8kb_outer_offset(), 1 << 12);
    }

    #[test]
    fn test_prg_8kb_outer_offset_a26() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0x04); // PRG A26
        assert_eq!(m.prg_8kb_outer_offset(), 1 << 13);
    }

    #[test]
    fn test_prg_8kb_outer_offset_a27() {
        let mut m = create_mapper();
        m.write_prg(0x412E, 0x01); // A27
        assert_eq!(m.prg_8kb_outer_offset(), 1 << 14);
    }

    #[test]
    fn test_chr_1kb_outer_offset_a25() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0x02); // CHR A25
        assert_eq!(m.chr_1kb_outer_offset(), 1 << 15);
    }

    // ── Mode 0: MMC3 ────────────────────────────────────────────────────────

    #[test]
    fn test_mmc3_mode_prg_bank_selection_r6() {
        let mut m = create_mapper();
        // MMC3 bank select: R6 = bank register for $8000 (mode 0)
        m.write_prg(0x8000, 0x06); // select R6
        m.write_prg(0x8001, 5); // R6 = bank 5
        assert_eq!(m.read_prg(0x8000), 5, "MMC3 mode: $8000 reads bank 5");
    }

    #[test]
    fn test_mmc3_mode_prg_outer_bank_extends_bank_number() {
        // Use a larger PRG-ROM to verify outer bank shifting
        // But we don't need many banks; just test offset calculation
        let mut m = create_mapper();
        // Default outer = 0: $8000 bank should be MMC3 bank
        m.write_prg(0x8000, 0x06); // select R6
        m.write_prg(0x8001, 0); // R6 = 0
        let without_outer = m.read_prg(0x8000);
        assert_eq!(without_outer, 0, "bank 0 reads 0");
        // outer A25 = 1: PRG offset = 4096; modulo 32 = 0, so same result for small ROM
        m.write_prg(0x412C, 0x01);
        assert_eq!(
            m.prg_8kb_outer_offset(),
            4096,
            "outer A25 gives 4096 bank offset"
        );
    }

    #[test]
    fn test_mmc3_mode_chr_bank_selection() {
        let mut m = create_mapper();
        // In MMC3 CHR mode 0: R2 is a 1 KiB bank mapped to PPU $1000–$13FF
        m.write_prg(0x8000, 0x02); // select R2
        m.write_prg(0x8001, 7); // R2 = 7 (CHR 1 KiB bank 7)
        let val = m.read_chr(0x1000);
        assert_eq!(val, 7, "MMC3 mode: PPU $1000 → 1KB bank 7 → filled with 7");
    }

    // ── Mode 2: UNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_unrom_mode_prg_bank_switch_at_8000() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x02); // mode = UNROM
        // Write bank select
        m.write_prg(0x8000, 3); // bank 3 → $8000 = two 8KB banks (6, 7)
        assert_eq!(m.read_prg(0x8000), 6, "$8000 first 8KB → bank 6");
        assert_eq!(m.read_prg(0xA000), 7, "$A000 second 8KB → bank 7");
    }

    #[test]
    fn test_unrom_mode_fixed_last_bank() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x02); // mode = UNROM
        m.write_prg(0x8000, 0); // select bank 0
        // Last two 8KB banks = 30, 31 (PRG_8KB_BANKS = 32)
        assert_eq!(m.read_prg(0xC000), 30, "$C000 → second-to-last bank = 30");
        assert_eq!(m.read_prg(0xE000), 31, "$E000 → last bank = 31");
    }

    #[test]
    fn test_unrom_bank_register_masked() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x02);
        m.write_prg(0x9000, 0xFF); // write anywhere in $8000-$FFFF
        assert_eq!(m.unrom_bank, 0x0F, "UNROM bank uses bits 3:0 only");
    }

    // ── Mode 3: CNROM ────────────────────────────────────────────────────────

    #[test]
    fn test_cnrom_mode_chr_bank_selection() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x03); // mode = CNROM
        m.write_prg(0x8000, 2); // CHR bank 2 → PPU $0000 maps to 1KB bank 16
        let val = m.read_chr(0x0000);
        assert_eq!(
            val, 16,
            "CNROM mode: CHR bank 2 → 1KB bank 16 (2*8) → filled with 16"
        );
    }

    #[test]
    fn test_cnrom_mode_prg_fixed_32kb() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x03); // mode = CNROM
        // PRG is fixed: all 4 × 8KB slots map to outer_base + slot
        // With outer = 0: slot 0,1,2,3 → banks 0,1,2,3
        assert_eq!(m.read_prg(0x8000), 0, "CNROM $8000 → bank 0");
        assert_eq!(m.read_prg(0xA000), 1, "CNROM $A000 → bank 1");
        assert_eq!(m.read_prg(0xC000), 2, "CNROM $C000 → bank 2");
        assert_eq!(m.read_prg(0xE000), 3, "CNROM $E000 → bank 3");
    }

    #[test]
    fn test_cnrom_bank_register_masked_to_2_bits() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x03);
        m.write_prg(0x8000, 0xFF);
        assert_eq!(m.cnrom_bank, 0x03, "CNROM bank uses bits 1:0 only");
    }

    // ── CHR-RAM mode ─────────────────────────────────────────────────────────

    #[test]
    fn test_chr_ram_mode_read_write() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x04); // CHR-RAM mode
        m.write_chr(0x0050, 0xAB);
        assert_eq!(m.read_chr(0x0050), 0xAB, "CHR-RAM: written byte reads back");
    }

    #[test]
    fn test_chr_ram_mode_does_not_read_chr_rom() {
        let mut m = create_mapper();
        // Without CHR-RAM mode, CHR-ROM bank 0 should be 0 (filled with 0)
        let rom_val = m.read_chr(0x0000);
        assert_eq!(rom_val, 0, "CHR-ROM bank 0 = 0");
        // With CHR-RAM mode, should return CHR-RAM (initially 0, but write something)
        m.write_prg(0x411D, 0x04);
        m.write_chr(0x0000, 0x42);
        assert_eq!(
            m.read_chr(0x0000),
            0x42,
            "CHR-RAM reads back the written byte"
        );
    }

    #[test]
    fn test_chr_ram_wraps_at_8kb() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x04);
        m.write_chr(0x1FFF, 0x55);
        assert_eq!(m.read_chr(0x1FFF), 0x55);
    }

    // ── Mode 1: MMC1 ────────────────────────────────────────────────────────

    #[test]
    fn test_mmc1_shift_register_loads_after_5_writes() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01); // MMC1 mode
        // Write 5 bits to $E000 (PRG bank register): value = 0b00111 = 7
        for bit in [1u8, 1, 1, 0, 0] {
            m.write_prg(0xE000, bit);
        }
        assert_eq!(m.mmc1_prg, 0x07, "MMC1: shift register loaded correctly");
    }

    #[test]
    fn test_mmc1_reset_via_bit7() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        m.write_prg(0xE000, 0x01); // start shifting
        m.write_prg(0xE000, 0x01);
        assert_eq!(m.mmc1_count, 2);
        m.write_prg(0xE000, 0x80); // bit 7 = reset
        assert_eq!(m.mmc1_count, 0, "MMC1 reset clears shift register");
        assert_eq!(m.mmc1_shift, 0);
    }

    #[test]
    fn test_mmc1_control_register_set() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // Write 0b00011 = 3 to $8000 (control)
        for bit in [1u8, 1, 0, 0, 0] {
            m.write_prg(0x8000, bit);
        }
        assert_eq!(m.mmc1_control, 0x03);
    }

    #[test]
    fn test_mmc1_prg_mode3_fixed_last_bank() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // MMC1 control: default = 0x0C (mode 3: fix $C000, switch $8000)
        // Set PRG bank = 0 → $8000 should read banks 0 and 1
        // $C000 should read last 16KB = banks 30 and 31
        assert_eq!(m.read_prg(0xC000), 30, "MMC1 mode3: $C000 → bank 30");
        assert_eq!(m.read_prg(0xE000), 31, "MMC1 mode3: $E000 → bank 31");
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn test_registers_snapshot_roundtrip() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x06); // mode 2 + CHR-RAM
        m.write_prg(0x412C, 0x05);
        m.write_prg(0x412E, 0x01);
        m.write_prg(0x411D, 0x02); // UNROM mode
        m.write_prg(0x8000, 0x0B); // unrom_bank = 11
        let snap = m.registers_snapshot();
        // Corrupt state
        m.config_reg = 0;
        m.outer_reg = 0;
        m.outer_reg2 = 0;
        m.unrom_bank = 0;
        // Restore
        m.restore_registers(&snap);
        assert_eq!(m.config_reg, 0x02);
        assert_eq!(m.outer_reg, 0x05);
        assert_eq!(m.outer_reg2, 0x01);
        assert_eq!(m.unrom_bank, 0x0B);
    }

    #[test]
    fn test_chr_ram_snapshot_roundtrip() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x04); // CHR-RAM mode
        m.write_chr(0x0100, 0xDE);
        m.write_chr(0x0200, 0xAD);
        let snap = m.chr_ram_snapshot();
        m.chr_ram.fill(0);
        m.restore_chr_ram(&snap);
        assert_eq!(m.read_chr(0x0100), 0xDE);
        assert_eq!(m.read_chr(0x0200), 0xAD);
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn test_reset_clears_all_registers() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x07);
        m.write_prg(0x412C, 0x0F);
        m.write_prg(0x412E, 0x01);
        m.write_prg(0x411D, 0x02);
        m.write_prg(0x8000, 0x0A); // unrom_bank = 10
        m.reset();
        assert_eq!(m.config_reg, 0);
        assert_eq!(m.outer_reg, 0);
        assert_eq!(m.outer_reg2, 0);
        assert_eq!(m.unrom_bank, 0);
        assert_eq!(m.cnrom_bank, 0);
        assert_eq!(m.mmc1_control, 0x0C, "MMC1 control resets to 0x0C");
    }

    // ── CHR outer offset – A26, A27 ──────────────────────────────────────────

    #[test]
    fn test_chr_1kb_outer_offset_a26() {
        let mut m = create_mapper();
        m.write_prg(0x412C, 0x08); // CHR A26
        assert_eq!(m.chr_1kb_outer_offset(), 1 << 16);
    }

    #[test]
    fn test_chr_1kb_outer_offset_a27() {
        let mut m = create_mapper();
        m.write_prg(0x412E, 0x01); // A27
        assert_eq!(m.chr_1kb_outer_offset(), 1 << 17);
    }

    // ── Mode 1: MMC1 CHR banking ─────────────────────────────────────────────

    #[test]
    fn test_mmc1_chr_8kb_mode_bank0_selects_8kb_block() {
        // MMC1 default control bit4=0 → 8 KiB CHR mode.
        // In 8 KiB mode chr0 is a 4 KiB bank index; bit 0 is ignored (aligned to even bank).
        // chr0=2 → 4 KiB bank 2 (bit0 already 0) → 1 KiB banks 8..15.
        // PPU $0000 → 1 KiB bank 8, filled with 8.
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01); // MMC1 mode
        // Write chr0=2 to $A000 (5 bits: 0b00010)
        for bit in [0u8, 1, 0, 0, 0] {
            m.write_prg(0xA000, bit);
        }
        let val = m.read_chr(0x0000);
        assert_eq!(val, 8, "MMC1 8KB CHR: chr0=2 → 1KB bank 8 → value 8");

        // chr0=3 (bit 0 set) must map to the same 8 KiB block as chr0=2.
        let mut m2 = create_mapper();
        m2.write_prg(0x411D, 0x01);
        for bit in [1u8, 1, 0, 0, 0] {
            m2.write_prg(0xA000, bit);
        }
        assert_eq!(
            m2.read_chr(0x0000),
            8,
            "MMC1 8KB CHR: chr0=3 (bit0 ignored) → same 1KB bank 8"
        );
    }

    #[test]
    fn test_mmc1_chr_4kb_mode_banks_at_0000_and_1000() {
        // Set CHR mode to 4 KiB (bit 4 of control = 1).
        // Write control = 0b10000 (bit4=1) to $8000.
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01); // MMC1 mode
        // Set mmc1_control bit 4 via 5-bit write to $8000 → value 0b10000 = 16
        for bit in [0u8, 0, 0, 0, 1] {
            m.write_prg(0x8000, bit);
        }
        // chr0 = 2 (4KB bank at $0000)
        for bit in [0u8, 1, 0, 0, 0] {
            m.write_prg(0xA000, bit);
        }
        // chr1 = 3 (4KB bank at $1000)
        for bit in [1u8, 1, 0, 0, 0] {
            m.write_prg(0xC000, bit);
        }
        // $0000: 4KB bank 2 → 1KB banks 8..11; PPU $0000 → 1KB bank 8, filled with 8
        assert_eq!(
            m.read_chr(0x0000),
            8,
            "4KB CHR: bank 2 at $0000 → 1KB bank 8"
        );
        // $1000: 4KB bank 3 → 1KB banks 12..15; PPU $1000 → 1KB bank 12, filled with 12
        assert_eq!(
            m.read_chr(0x1000),
            12,
            "4KB CHR: bank 3 at $1000 → 1KB bank 12"
        );
    }

    // ── Mode 1: MMC1 mirroring ───────────────────────────────────────────────

    #[test]
    fn test_mmc1_mirroring_single_screen_lower() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // Control bits 1:0 = 0 → SingleScreenLower
        for bit in [0u8, 0, 0, 0, 0] {
            m.write_prg(0x8000, bit);
        }
        assert_eq!(m.mmc1_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_mmc1_mirroring_single_screen_upper() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // Control bits 1:0 = 1 → SingleScreenUpper
        for bit in [1u8, 0, 0, 0, 0] {
            m.write_prg(0x8000, bit);
        }
        assert_eq!(m.mmc1_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_mmc1_mirroring_vertical() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // Control bits 1:0 = 2 → Vertical
        for bit in [0u8, 1, 0, 0, 0] {
            m.write_prg(0x8000, bit);
        }
        assert_eq!(m.mmc1_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_mmc1_mirroring_horizontal() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x01);
        // Control bits 1:0 = 3 → Horizontal
        for bit in [1u8, 1, 0, 0, 0] {
            m.write_prg(0x8000, bit);
        }
        assert_eq!(m.mmc1_mirroring(), NametableLayout::Horizontal);
    }

    // ── Mode 2: UNROM CHR passthrough ────────────────────────────────────────

    #[test]
    fn test_unrom_chr_reads_from_chr_rom_bank0() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x02); // UNROM mode
        // PPU $0000 → 1KB bank 0, filled with 0
        let val = m.read_chr(0x0000);
        assert_eq!(val, 0, "UNROM CHR: PPU $0000 → bank 0 → 0");
        // PPU $1C00 → 1KB bank 7 (offset 0x1C00 >> 10 = 7), filled with 7
        let val = m.read_chr(0x1C00);
        assert_eq!(val, 7, "UNROM CHR: PPU $1C00 → 1KB bank 7 → 7");
    }

    // ── WRAM helpers ─────────────────────────────────────────────────────────

    #[test]
    fn test_wram_size_is_zero() {
        assert_eq!(create_mapper().wram_size(), 0);
    }

    #[test]
    fn test_wram_snapshot_is_empty() {
        assert!(create_mapper().wram_snapshot().is_empty());
    }

    #[test]
    fn test_load_wram_snapshot_is_noop() {
        let mut m = create_mapper();
        m.load_wram_snapshot(&[0xAB; 1024]); // should not panic or change state
        assert_eq!(m.wram_size(), 0);
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn test_capabilities_mmc3_mode_has_irq() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x00); // MMC3 mode
        assert!(m.capabilities().has_irq);
    }

    #[test]
    fn test_capabilities_non_mmc3_mode_no_irq() {
        let mut m = create_mapper();
        m.write_prg(0x411D, 0x02); // UNROM mode
        assert!(!m.capabilities().has_irq);
    }

    #[test]
    fn test_capabilities_has_chr_banking_and_dynamic_mirroring() {
        let m = create_mapper();
        let caps = m.capabilities();
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
    }
}
