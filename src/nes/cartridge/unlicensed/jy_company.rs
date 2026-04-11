//! Mapper 090 / 209 – J.Y. Company ASIC
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/J.Y._Company_ASIC>
//! - Fallback: <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/JyCompany/JyCompany.h>
//!
//! The J.Y. Company ASIC is used for their later single-game cartridges and multicarts.
//! It provides flexible PRG/CHR banking, four-way mirroring, a prescaler IRQ, and a
//! hardware multiplier.
//!
//! **Mapper 209** is the standard implementation. ROM nametables and Extended Mirroring
//! are controlled via the $D001 register.
//!
//! **Mapper 90** inhibits ROM nametables and Extended Mirroring via a PCB jumper; $D001
//! bits 2–6 are permanently forced to zero regardless of what is written.
//!
//! Other related mappers:
//! - Mapper 160: shares mapper-90 hardware (inhibited extended mirroring).
//! - Mapper 211: shares mapper-209 behavior (standard).
//!
//! Known Limitations:
//! - ROM nametable and Extended Mirroring features ($D001 bits 2–6, $B000–$B007) are
//!   not yet implemented for mapper 209; those bits are accepted but currently ignored.
//!   (Standard mirroring via bits 0–1 is always active.)
//! - MMC4-like automatic CHR bankswitching ($D003 bit 7) is not implemented.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::common::A12RisingEdgeDetector;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

#[derive(Clone, Copy, PartialEq)]
enum IrqSource {
    CpuClock = 0,
    PpuA12Rise = 1,
    PpuRead = 2,
    CpuWrite = 3,
}

impl IrqSource {
    fn from_u8(v: u8) -> Self {
        match v & 0x03 {
            0 => Self::CpuClock,
            1 => Self::PpuA12Rise,
            2 => Self::PpuRead,
            _ => Self::CpuWrite,
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Mapper 090 / 209 – J.Y. Company ASIC
pub struct JyCompanyMapper {
    base: BaseMapper,

    // PRG state
    prg_regs: [u8; 4],
    prg_mode: u8,       // bits 0-1 from $D000
    last_bank_sw: bool, // $D000 bit 2: true = $8003 switchable
    prg_at_6000: bool,  // $D000 bit 7

    // CHR state
    chr_low: [u8; 8],  // $9000-$9007 LSB
    chr_high: [u8; 8], // $A000-$A007 MSB

    // CHR mode from $D000 bits 3-4
    chr_mode: u8,

    // Mirroring
    /// $D001 raw value. For mapper 90 only bits 0-1 are active; for mapper 209 all bits
    /// are stored (extended mirroring, though not yet acted upon beyond bits 0-1).
    mirror_reg: u8,

    /// When `true` (mapper 90), $D001 bits 2-6 are permanently inhibited by PCB jumper.
    /// When `false` (mapper 209/211), the full register value is stored.
    inhibit_extended: bool,

    // IRQ
    irq_enabled: bool,
    irq_pending: bool,
    irq_source: IrqSource,
    irq_count_dir: u8,         // bits 6-7 of $C001 (1=up, 2=down, 0/3=disabled)
    irq_small_prescaler: bool, // $C001 bit 2
    irq_prescaler: u8,
    irq_counter: u8,
    irq_xor: u8,

    // Hardware multiplier
    mul_a: u8,
    mul_b: u8,

    // A12 detector for PPU-A12 IRQ source
    a12: A12RisingEdgeDetector,
}

impl JyCompanyMapper {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;

    /// Constructor for mapper 90 / 160 — extended mirroring inhibited by PCB jumper.
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self::new_with_flags(ctx, true)
    }

    /// Constructor for mapper 209 / 211 — standard J.Y. Company ASIC, extended mirroring
    /// register bits are accepted (though currently only bits 0-1 are acted upon).
    pub fn new_standard(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self::new_with_flags(ctx, false)
    }

    fn new_with_flags(
        ctx: crate::nes::cartridge::mapper::MapperContext,
        inhibit_extended: bool,
    ) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(NametableLayout::Vertical);

        let mut m = Self {
            base,
            prg_regs: [0; 4],
            prg_mode: 0,
            last_bank_sw: false,
            prg_at_6000: false,
            chr_low: [0; 8],
            chr_high: [0; 8],
            chr_mode: 0,
            mirror_reg: 0,
            inhibit_extended,
            irq_enabled: false,
            irq_pending: false,
            irq_source: IrqSource::CpuClock,
            irq_count_dir: 0,
            irq_small_prescaler: false,
            irq_prescaler: 0,
            irq_counter: 0,
            irq_xor: 0,
            mul_a: 0,
            mul_b: 0,
            a12: A12RisingEdgeDetector::new(3),
        };
        m.update_banks();
        m
    }

    fn chr_bank(&self, index: usize) -> i16 {
        let lo = self.chr_low[index] as u16;
        let hi = self.chr_high[index] as u16;
        ((hi << 8) | lo) as i16
    }

    /// Returns the PRG register at `index` as an 8 KiB page index.
    ///
    /// In mode 3 (inverted), the 7-bit value has its bits reversed before use.
    fn prg_reg_as_page(&self, index: usize) -> i16 {
        let r = (self.prg_regs[index] & 0x7F) as u16;
        if (self.prg_mode & 0x03) == 3 {
            let reversed = ((r & 0x01) << 6)
                | ((r & 0x02) << 3)
                | (r & 0x04)
                | ((r & 0x08) >> 3)
                | ((r & 0x10) >> 2)
                | ((r & 0x20) >> 4)
                | ((r & 0x40) >> 6);
            reversed as i16
        } else {
            r as i16
        }
    }

    fn update_prg(&mut self) {
        match self.prg_mode & 0x03 {
            0 => {
                // 32KB mode: $8003 selects 8KB-page base of the 32KB block.
                // Low 2 bits are ignored by convention (programmer writes 4-aligned value).
                let b = if self.last_bank_sw {
                    self.prg_reg_as_page(3)
                } else {
                    -4i16
                };
                for i in 0..4 {
                    self.base.select_prg_page(i, b + i as i16);
                }
            }
            1 => {
                // 16KB mode: $8001 holds a 16KB bank number → shift by 1 for 8KB pages.
                let lo = self.prg_reg_as_page(1) << 1;
                let hi = if self.last_bank_sw {
                    self.prg_reg_as_page(3) << 1
                } else {
                    -2i16
                };
                self.base.select_prg_page(0, lo);
                self.base.select_prg_page(1, lo + 1);
                self.base.select_prg_page(2, hi);
                self.base.select_prg_page(3, hi + 1);
            }
            _ => {
                // 8KB mode (modes 2 and 3): regs are direct 8KB page indices.
                for i in 0..3 {
                    let page = self.prg_reg_as_page(i);
                    self.base.select_prg_page(i, page);
                }
                let last = if self.last_bank_sw {
                    self.prg_reg_as_page(3)
                } else {
                    -1i16
                };
                self.base.select_prg_page(3, last);
            }
        }
    }

    fn update_chr(&mut self) {
        match self.chr_mode {
            0 => {
                // 8KB mode: chr_bank(0) is an 8KB bank number → shift by 3 for 1KB pages.
                let b = self.chr_bank(0) << 3;
                for i in 0..8 {
                    self.base.select_chr_page(i, b + i as i16);
                }
            }
            1 => {
                // 4KB mode: chr_bank(0) and chr_bank(4) are 4KB bank numbers → shift by 2.
                let b0 = self.chr_bank(0) << 2;
                let b1 = self.chr_bank(4) << 2;
                for i in 0..4 {
                    self.base.select_chr_page(i, b0 + i as i16);
                    self.base.select_chr_page(4 + i, b1 + i as i16);
                }
            }
            2 => {
                // 2KB mode: chr_bank(0,2,4,6) are 2KB bank numbers → shift by 1.
                for slot in 0..4 {
                    let b = self.chr_bank(slot * 2) << 1;
                    self.base.select_chr_page(slot * 2, b);
                    self.base.select_chr_page(slot * 2 + 1, b + 1);
                }
            }
            _ => {
                // 1KB mode: all 8 registers are 1KB bank numbers (direct).
                for i in 0..8 {
                    self.base.select_chr_page(i, self.chr_bank(i));
                }
            }
        }
    }

    fn update_mirroring(&mut self) {
        // Bits 0-1 control standard 2-way mirroring in all J.Y. Company variants.
        // Bits 2-6 (extended per-nametable control) are currently only stored for
        // mapper 209/211 but not yet acted upon (Known Limitation).
        let layout = match self.mirror_reg & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            2 => NametableLayout::SingleScreen,
            _ => NametableLayout::SingleScreenUpper,
        };
        self.base.set_mirroring(layout);
    }

    fn update_banks(&mut self) {
        self.update_prg();
        self.update_chr();
    }

    fn tick_irq(&mut self) {
        if self.irq_count_dir == 0 || self.irq_count_dir == 3 {
            return; // counting disabled
        }
        let mask: u8 = if self.irq_small_prescaler { 0x07 } else { 0xFF };
        let mut prescaler = self.irq_prescaler & mask;
        let mut clocked = false;
        if self.irq_count_dir == 1 {
            prescaler = prescaler.wrapping_add(1);
            if (prescaler & mask) == 0 {
                clocked = true;
            }
        } else {
            // direction == 2 (decrease)
            prescaler = prescaler.wrapping_sub(1);
            if prescaler == 0 {
                clocked = true;
            }
        }
        self.irq_prescaler = (self.irq_prescaler & !mask) | (prescaler & mask);
        if clocked {
            if self.irq_count_dir == 1 {
                self.irq_counter = self.irq_counter.wrapping_add(1);
                if self.irq_counter == 0 && self.irq_enabled {
                    self.irq_pending = true;
                }
            } else {
                self.irq_counter = self.irq_counter.wrapping_sub(1);
                if self.irq_counter == 0xFF && self.irq_enabled {
                    self.irq_pending = true;
                }
            }
        }
    }
}

impl Mapper for JyCompanyMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5000..=0x5FFF => match addr & 0xF803 {
                0x5000 => 0x00,
                0x5800 => ((self.mul_a as u16) * (self.mul_b as u16)) as u8,
                0x5801 => (((self.mul_a as u16) * (self.mul_b as u16)) >> 8) as u8,
                _ => open_bus,
            },
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            match addr & 0xF803 {
                0x5800 => self.mul_a = value,
                0x5801 => self.mul_b = value,
                _ => {}
            }
            return;
        }
        match addr & 0xF007 {
            0x8000..=0x8003 => {
                self.prg_regs[(addr & 0x03) as usize] = value & 0x7F;
                self.update_prg();
            }
            0x9000..=0x9007 => {
                self.chr_low[(addr & 0x07) as usize] = value;
                self.update_chr();
            }
            0xA000..=0xA007 => {
                self.chr_high[(addr & 0x07) as usize] = value;
                self.update_chr();
            }
            0xC000 => {
                if (value & 0x01) != 0 {
                    self.irq_enabled = true;
                } else {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                }
            }
            0xC001 => {
                self.irq_count_dir = (value >> 6) & 0x03;
                self.irq_small_prescaler = (value & 0x04) != 0;
                self.irq_source = IrqSource::from_u8(value);
            }
            0xC002 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0xC003 => {
                self.irq_enabled = true;
            }
            0xC004 => {
                self.irq_prescaler = value ^ self.irq_xor;
            }
            0xC005 => {
                self.irq_counter = value ^ self.irq_xor;
            }
            0xC006 => {
                self.irq_xor = value;
            }
            0xD000 => {
                self.prg_mode = value & 0x03;
                self.last_bank_sw = (value & 0x04) != 0;
                self.chr_mode = (value >> 3) & 0x03;
                // bits 5 and 6 (ROM nametables) accepted but not yet acted upon for mapper 209
                self.update_banks();
            }
            0xD001 => {
                // Mapper 90/160 (inhibit_extended=true): only bits 0-1 are active; the PCB
                // jumper forces bits 2-6 to zero so they are masked here as well.
                // Mapper 209/211 (inhibit_extended=false): store the full byte; extended
                // mirroring bits are accepted though currently only bits 0-1 are acted upon.
                self.mirror_reg = if self.inhibit_extended {
                    value & 0x03
                } else {
                    value
                };
                self.update_mirroring();
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        // Always update the A12 edge detector for correct filtering,
        // regardless of which IRQ source is selected.
        let a12_rose = self.a12.update(addr);

        if self.irq_source == IrqSource::PpuA12Rise && a12_rose {
            // IRQ source 1: tick on PPU A12 rising edge
            self.tick_irq();
        } else if self.irq_source == IrqSource::PpuRead {
            // IRQ source 2: tick on PPU activity (approximated here by address changes)
            self.tick_irq();
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12.cpu_tick();
        if self.irq_source == IrqSource::CpuClock || self.irq_source == IrqSource::CpuWrite {
            self.tick_irq();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(32);
        v.push(self.prg_mode | (self.last_bank_sw as u8) << 2 | (self.prg_at_6000 as u8) << 7);
        v.push(self.chr_mode);
        v.push(self.mirror_reg);
        v.push(self.irq_enabled as u8 | ((self.irq_pending as u8) << 1));
        v.push(
            self.irq_count_dir << 6
                | (self.irq_small_prescaler as u8) << 2
                | self.irq_source.to_u8(),
        );
        v.push(self.irq_prescaler);
        v.push(self.irq_counter);
        v.push(self.irq_xor);
        v.push(self.mul_a);
        v.push(self.mul_b);
        v.extend_from_slice(&self.prg_regs);
        v.extend_from_slice(&self.chr_low);
        v.extend_from_slice(&self.chr_high);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 30 {
            return;
        }
        let d0 = data[0];
        self.prg_mode = d0 & 0x03;
        self.last_bank_sw = (d0 & 0x04) != 0;
        self.prg_at_6000 = (d0 & 0x80) != 0;
        self.chr_mode = data[1];
        self.mirror_reg = data[2];
        let flags = data[3];
        self.irq_enabled = (flags & 1) != 0;
        self.irq_pending = (flags & 2) != 0;
        let c1 = data[4];
        self.irq_count_dir = (c1 >> 6) & 0x03;
        self.irq_small_prescaler = (c1 & 0x04) != 0;
        self.irq_source = IrqSource::from_u8(c1);
        self.irq_prescaler = data[5];
        self.irq_counter = data[6];
        self.irq_xor = data[7];
        self.mul_a = data[8];
        self.mul_b = data[9];
        self.prg_regs.copy_from_slice(&data[10..14]);
        self.chr_low.copy_from_slice(&data[14..22]);
        self.chr_high.copy_from_slice(&data[22..30]);
        self.update_banks();
        self.update_mirroring();
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 4];
        self.chr_low = [0; 8];
        self.chr_high = [0; 8];
        self.prg_mode = 0;
        self.last_bank_sw = false;
        self.prg_at_6000 = false;
        self.chr_mode = 0;
        self.mirror_reg = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_source = IrqSource::CpuClock;
        self.irq_count_dir = 0;
        self.irq_small_prescaler = false;
        self.irq_prescaler = 0;
        self.irq_counter = 0;
        self.irq_xor = 0;
        self.mul_a = 0;
        self.mul_b = 0;
        self.a12 = A12RisingEdgeDetector::new(3);
        self.base.set_mirroring(NametableLayout::Vertical);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use non-power-of-two to catch modulo-wrap false-passes
    const PRG_8K_BANKS: usize = 11;
    const CHR_1K_BANKS: usize = 13;

    fn make_mapper() -> JyCompanyMapper {
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        JyCompanyMapper::new(MapperContext::new_for_test(
            90,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_90_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            90,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 90 must be registered in the factory"
        );
    }

    // ── PRG banking – 8KB mode (default) ────────────────────────────

    #[test]
    fn prg_window3_fixed_to_last_bank_at_poweron() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "Window 3 ($E000) must be fixed to last bank at power-on"
        );
    }

    #[test]
    fn prg_8k_write_8000_selects_window0() {
        let mut mapper = make_mapper();
        // force 8KB mode
        mapper.write_prg(0xD000, 0x02); // bits 0-1 = 2 (8KB mode)
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 write must select PRG window 0 in 8KB mode"
        );
    }

    #[test]
    fn prg_8k_write_8001_selects_window1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x02);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_prg(0xA000), 5, "$8001 selects window 1");
    }

    #[test]
    fn prg_8k_write_8002_selects_window2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x02);
        mapper.write_prg(0x8002, 7);
        assert_eq!(mapper.read_prg(0xC000), 7, "$8002 selects window 2");
    }

    #[test]
    fn prg_8k_last_bank_fixed_when_bit2_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x02); // 8KB mode, bit2=0 (fixed)
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "Window 3 must remain fixed to last bank when $D000 bit 2 is clear"
        );
    }

    #[test]
    fn prg_8k_last_bank_switchable_when_bit2_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x06); // 8KB mode, bit2=1 (switchable)
        mapper.write_prg(0x8003, 3);
        assert_eq!(
            mapper.read_prg(0xE000),
            3,
            "Window 3 must be switchable when $D000 bit 2 is set"
        );
    }

    // ── PRG banking – 16KB mode ─────────────────────────────────────

    #[test]
    fn prg_16k_8001_selects_lower_half() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x01); // 16KB mode, last bank fixed
        mapper.write_prg(0x8001, 2); // selects 16KB bank 2 → 8KB pages 4+5
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "16KB mode: low page of window 1 reg"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "16KB mode: high page of window 1 reg"
        );
    }

    #[test]
    fn prg_16k_last_half_fixed_when_bit2_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x01); // 16KB mode, bit2=0 (fixed)
        let total = PRG_8K_BANKS as u8;
        // Last two 8KB banks are PRG_8K_BANKS-2 and PRG_8K_BANKS-1
        assert_eq!(
            mapper.read_prg(0xC000),
            total - 2,
            "Fixed upper 16KB low page"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            total - 1,
            "Fixed upper 16KB high page"
        );
    }

    // ── PRG banking – 32KB mode ─────────────────────────────────────

    #[test]
    fn prg_32k_8003_selects_all_when_bit2_set() {
        let mut mapper = make_mapper();
        // 32KB mode, bit2=1 (switchable): $8003 = 4 (8KB page base) → pages 4,5,6,7
        mapper.write_prg(0xD000, 0x04); // 32KB mode, last_bank_sw=1
        mapper.write_prg(0x8003, 4); // 8KB page base 4
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    // ── CHR banking – 1KB mode (default power-on) ───────────────────

    #[test]
    fn chr_1k_write_9000_selects_window0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3 (1KB)
        mapper.write_prg(0x9000, 5);
        assert_eq!(mapper.read_chr(0x0000), 5 % CHR_1K_BANKS as u8);
    }

    #[test]
    fn chr_1k_write_9007_selects_window7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3
        mapper.write_prg(0x9007, 9);
        assert_eq!(
            mapper.read_chr(0x1C00),
            9 % CHR_1K_BANKS as u8,
            "$9007 must select 1KB window 7"
        );
    }

    #[test]
    fn chr_1k_msb_a000_extends_bank_number() {
        // Use upper-marker data so bank 256 (marker=1) differs from bank 0 (marker=0)
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = crate::nes::cartridge::test_helpers::banked_data_with_upper_marker(1024, 300);
        let mut mapper = JyCompanyMapper::new(MapperContext::new_for_test(
            90,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        mapper.write_prg(0xD000, 0x18); // 1KB mode
        mapper.write_prg(0x9000, 0x00); // LSB = 0
        mapper.write_prg(0xA000, 0x01); // MSB = 1 → bank 256
        // Bank 256 marker = 256 >> 8 = 1; bank 0 marker = 0 >> 8 = 0
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "MSB register must extend CHR bank number beyond 255"
        );
    }

    // ── CHR banking – 2KB mode ───────────────────────────────────────

    #[test]
    fn chr_2k_mode_maps_four_2kb_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x10); // chr_mode=2 (2KB)
        // indices 0, 2, 4, 6
        mapper.write_prg(0x9000, 4); // 2KB bank 4 → 1KB pages 8+9
        assert_eq!(mapper.read_chr(0x0000), 8 % CHR_1K_BANKS as u8);
        assert_eq!(mapper.read_chr(0x0400), 9 % CHR_1K_BANKS as u8);
    }

    // ── CHR banking – 4KB mode ───────────────────────────────────────

    #[test]
    fn chr_4k_mode_maps_two_4kb_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x08); // chr_mode=1 (4KB)
        mapper.write_prg(0x9000, 2); // 4KB bank 2 → 1KB pages 8-11
        assert_eq!(mapper.read_chr(0x0000), 8 % CHR_1K_BANKS as u8);
        assert_eq!(mapper.read_chr(0x0C00), 11 % CHR_1K_BANKS as u8);
    }

    // ── CHR banking – 8KB mode ───────────────────────────────────────

    #[test]
    fn chr_8k_mode_maps_single_8kb_window() {
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = banked_data(1024, 16); // 16 1KB banks
        let mut mapper = JyCompanyMapper::new(MapperContext::new_for_test(
            90,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        mapper.write_prg(0xD000, 0x00); // chr_mode=0 (8KB), prg_mode=0
        // 8KB bank 1 → 1KB pages 8-15 (shift by 3)
        mapper.write_prg(0x9000, 1);
        for i in 0u16..8 {
            assert_eq!(
                mapper.read_chr(i * 0x400),
                (8 + i) as u8,
                "8KB mode: CHR page {i} should be {}",
                8 + i
            );
        }
    }

    // ── Mirroring ────────────────────────────────────────────────────

    #[test]
    fn mirroring_default_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_0_is_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_1_is_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_d001_2_is_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreen);
    }

    #[test]
    fn mirroring_d001_3_is_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    // ── IRQ ──────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_poweron() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_c003_enables_irq() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC003, 0);
        assert!(mapper.irq_enabled);
    }

    #[test]
    fn irq_c002_disables_and_acks() {
        let mut mapper = make_mapper();
        mapper.irq_enabled = true;
        mapper.irq_pending = true;
        mapper.write_prg(0xC002, 0);
        assert!(!mapper.irq_enabled);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_c000_bit0_enables() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x01);
        assert!(mapper.irq_enabled);
    }

    #[test]
    fn irq_c000_bit0_clear_disables_and_acks() {
        let mut mapper = make_mapper();
        mapper.irq_enabled = true;
        mapper.irq_pending = true;
        mapper.write_prg(0xC000, 0x00);
        assert!(!mapper.irq_enabled);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_fires_on_cpu_clock_prescaler_overflow_count_up() {
        let mut mapper = make_mapper();
        // count_dir=1 (up), small_prescaler=true (mask=$07), cpu clock source
        mapper.write_prg(0xC001, 0x44); // dir=1, small_prescaler=1, source=cpu
        mapper.write_prg(0xC004, 0xFF); // prescaler = 0xFF ^ 0 = 0xFF
        mapper.write_prg(0xC005, 0xFE); // counter = 0xFE ^ 0 = 0xFE → needs 2 prescaler overflows
        mapper.write_prg(0xC003, 0); // enable

        // With small prescaler (mask $07), prescaler starts at 0xFF & 0x07 = 7.
        // Each overflow: prescaler 7 → 0 → counts as wrap. Need counter to reach 0.
        // counter starts 0xFE → needs 2 increments (overflows) to reach 0x00 and fire.
        // Each overflow takes 1 cpu tick (prescaler goes from 7 to 0 in 1 step is wrong)
        // Actually prescaler = 7, step +1 each tick, wraps 0→ overflow when (prescaler+1)&0x07==0.
        // Starting prescaler = 7, after 1 tick = (7+1)&0x07=0 → overflow! counter++
        // counter = 0xFF → after next overflow: 0x00 → FIRE
        // So: need 2 overflows = 2 cpu ticks each (prescaler starts at 7, each tick wraps)
        let mut fired = false;
        for _ in 0..256 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(
            fired,
            "IRQ must fire after prescaler overflows cause counter wrap"
        );
    }

    #[test]
    fn irq_fires_on_a12_rise_when_configured() {
        let mut mapper = make_mapper();
        // source=PpuA12Rise, dir=up, small_prescaler=true
        mapper.write_prg(0xC001, 0x45); // dir=1, small_prescaler=1, source=A12
        mapper.write_prg(0xC004, 0xFF); // prescaler starts at 7
        mapper.write_prg(0xC005, 0xFE); // counter = 0xFE
        mapper.write_prg(0xC003, 0); // enable

        let mut fired = false;
        for _ in 0..256 {
            // Simulate A12 rising edge: low → tick → high
            mapper.ppu_address_changed(0x0000);
            for _ in 0..4 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(
            fired,
            "IRQ must fire on A12 rising edges when source=PpuA12Rise"
        );
    }

    #[test]
    fn irq_does_not_fire_when_disabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC001, 0x44); // up, cpu
        mapper.write_prg(0xC004, 0xFF);
        mapper.write_prg(0xC005, 0x00); // counter already at 0
        // Do NOT enable
        for _ in 0..1000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when not enabled");
    }

    #[test]
    fn irq_xor_register_applied_to_prescaler_and_counter() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC006, 0xAA); // set XOR
        mapper.write_prg(0xC004, 0xAA); // prescaler = 0xAA ^ 0xAA = 0x00
        mapper.write_prg(0xC005, 0xAB); // counter = 0xAB ^ 0xAA = 0x01
        assert_eq!(mapper.irq_prescaler, 0x00);
        assert_eq!(mapper.irq_counter, 0x01);
    }

    // ── Hardware multiplier ──────────────────────────────────────────

    #[test]
    fn multiplier_5800_5801_read_product() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5800, 12);
        mapper.write_prg(0x5801, 25);
        let lo = mapper.read_prg_open_bus(0x5800, 0x00);
        let hi = mapper.read_prg_open_bus(0x5801, 0x00);
        let result = (hi as u16) << 8 | lo as u16;
        assert_eq!(
            result,
            12 * 25,
            "5800/5801 must return 16-bit product of operands"
        );
    }

    #[test]
    fn multiplier_handles_overflow_correctly() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5800, 0xFF);
        mapper.write_prg(0x5801, 0xFF);
        let lo = mapper.read_prg_open_bus(0x5800, 0x00);
        let hi = mapper.read_prg_open_bus(0x5801, 0x00);
        let result = (hi as u16) << 8 | lo as u16;
        assert_eq!(result, 0xFE01, "255 * 255 = 65025 = 0xFE01");
    }

    // ── Snapshot / restore ───────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut a = make_mapper();
        a.write_prg(0xD000, 0x1A); // chr_mode=3, prg_mode=2, last_bank_sw=1
        a.write_prg(0x8000, 2);
        a.write_prg(0x8001, 4);
        a.write_prg(0x8002, 6);
        a.write_prg(0x9000, 3);
        a.write_prg(0x9001, 5);
        a.write_prg(0xD001, 0x01); // horizontal
        a.write_prg(0xC006, 0x11);
        a.write_prg(0xC005, 0x07);
        a.write_prg(0xC003, 0); // enable IRQ

        let snap = a.registers_snapshot();
        let mut b = make_mapper();
        b.restore_registers(&snap);

        assert_eq!(b.read_prg(0x8000), a.read_prg(0x8000));
        assert_eq!(b.read_prg(0xA000), a.read_prg(0xA000));
        assert_eq!(b.read_chr(0x0000), a.read_chr(0x0000));
        assert_eq!(b.get_mirroring(), a.get_mirroring());
        assert_eq!(b.irq_enabled, a.irq_enabled);
        assert_eq!(b.irq_counter, a.irq_counter);
    }

    // ── Reset ────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x1A);
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xD001, 0x01);
        mapper.reset();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0xE000), (PRG_8K_BANKS - 1) as u8);
    }

    // ── Mapper 90 extended-mirroring inhibit ─────────────────────────

    #[test]
    fn mapper_90_d001_upper_bits_are_masked() {
        let mut mapper = make_mapper(); // mapper 90, inhibit_extended=true
        // Write a value with bits 2-6 set; those bits should be masked to zero.
        mapper.write_prg(0xD001, 0xFF);
        assert_eq!(
            mapper.mirror_reg & !0x03,
            0,
            "Mapper 90 must mask $D001 bits 2-6 to zero"
        );
    }

    // ── Mapper 209 – registration and standard behaviour ─────────────

    fn make_mapper_209() -> JyCompanyMapper {
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        JyCompanyMapper::new_standard(MapperContext::new_for_test(
            209,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_209_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            209,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 209 must be registered in the factory"
        );
    }

    #[test]
    fn mapper_209_d001_stores_full_byte() {
        let mut mapper = make_mapper_209(); // inhibit_extended=false
        mapper.write_prg(0xD001, 0xFF);
        assert_eq!(
            mapper.mirror_reg, 0xFF,
            "Mapper 209 must store all $D001 bits (extended mirroring not inhibited)"
        );
    }

    #[test]
    fn mapper_209_standard_mirroring_bits_still_work() {
        let mut mapper = make_mapper_209();
        mapper.write_prg(0xD001, 0x01); // bits 0-1 = 1 → horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xD001, 0x02); // single-screen lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreen);
    }

    #[test]
    fn mapper_209_prg_banking_matches_mapper_90() {
        let mut m90 = make_mapper();
        let mut m209 = make_mapper_209();
        // Both should share the same PRG banking logic
        m90.write_prg(0xD000, 0x02); // 8KB mode
        m90.write_prg(0x8000, 3);
        m209.write_prg(0xD000, 0x02);
        m209.write_prg(0x8000, 3);
        assert_eq!(
            m209.read_prg(0x8000),
            m90.read_prg(0x8000),
            "Mapper 209 PRG banking must match mapper 90"
        );
    }

    #[test]
    fn mapper_209_last_prg_bank_fixed_at_power_on() {
        let mapper = make_mapper_209();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "Mapper 209 window 3 must be fixed to last bank at power-on"
        );
    }
}
