//! Mapper 282 – J.Y. Company ASIC (outer-bank variant)
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/NES_2.0_Mapper_282>
//! - Fallback: Mesen (libretro) `Core/JyCompany.h`
//!   <https://github.com/libretro/Mesen/blob/master/Core/JyCompany.h>
//!
//! Mapper 282 is the J.Y. Company ASIC with an outer-bank register at $D003.
//! Used by J.Y. multicarts (1 MiB PRG-ROM, 1 MiB CHR-ROM):
//!   - 1997 Super HIK 18-in-1 (JY-101)
//!   - 1997 Super HIK 21-in-1 (JY-105)
//!   - 1998 Super HIK 5-in-1  (JY-114)
//!   - Super 25-in-1 Final Fight (SC-128)
//!
//! # Differences from mapper 281
//!
//! The outer-bank register $D003 has a different bit layout:
//!
//! ```text
//! 7654 3210
//! ---- ----
//! ..LM CPPc
//!  || |||+- c: Select 256 KiB outer CHR-ROM bank (CHR A18), ignored if L=1
//!  || |++-- PP: Select outer PRG-ROM bank
//!  || +---- C: Select 512 KiB outer CHR-ROM bank when M=1
//!  |+------ M: 0=ignore C, 1=obey C
//!  +------- L: 0=256 KiB CHR window (mask $9x/$Ax to 0xFF), 1=512 KiB window (ignore c)
//! ```
//!
//! PRG outer bank: `mask = 0x1F`, `base = (outer_bank << 4) & 0xE0`
//! CHR outer bank:
//!   - L=0: `mask = 0x00FF`, `base = (outer_bank & 0x18) << 6 | (outer_bank & 0x01) << 8`
//!   - L=1: `mask = 0x01FF`, `base = (outer_bank & 0x18) << 6`
//!
//! # Hardware overview (shared with mapper 90/209/211/281)
//!
//! - PRG-ROM: 4 × 8 KiB independently-bankable slots covering $8000–$FFFF.
//! - CHR-ROM: 8 × 1 KiB independently-bankable slots covering $0000–$1FFF.
//! - Mirroring: programmable via $D001; extended (per-nametable) mirroring via
//!   $D001 bit 3 and $B000–$B003.
//! - IRQ: prescaler-based counter; CPU clock, PPU A12, PPU read, or CPU write sources.
//! - Hardware multiplier at $5800/$5801.
//!
//! # Register map
//!
//! | Address     | Bits  | Effect                                     |
//! |-------------|-------|--------------------------------------------|
//! | $8000–$8003 | [6:0] | PRG bank registers 0–3 (8 KiB page)       |
//! | $9000–$9007 | [7:0] | CHR bank LSB registers 0–7                 |
//! | $A000–$A007 | [7:0] | CHR bank MSB registers 0–7                 |
//! | $B000–$B003 | [0]   | Nametable low regs (extended mirroring)    |
//! | $C000       | [0]   | IRQ: 1=enable, 0=disable+ack               |
//! | $C001       | [7:6] | IRQ count dir; [2] small prescaler; [1:0] source |
//! | $C002       | –     | IRQ disable + ack                          |
//! | $C003       | –     | IRQ enable                                 |
//! | $C004       | [7:0] | IRQ prescaler (XOR'd)                      |
//! | $C005       | [7:0] | IRQ counter (XOR'd)                        |
//! | $C006       | [7:0] | IRQ XOR register                           |
//! | $D000       | [1:0] PRG mode; [4:3] CHR mode; [7] PRG at $6000 | |
//! | $D001       | [1:0] mirroring; [3] extended mirroring enable   | |
//! | $D003       | [5:0] | Outer bank register                        |
//! | $5800       | [7:0] | Multiplier input A / result low byte       |
//! | $5801       | [7:0] | Multiplier input B / result high byte      |

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::A12RisingEdgeDetector;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// IRQ clock source selection
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

/// Mapper 282 – J.Y. Company ASIC with outer-bank register
pub struct Mapper282 {
    base: BaseMapper,

    // PRG state
    prg_regs: [u8; 4],
    prg_mode: u8,       // bits 0-1 from $D000
    last_bank_sw: bool, // $D000 bit 2: true = $8003 switchable
    prg_at_6000: bool,  // $D000 bit 7

    // CHR state
    chr_low: [u8; 8],  // $9000-$9007 LSB
    chr_high: [u8; 8], // $A000-$A007 MSB
    chr_mode: u8,      // from $D000 bits 3-4

    // Outer bank (from $D003)
    outer_bank: u8,

    // Mirroring
    mirror_reg: u8,           // $D001 bits 0-1
    extended_mirroring: bool, // $D001 bit 3
    nt_low: [u8; 4],          // $B000-$B003 (nametable control low bytes)

    // IRQ
    irq_enabled: bool,
    irq_pending: bool,
    irq_source: IrqSource,
    irq_count_dir: u8,         // bits 6-7 of $C001
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

impl Mapper282 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
            outer_bank: 0,
            mirror_reg: 0,
            extended_mirroring: false,
            nt_low: [0; 4],
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

    fn chr_bank(&self, index: usize) -> u16 {
        let lo = self.chr_low[index] as u16;
        let hi = self.chr_high[index] as u16;
        (hi << 8) | lo
    }

    /// PRG outer-bank mask and base for mapper 282:
    ///   mask = 0x1F (32-bank window = 256 KiB)
    ///   base = (outer_bank << 4) & 0xE0  — uses bits 1-3 of outer_bank (PP and C)
    fn prg_mask_base(&self) -> (u8, u8) {
        let mask: u8 = 0x1F;
        let base: u8 = (self.outer_bank << 4) & 0xE0;
        (mask, base)
    }

    /// CHR outer-bank mask and base for mapper 282:
    ///   Controlled by bit 5 (L) of outer_bank:
    ///   - L=0: mask = 0x00FF, base = (outer & 0x18) << 6 | (outer & 0x01) << 8
    ///   - L=1: mask = 0x01FF, base = (outer & 0x18) << 6
    fn chr_mask_base(&self) -> (u16, u16) {
        let outer = self.outer_bank as u16;
        if (outer & 0x20) != 0 {
            // L=1: 512 KiB CHR window, ignore c (bit 0)
            let mask: u16 = 0x01FF;
            let base: u16 = (outer & 0x18) << 6;
            (mask, base)
        } else {
            // L=0: 256 KiB CHR window, use c (bit 0) as CHR A18
            let mask: u16 = 0x00FF;
            let base: u16 = ((outer & 0x18) << 6) | ((outer & 0x01) << 8);
            (mask, base)
        }
    }

    fn invert_prg_bits(v: u8) -> u8 {
        let v = v as u16;
        ((v & 1) << 6
            | (v & 2) << 3
            | (v & 4)
            | (v & 8) >> 3
            | (v & 0x10) >> 2
            | (v & 0x20) >> 4
            | (v & 0x40) >> 6) as u8
    }

    fn update_prg(&mut self) {
        let (mask, base) = self.prg_mask_base();
        let inv = (self.prg_mode & 0x03) == 3;

        let cooked = |i: usize, regs: &[u8; 4]| -> i16 {
            let r = regs[i] & 0x7F;
            let r = if inv { Self::invert_prg_bits(r) } else { r };
            ((r & mask) | base) as i16
        };

        match self.prg_mode & 0x03 {
            0 => {
                // 32KB mode: use $8003 as the 8KB page base of a 32KB block
                let b = if self.last_bank_sw {
                    let r = self.prg_regs[3] & 0x7F;
                    let r = if inv { Self::invert_prg_bits(r) } else { r };
                    let inner = r & (mask >> 2);
                    let outer = base >> 2;
                    ((inner | outer) << 2) as i16
                } else {
                    // fixed to last 32KB block in the outer window
                    let outer_base = (base as i16 >> 2) << 2;
                    outer_base | ((mask as i16 >> 2) << 2) | -4i16
                };
                self.base.select_prg_page(0, b);
                self.base.select_prg_page(1, b + 1);
                self.base.select_prg_page(2, b + 2);
                self.base.select_prg_page(3, b + 3);
            }
            1 => {
                // 16KB mode
                let lo = {
                    let r = self.prg_regs[1] & 0x7F;
                    let r = if inv { Self::invert_prg_bits(r) } else { r };
                    let inner = r & (mask >> 1);
                    let outer = base >> 1;
                    ((inner | outer) << 1) as i16
                };
                let hi = if self.last_bank_sw {
                    let r = self.prg_regs[3] & 0x7F;
                    let r = if inv { Self::invert_prg_bits(r) } else { r };
                    let inner = r & (mask >> 1);
                    let outer = base >> 1;
                    ((inner | outer) << 1) as i16
                } else {
                    // fixed: last two banks in outer window
                    let outer_base = (base as i16 >> 1) << 1;
                    outer_base | ((mask as i16 >> 1) << 1) | -2i16
                };
                self.base.select_prg_page(0, lo);
                self.base.select_prg_page(1, lo + 1);
                self.base.select_prg_page(2, hi);
                self.base.select_prg_page(3, hi + 1);
            }
            _ => {
                // 8KB mode (modes 2 and 3)
                for i in 0..3 {
                    self.base.select_prg_page(i, cooked(i, &self.prg_regs));
                }
                let last = if self.last_bank_sw {
                    cooked(3, &self.prg_regs)
                } else {
                    // fixed: last bank in outer window
                    (base | mask) as i16
                };
                self.base.select_prg_page(3, last);
            }
        }
    }

    fn update_chr(&mut self) {
        let (chr_mask, chr_base) = self.chr_mask_base();
        match self.chr_mode {
            0 => {
                // 8KB mode
                let b = ((self.chr_bank(0) & (chr_mask >> 3)) | (chr_base >> 3)) << 3;
                for i in 0..8 {
                    self.base.select_chr_page(i, (b + i as u16) as i16);
                }
            }
            1 => {
                // 4KB mode: banks at indices 0 and 4
                let b0 = ((self.chr_bank(0) & (chr_mask >> 2)) | (chr_base >> 2)) << 2;
                let b1 = ((self.chr_bank(4) & (chr_mask >> 2)) | (chr_base >> 2)) << 2;
                for i in 0..4 {
                    self.base.select_chr_page(i, (b0 + i as u16) as i16);
                    self.base.select_chr_page(4 + i, (b1 + i as u16) as i16);
                }
            }
            2 => {
                // 2KB mode: indices 0, 2, 4, 6
                for slot in 0..4 {
                    let b = ((self.chr_bank(slot * 2) & (chr_mask >> 1)) | (chr_base >> 1)) << 1;
                    self.base.select_chr_page(slot * 2, b as i16);
                    self.base.select_chr_page(slot * 2 + 1, (b + 1) as i16);
                }
            }
            _ => {
                // 1KB mode: all 8 registers
                for i in 0..8 {
                    let page = (self.chr_bank(i) & chr_mask) | chr_base;
                    self.base.select_chr_page(i, page as i16);
                }
            }
        }
    }

    fn update_mirroring(&mut self) {
        if self.extended_mirroring {
            let layout = match [
                self.nt_low[0] & 0x01,
                self.nt_low[1] & 0x01,
                self.nt_low[2] & 0x01,
                self.nt_low[3] & 0x01,
            ] {
                [0, 0, 0, 0] => NametableLayout::SingleScreen,
                [1, 1, 1, 1] => NametableLayout::SingleScreenUpper,
                [0, 0, 1, 1] => NametableLayout::Horizontal,
                [0, 1, 0, 1] => NametableLayout::Vertical,
                _ => NametableLayout::Horizontal,
            };
            self.base.set_mirroring(layout);
        } else {
            let layout = match self.mirror_reg & 0x03 {
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                2 => NametableLayout::SingleScreen,
                _ => NametableLayout::SingleScreenUpper,
            };
            self.base.set_mirroring(layout);
        }
    }

    fn update_banks(&mut self) {
        self.update_prg();
        self.update_chr();
    }

    fn tick_irq(&mut self) {
        if self.irq_count_dir == 0 || self.irq_count_dir == 3 {
            return;
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

impl Mapper for Mapper282 {
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
            0xB000..=0xB003 => {
                self.nt_low[(addr & 0x03) as usize] = value;
                self.update_mirroring();
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
                self.prg_mode = value & 0x07;
                self.last_bank_sw = (value & 0x04) != 0;
                self.chr_mode = (value >> 3) & 0x03;
                self.prg_at_6000 = (value & 0x80) != 0;
                self.update_banks();
            }
            0xD001 => {
                self.mirror_reg = value & 0x03;
                self.extended_mirroring = (value & 0x08) != 0;
                self.update_mirroring();
            }
            0xD003 => {
                self.outer_bank = value & 0x3F;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        let a12_rose = self.a12.update(addr);
        if (self.irq_source == IrqSource::PpuA12Rise && a12_rose)
            || self.irq_source == IrqSource::PpuRead
        {
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
        let mut v = Vec::with_capacity(35);
        v.push(self.prg_mode | (self.last_bank_sw as u8) << 2 | (self.prg_at_6000 as u8) << 7);
        v.push(self.chr_mode);
        v.push(self.mirror_reg | (self.extended_mirroring as u8) << 3);
        v.push(self.outer_bank);
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
        v.extend_from_slice(&self.nt_low);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 35 {
            return;
        }
        let d0 = data[0];
        self.prg_mode = d0 & 0x07;
        self.last_bank_sw = (d0 & 0x04) != 0;
        self.prg_at_6000 = (d0 & 0x80) != 0;
        self.chr_mode = data[1];
        let d2 = data[2];
        self.mirror_reg = d2 & 0x03;
        self.extended_mirroring = (d2 & 0x08) != 0;
        self.outer_bank = data[3];
        let flags = data[4];
        self.irq_enabled = (flags & 1) != 0;
        self.irq_pending = (flags & 2) != 0;
        let c1 = data[5];
        self.irq_count_dir = (c1 >> 6) & 0x03;
        self.irq_small_prescaler = (c1 & 0x04) != 0;
        self.irq_source = IrqSource::from_u8(c1);
        self.irq_prescaler = data[6];
        self.irq_counter = data[7];
        self.irq_xor = data[8];
        self.mul_a = data[9];
        self.mul_b = data[10];
        self.prg_regs.copy_from_slice(&data[11..15]);
        self.chr_low.copy_from_slice(&data[15..23]);
        self.chr_high.copy_from_slice(&data[23..31]);
        self.nt_low.copy_from_slice(&data[31..35]);
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
        self.outer_bank = 0;
        self.mirror_reg = 0;
        self.extended_mirroring = false;
        self.nt_low = [0; 4];
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
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    // Non-power-of-two bank counts to catch modulo-wrapping false-passes.
    // PRG: 32 banks per window, 3 windows tested → need > 96 banks
    // CHR: 256 banks per 256KiB window; testing outer bank 1 (c=1) → base=256+0=256; need >512
    const PRG_8K_BANKS: usize = 97; // prime, avoids modulo aliasing
    const CHR_1K_BANKS: usize = 769; // prime-ish, avoids modulo aliasing

    fn make_mapper() -> Mapper282 {
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = banked_data_with_upper_marker(1024, CHR_1K_BANKS);
        Mapper282::new(MapperContext::new_for_test(
            282,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_282_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            282,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data_with_upper_marker(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 282 must be registered in the factory"
        );
    }

    // ── Power-on state ───────────────────────────────────────────────

    #[test]
    fn prg_window3_fixed_to_last_bank_at_poweron() {
        let mapper = make_mapper();
        // Default mode 0 (32KB), last_bank_sw=false → fixed last 32KB block
        // outer_bank=0 → base=0, mask=0x1F → last bank = 31; window 3 = bank 31
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "Window 3 ($E000) must be fixed to last bank at power-on"
        );
    }

    // ── PRG outer bank windowing ──────────────────────────────────────
    //
    // Mapper 282 PRG: mask=0x1F, base=(outer_bank << 4) & 0xE0
    //   outer=0x02 (bits PP=01) → base = (0x02 << 4) & 0xE0 = 0x20 = 32
    //   outer=0x04 (bits PP=10) → base = (0x04 << 4) & 0xE0 = 0x40 = 64
    //   outer=0x08 (bit  C=1)   → base = (0x08 << 4) & 0xE0 = 0x80 = 128
    //
    // (Mapper 281 used outer & 0x03 << 5; mapper 282 uses different bits.)

    #[test]
    fn prg_outer_d003_bit1_shifts_prg_window_by_32() {
        let mut mapper = make_mapper();
        // 8KB mode (prg_mode=2), last_bank_sw=true so all slots switchable
        mapper.write_prg(0xD000, 0x06);
        mapper.write_prg(0x8000, 0);
        // outer=0 → base=0, page=0
        assert_eq!(mapper.read_prg(0x8000), 0, "outer=0, reg=0 → PRG page 0");
        // outer=0x02 → base=(0x02<<4)&0xE0=32, page=(0&0x1F)|32=32
        mapper.write_prg(0xD003, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "outer=0x02 (PP bit 0 set) → PRG page 32"
        );
    }

    #[test]
    fn prg_outer_d003_bit2_shifts_prg_window_by_64() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x06);
        mapper.write_prg(0x8000, 0);
        // outer=0x04 → base=(0x04<<4)&0xE0=0x40=64, page=64
        mapper.write_prg(0xD003, 0x04);
        assert_eq!(
            mapper.read_prg(0x8000),
            64,
            "outer=0x04 (PP bit 1 set) → PRG page 64"
        );
    }

    #[test]
    fn prg_outer_bank_mask_prevents_crossing_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x06); // 8KB mode, last_bank_sw=1
        // outer=0x02 → base=32, mask=0x1F
        // reg=0x7F → inner=0x7F & 0x1F = 31 → page = 32 | 31 = 63
        mapper.write_prg(0xD003, 0x02);
        mapper.write_prg(0x8000, 0x7F);
        assert_eq!(
            mapper.read_prg(0x8000),
            63, // 32 + 31 = 63
            "PRG masked to 0x1F within outer window; outer=0x02, reg=31 → page 63"
        );
    }

    // ── CHR outer bank windowing ──────────────────────────────────────
    //
    // Mapper 282 CHR default (L=0, bit5=0):
    //   mask = 0x00FF
    //   base = (outer & 0x18) << 6 | (outer & 0x01) << 8
    //   outer=0x01 (c=1):  base = 0 | (1<<8) = 256
    //   outer=0x08 (C=1):  base = (0x08<<6) | 0 = 512
    //   outer=0x10 (M=1):  base = (0x10<<6) | 0 = 1024
    //
    // Mapper 282 CHR with L=1 (bit5=1):
    //   mask = 0x01FF
    //   base = (outer & 0x18) << 6  (c ignored)

    #[test]
    fn chr_outer_c_bit0_shifts_chr_window_256kb_when_l_is_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3 (1KB)
        mapper.write_prg(0x9000, 0x00);
        mapper.write_prg(0xA000, 0x00); // bank 0
        // outer=0 → base=0, page=0 → marker byte = 0
        assert_eq!(mapper.read_chr(0x0000), 0, "outer=0 → CHR page 0");
        // outer=0x01 (c=1, L=0) → base=256, page=256 → marker = 256>>8 = 1
        mapper.write_prg(0xD003, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "outer=0x01 (c=1, L=0) → CHR page 256, marker=1"
        );
    }

    #[test]
    fn chr_outer_c_bit0_ignored_when_l_is_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3 (1KB)
        mapper.write_prg(0x9000, 0x00);
        mapper.write_prg(0xA000, 0x00); // bank 0
        // outer=0x21 (L=1, c=1) → L=1 so c ignored; base=(0x20&0x18)<<6=0; page=0 → marker=0
        mapper.write_prg(0xD003, 0x21);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "outer=0x21 (L=1, c=1) → c ignored; CHR page 0"
        );
    }

    #[test]
    fn chr_outer_l1_enables_512kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3 (1KB)
        // outer=0x20 (L=1, others=0) → mask=0x1FF, base=0
        // Set chr_reg=0x1FF (9 bits = 511) → page=511 → marker=511>>8=1
        mapper.write_prg(0xD003, 0x20);
        mapper.write_prg(0x9000, 0xFF); // LSB = 0xFF
        mapper.write_prg(0xA000, 0x01); // MSB = 0x01 → bank = 0x01FF = 511
        assert_eq!(
            mapper.read_chr(0x0000),
            1, // marker for page 511 = 511>>8 = 1
            "L=1 enables 512KB window; chr_reg=511 → page 511, marker=1"
        );
    }

    #[test]
    fn chr_outer_l0_mask_is_0xff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // chr_mode=3 (1KB)
        // outer=0x08 (C=1, L=0) → base=(0x08<<6)=512, mask=0xFF
        // chr_reg=255 → page=(255&0xFF)|512=767 → marker=767>>8=2
        mapper.write_prg(0xD003, 0x08);
        mapper.write_prg(0x9000, 0xFF);
        mapper.write_prg(0xA000, 0x00); // bank=0x00FF=255
        assert_eq!(
            mapper.read_chr(0x0000),
            2, // 767>>8 = 2
            "L=0 mask=0xFF; outer=0x08 base=512, reg=255 → page 767, marker=2"
        );
    }

    // ── Mirroring ────────────────────────────────────────────────────

    #[test]
    fn mirroring_default_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_selects_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xD001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xD001, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreen);
        mapper.write_prg(0xD001, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn extended_mirroring_enabled_by_d001_bit3() {
        let mut mapper = make_mapper();
        assert!(!mapper.extended_mirroring);
        mapper.write_prg(0xD001, 0x08);
        assert!(
            mapper.extended_mirroring,
            "$D001 bit 3 must enable extended mirroring"
        );
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
    fn irq_fires_on_cpu_clock_prescaler_overflow() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC001, 0x44); // dir=1, small_prescaler=1, source=CpuClock
        mapper.write_prg(0xC004, 0xF8); // prescaler = 0xF8 ^ 0 = 0xF8; masked → 0
        mapper.write_prg(0xC005, 0xFF); // counter = 0xFF (will overflow → fire)
        mapper.write_prg(0xC003, 0); // enable IRQ
        for _ in 0..8 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after prescaler overflow"
        );
    }

    // ── Multiplier ───────────────────────────────────────────────────

    #[test]
    fn multiplier_5800_5801_product() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5800, 7);
        mapper.write_prg(0x5801, 11);
        assert_eq!(mapper.read_prg_open_bus(0x5800, 0xFF), 77);
        assert_eq!(mapper.read_prg_open_bus(0x5801, 0xFF), 0);
    }

    // ── Snapshot / restore ────────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip_preserves_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x12); // prg_mode=2, chr_mode=2
        mapper.write_prg(0xD001, 0x01); // mirror_reg=1 (Horizontal)
        mapper.write_prg(0xD003, 0x02); // outer_bank=0x02
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x9002, 7);
        mapper.write_prg(0xA003, 2);
        mapper.write_prg(0xC001, 0x44);
        mapper.write_prg(0xC004, 0x10);
        mapper.write_prg(0xC005, 0x20);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0x5800, 3);
        mapper.write_prg(0x5801, 9);

        let snap = mapper.registers_snapshot();
        assert_eq!(snap.len(), 35, "snapshot must be exactly 35 bytes");

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.prg_mode, mapper.prg_mode);
        assert_eq!(restored.chr_mode, mapper.chr_mode);
        assert_eq!(restored.mirror_reg, mapper.mirror_reg);
        assert_eq!(restored.outer_bank, mapper.outer_bank);
        assert_eq!(restored.irq_enabled, mapper.irq_enabled);
        assert_eq!(restored.irq_prescaler, mapper.irq_prescaler);
        assert_eq!(restored.irq_counter, mapper.irq_counter);
        assert_eq!(restored.mul_a, mapper.mul_a);
        assert_eq!(restored.mul_b, mapper.mul_b);
        assert_eq!(restored.prg_regs, mapper.prg_regs);
        assert_eq!(restored.chr_low, mapper.chr_low);
        assert_eq!(restored.chr_high, mapper.chr_high);
        assert_eq!(restored.nt_low, mapper.nt_low);
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5);
        mapper.restore_registers(&[0u8; 34]); // 34 bytes → must be ignored
        assert_eq!(
            mapper.prg_regs[0], 5,
            "restore with short data must not overwrite state"
        );
    }

    // ── Reset ──────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD003, 0x02);
        mapper.write_prg(0x8000, 10);
        mapper.write_prg(0xC003, 0);
        mapper.reset();
        assert_eq!(mapper.outer_bank, 0, "outer_bank cleared on reset");
        assert_eq!(mapper.prg_regs[0], 0, "prg_regs cleared on reset");
        assert!(!mapper.irq_enabled, "irq_enabled cleared on reset");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
