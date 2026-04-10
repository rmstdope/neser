//! Mapper 211 – J.Y. Company ASIC (extended-mirroring variant)
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/INES_Mapper_211> (redirects to J.Y. Company ASIC)
//! - Primary: <https://www.nesdev.org/wiki/J.Y._Company_ASIC>
//! - Fallback: <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/JyCompany/JyCompany.h>
//!
//! Mapper 211 is a variant of the J.Y. Company ASIC (mappers 90/209/211).
//! Unlike mapper 90, the ROM-nametable jumper is NOT present, so extended
//! (per-nametable CIRAM) mirroring is available via $D001 bit 3.  The outer
//! PRG/CHR bank size is 512 KiB, identical to mappers 90 and 209.
//!
//! Per NesDev, mapper 211 is a functional duplicate of mapper 209 with correct
//! emulation (the distinction between 209 and 211 was a historical artefact).
//!
//! # Hardware overview
//! - PRG-ROM: 4 × 8 KiB independently-bankable slots ($8000–$FFFF).
//! - CHR-ROM: 8 × 1 KiB independently-bankable slots ($0000–$1FFF).
//! - Mirroring: standard (V/H/single) or extended per-nametable via $D001 bit 3
//!   and $B000–$B003 registers.
//! - IRQ: flexible prescaler counter; CPU clock, PPU A12, PPU read, or CPU write.
//! - Hardware multiplier at $5800/$5801.
//!
//! # Register map
//!
//! | Address      | Mask   | Effect                                              |
//! |--------------|--------|-----------------------------------------------------|
//! | $5000        | $FFFF  | Jumper register (read, returns 0)                   |
//! | $5800/$5801  | $F803  | Hardware multiplier                                 |
//! | $8000–$8003  | $F803  | PRG bank registers 0–3 (bits 6:0 = 8 KiB page)     |
//! | $9000–$9007  | $F807  | CHR bank LSB registers 0–7                          |
//! | $A000–$A007  | $F807  | CHR bank MSB registers 0–7                          |
//! | $B000–$B003  | $F807  | Nametable low regs (extended mirroring)              |
//! | $C000        | $F007  | IRQ enable/disable                                  |
//! | $C001        | $F007  | IRQ mode                                            |
//! | $C002        | $F007  | IRQ disable + ack                                   |
//! | $C003        | $F007  | IRQ enable                                          |
//! | $C004        | $F007  | IRQ prescaler (XOR'd with $C006)                    |
//! | $C005        | $F007  | IRQ counter  (XOR'd with $C006)                     |
//! | $C006        | $F007  | IRQ XOR register                                    |
//! | $D000        | $F803  | Mode select (PRG/CHR modes, PRG at $6000)           |
//! | $D001        | $F803  | Mirroring / extended-mirroring enable ($D001 bit 3) |
//! | $D003        | $F803  | (unused for 211; no outer bank)                     |

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::common::A12RisingEdgeDetector;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

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

/// Mapper 211 – J.Y. Company ASIC (extended-mirroring variant)
pub struct Mapper211 {
    base: BaseMapper,

    // PRG state
    prg_regs: [u8; 4],
    prg_mode: u8,       // bits 0-1 from $D000
    last_bank_sw: bool, // $D000 bit 2: true = $8003 switchable
    prg_at_6000: bool,  // $D000 bit 7

    // CHR state
    chr_low: [u8; 8],  // $9000-$9007 LSB
    chr_high: [u8; 8], // $A000-$A007 MSB
    chr_mode: u8,      // $D000 bits 3-4

    // Mirroring
    mirror_reg: u8,           // $D001 bits 0-1 (standard mode)
    extended_mirroring: bool, // $D001 bit 3
    nt_low: [u8; 4],          // $B000-$B003 (extended-mirroring per-nametable low regs)

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

impl Mapper211 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
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

    fn chr_bank(&self, index: usize) -> i16 {
        let lo = self.chr_low[index] as u16;
        let hi = self.chr_high[index] as u16;
        ((hi << 8) | lo) as i16
    }

    fn invert_prg_bits(r: u8) -> u8 {
        let r = r as u16;
        ((r & 1) << 6
            | (r & 2) << 3
            | (r & 4)
            | (r & 8) >> 3
            | (r & 0x10) >> 2
            | (r & 0x20) >> 4
            | (r & 0x40) >> 6) as u8
    }

    fn update_prg(&mut self) {
        let inv = (self.prg_mode & 0x03) == 3;

        let cooked = |i: usize, regs: &[u8; 4]| -> i16 {
            let r = regs[i] & 0x7F;
            if inv {
                Self::invert_prg_bits(r) as i16
            } else {
                r as i16
            }
        };

        match self.prg_mode & 0x03 {
            0 => {
                // 32KB mode
                let b = if self.last_bank_sw {
                    cooked(3, &self.prg_regs)
                } else {
                    -4i16
                };
                self.base.select_prg_page(0, b);
                self.base.select_prg_page(1, b + 1);
                self.base.select_prg_page(2, b + 2);
                self.base.select_prg_page(3, b + 3);
            }
            1 => {
                // 16KB mode
                let lo = cooked(1, &self.prg_regs) << 1;
                let hi = if self.last_bank_sw {
                    cooked(3, &self.prg_regs) << 1
                } else {
                    -2i16
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
                    -1i16
                };
                self.base.select_prg_page(3, last);
            }
        }

        // $D000 bit 7: map PRG register 0 to $6000-$7FFF when enabled
        if self.prg_at_6000 {
            self.base.configure_prg_6000_banking();
            self.base.select_prg_6000_page(cooked(0, &self.prg_regs));
        } else {
            self.base.disable_prg_6000_banking();
        }
    }

    fn update_chr(&mut self) {
        match self.chr_mode {
            0 => {
                // 8KB mode
                let b = self.chr_bank(0) << 3;
                for i in 0..8 {
                    self.base.select_chr_page(i, b + i as i16);
                }
            }
            1 => {
                // 4KB mode
                let b0 = self.chr_bank(0) << 2;
                let b1 = self.chr_bank(4) << 2;
                for i in 0..4 {
                    self.base.select_chr_page(i, b0 + i as i16);
                    self.base.select_chr_page(4 + i, b1 + i as i16);
                }
            }
            2 => {
                // 2KB mode
                for slot in 0..4 {
                    let b = self.chr_bank(slot * 2) << 1;
                    self.base.select_chr_page(slot * 2, b);
                    self.base.select_chr_page(slot * 2 + 1, b + 1);
                }
            }
            _ => {
                // 1KB mode
                for i in 0..8 {
                    self.base.select_chr_page(i, self.chr_bank(i));
                }
            }
        }
    }

    fn update_mirroring(&mut self) {
        if self.extended_mirroring {
            // Per-nametable: bit 0 selects CIRAM bank (A=0 / B=1)
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

impl Mapper for Mapper211 {
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
        let mut v = Vec::with_capacity(36);
        v.push(self.prg_mode | (self.last_bank_sw as u8) << 2 | (self.prg_at_6000 as u8) << 7);
        v.push(self.chr_mode);
        v.push(self.mirror_reg | (self.extended_mirroring as u8) << 3);
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
        if data.len() < 30 {
            return;
        }
        let d0 = data[0];
        self.prg_mode = d0 & 0x03;
        self.last_bank_sw = (d0 & 0x04) != 0;
        self.prg_at_6000 = (d0 & 0x80) != 0;
        self.chr_mode = data[1];
        let d2 = data[2];
        self.mirror_reg = d2 & 0x03;
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
        if data.len() >= 34 {
            self.extended_mirroring = (d2 & 0x08) != 0;
            self.nt_low.copy_from_slice(&data[30..34]);
        } else {
            self.extended_mirroring = false;
            self.nt_low = [0; 4];
        }
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
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K_BANKS: usize = 11;
    const CHR_1K_BANKS: usize = 13;

    fn make_mapper() -> Mapper211 {
        let prg = banked_data(8 * 1024, PRG_8K_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        Mapper211::new(MapperContext::new_for_test(
            211,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_211_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            211,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 211 must be registered in the factory"
        );
    }

    // ── PRG banking – 8KB mode ────────────────────────────────────────

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
        mapper.write_prg(0xD000, 0x02); // 8KB mode
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
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    #[test]
    fn prg_8k_write_8002_selects_window2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x02);
        mapper.write_prg(0x8002, 7);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn prg_8k_last_bank_fixed_when_bit2_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x02); // 8KB mode, bit2=0 (fixed)
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "Window 3 must remain fixed when $D000 bit 2 is clear"
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

    // ── PRG banking – 16KB mode ───────────────────────────────────────

    #[test]
    fn prg_16k_8001_selects_lower_half() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x01); // 16KB mode
        mapper.write_prg(0x8001, 2); // 16KB bank 2 → 8KB pages 4+5
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    // ── CHR banking – all modes ───────────────────────────────────────

    #[test]
    fn chr_1kb_mode_all_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x18); // CHR mode = 3 (1KB)
        for i in 0u8..8 {
            let bank = (i as usize % CHR_1K_BANKS) as u8;
            mapper.write_prg(0x9000 + i as u16, bank);
        }
        for i in 0u8..8 {
            let expected = (i as usize % CHR_1K_BANKS) as u8;
            assert_eq!(
                mapper.read_chr(i as u16 * 0x400),
                expected,
                "CHR window {i} must map to bank {expected}"
            );
        }
    }

    #[test]
    fn chr_8kb_mode_single_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x00); // CHR mode = 0 (8KB)
        mapper.write_prg(0x9000, 0); // select 8KB bank 0 → 1KB pages 0-7
        // In 8KB mode bank 0: slot 0 → 1KB page 0 (value 0), slot 7 → 1KB page 7 (value 7)
        assert_eq!(mapper.read_chr(0x0000), 0); // slot 0 = page 0
        assert_eq!(mapper.read_chr(0x1C00), 7); // slot 7 = page 7
    }

    // ── Mirroring – standard mode ─────────────────────────────────────

    #[test]
    fn mirroring_vertical_by_default() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_bit1_0_is_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_bit1_1_is_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_d001_value2_is_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreen);
    }

    #[test]
    fn mirroring_d001_value3_is_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    // ── Mirroring – extended mode (mapper 211 specific) ───────────────

    #[test]
    fn extended_mirroring_enabled_by_d001_bit3() {
        let mut mapper = make_mapper();
        // Enable extended mirroring; set nt[0..3] = [0, 1, 0, 1] → Vertical
        mapper.write_prg(0xD001, 0x08); // extended_mirroring = true
        mapper.write_prg(0xB000, 0x00); // nt[0] bit0 = 0
        mapper.write_prg(0xB001, 0x01); // nt[1] bit0 = 1
        mapper.write_prg(0xB002, 0x00); // nt[2] bit0 = 0
        mapper.write_prg(0xB003, 0x01); // nt[3] bit0 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Extended mirroring [0,1,0,1] must yield Vertical"
        );
    }

    #[test]
    fn extended_mirroring_horizontal_pattern() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x08);
        mapper.write_prg(0xB000, 0x00); // nt[0] = 0
        mapper.write_prg(0xB001, 0x00); // nt[1] = 0
        mapper.write_prg(0xB002, 0x01); // nt[2] = 1
        mapper.write_prg(0xB003, 0x01); // nt[3] = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Extended mirroring [0,0,1,1] must yield Horizontal"
        );
    }

    #[test]
    fn extended_mirroring_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x08);
        for i in 0..4 {
            mapper.write_prg(0xB000 + i, 0x00); // all bank 0
        }
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreen,
            "Extended mirroring [0,0,0,0] must yield SingleScreen"
        );
    }

    #[test]
    fn extended_mirroring_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x08);
        for i in 0..4 {
            mapper.write_prg(0xB000 + i, 0x01); // all bank 1
        }
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "Extended mirroring [1,1,1,1] must yield SingleScreenUpper"
        );
    }

    #[test]
    fn extended_mirroring_disabled_reverts_to_mirror_reg() {
        let mut mapper = make_mapper();
        // First enable extended mirroring and set it up
        mapper.write_prg(0xD001, 0x08); // extended
        mapper.write_prg(0xB000, 0x00);
        mapper.write_prg(0xB001, 0x01);
        mapper.write_prg(0xB002, 0x00);
        mapper.write_prg(0xB003, 0x01);
        // Disable extended mirroring (keep mirror_reg = 1 = Horizontal)
        mapper.write_prg(0xD001, 0x01); // no bit3, mirror_reg = 1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Disabling extended mirroring must revert to mirror_reg-based layout"
        );
    }

    // ── IRQ ───────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_poweron() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_fires_on_cpu_clock_source() {
        let mut mapper = make_mapper();
        // IRQ source = CPU clock (default), count up, prescaler = FF
        // With small prescaler bit clear, mask = 0xFF
        // Direction 1 (up), prescaler wraps after 256 ticks → counter++
        // Enable IRQ, set counter to 0xFE so it fires after 2 prescaler wraps
        mapper.write_prg(0xC001, 0x40); // direction = up, source = CPU clock
        mapper.write_prg(0xC006, 0x00); // xor = 0
        mapper.write_prg(0xC005, 0xFE); // counter = 0xFE, fires when it wraps (0xFF→0x00)
        mapper.write_prg(0xC004, 0xFF); // prescaler = 0xFF (wraps after 1 more tick)
        mapper.write_prg(0xC003, 0); // enable IRQ

        // tick 257 times: one more tick to wrap prescaler, then counter++, then 1 more wrap
        let mut fired = false;
        for _ in 0..10000 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(
            fired,
            "IRQ must fire after counter wraps with CPU clock source"
        );
    }

    #[test]
    fn irq_fires_on_a12_rise_source() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC001, 0x41); // direction=up, source=PPU A12
        mapper.write_prg(0xC006, 0x00);
        mapper.write_prg(0xC005, 0xFE);
        mapper.write_prg(0xC004, 0xFF);
        mapper.write_prg(0xC003, 0); // enable

        let mut fired = false;
        for _ in 0..10000 {
            mapper.cpu_cycle();
            mapper.ppu_address_changed(0x0000);
            for _ in 0..4 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000); // A12 rise
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(fired, "IRQ must fire with PPU A12 source");
    }

    #[test]
    fn irq_c002_disables_and_acks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC001, 0x40);
        mapper.write_prg(0xC005, 0xFF);
        mapper.write_prg(0xC004, 0xFF);
        mapper.write_prg(0xC003, 0);
        for _ in 0..10000 {
            mapper.cpu_cycle();
            if mapper.irq_pending() {
                break;
            }
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xC002, 0);
        assert!(!mapper.irq_pending());
        assert!(!mapper.irq_enabled);
    }

    // ── Hardware multiplier ───────────────────────────────────────────

    #[test]
    fn multiplier_5800_5801_returns_product() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5800, 12);
        mapper.write_prg(0x5801, 13);
        let lo = mapper.read_prg_open_bus(0x5800, 0xFF);
        let hi = mapper.read_prg_open_bus(0x5801, 0xFF);
        let product = (lo as u16) | ((hi as u16) << 8);
        assert_eq!(product, 12 * 13, "Multiplier must return 12×13=156");
    }

    #[test]
    fn multiplier_overflow_16bit() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5800, 255);
        mapper.write_prg(0x5801, 255);
        let lo = mapper.read_prg_open_bus(0x5800, 0xFF);
        let hi = mapper.read_prg_open_bus(0x5801, 0xFF);
        let product = (lo as u16) | ((hi as u16) << 8);
        assert_eq!(product, 255 * 255, "Multiplier overflow must be handled");
    }

    // ── Snapshot / restore ────────────────────────────────────────────

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut a = make_mapper();
        a.write_prg(0xD000, 0x02); // 8KB mode
        a.write_prg(0x8000, 2);
        a.write_prg(0x8001, 4);
        a.write_prg(0x8002, 6);
        a.write_prg(0x9000, 3);
        a.write_prg(0xD001, 0x09); // Horizontal + extended
        a.write_prg(0xB000, 0x00);
        a.write_prg(0xB001, 0x00);
        a.write_prg(0xB002, 0x01);
        a.write_prg(0xB003, 0x01);
        a.write_prg(0xC005, 7);
        a.write_prg(0xC003, 0); // enable IRQ

        let snap = a.registers_snapshot();
        let mut b = make_mapper();
        b.restore_registers(&snap);

        assert_eq!(b.read_prg(0x8000), a.read_prg(0x8000));
        assert_eq!(b.read_prg(0xA000), a.read_prg(0xA000));
        assert_eq!(b.read_prg(0xC000), a.read_prg(0xC000));
        assert_eq!(b.read_chr(0x0000), a.read_chr(0x0000));
        assert_eq!(b.get_mirroring(), a.get_mirroring());
        assert_eq!(b.irq_counter, a.irq_counter);
        assert_eq!(b.irq_enabled, a.irq_enabled);
        assert_eq!(b.extended_mirroring, a.extended_mirroring);
        assert_eq!(b.nt_low, a.nt_low);
    }

    // ── PRG at $6000 ──────────────────────────────────────────────────

    #[test]
    fn prg_at_6000_disabled_by_default() {
        let mapper = make_mapper();
        // Without prg_at_6000, $6000 should not return banked PRG-ROM
        assert!(!mapper.base.has_prg_6000_banking());
    }

    #[test]
    fn prg_at_6000_enables_6000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x82); // 8KB mode + prg_at_6000 (bit 7)
        mapper.write_prg(0x8000, 3); // select bank 3 for reg 0
        // $6000 must now read bank 3
        assert_eq!(mapper.read_prg(0x6000), 3);
        assert!(mapper.base.has_prg_6000_banking());
    }

    #[test]
    fn prg_at_6000_disable_removes_6000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x82); // enable
        assert!(mapper.base.has_prg_6000_banking());
        mapper.write_prg(0xD000, 0x02); // disable (bit 7 clear)
        assert!(!mapper.base.has_prg_6000_banking());
    }

    // ── Snapshot / restore – 30-byte (mapper 90/209) compatibility ────

    #[test]
    fn restore_registers_accepts_30_byte_snapshot() {
        let mut a = make_mapper();
        a.write_prg(0xD000, 0x02); // 8KB mode
        a.write_prg(0x8000, 2);
        a.write_prg(0xD001, 0x01); // horizontal

        // Build a 30-byte snapshot (mapper 90/209 format, no nt_low)
        let snap_34 = a.registers_snapshot();
        assert_eq!(snap_34.len(), 34);
        let snap_30 = snap_34[..30].to_vec();

        let mut b = make_mapper();
        b.restore_registers(&snap_30);

        assert_eq!(b.read_prg(0x8000), a.read_prg(0x8000));
        assert_eq!(b.get_mirroring(), a.get_mirroring());
        // extended_mirroring and nt_low default to initial values
        assert!(!b.extended_mirroring);
        assert_eq!(b.nt_low, [0u8; 4]);
    }

    #[test]
    fn restore_registers_rejects_snapshot_shorter_than_30_bytes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5);
        let bank_before = mapper.read_prg(0x8000);
        mapper.restore_registers(&[0u8; 29]);
        // State must be unchanged
        assert_eq!(mapper.read_prg(0x8000), bank_before);
    }

    // ── Reset ─────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_defaults() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x09);
        mapper.write_prg(0x8000, 5);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_8K_BANKS - 1) as u8,
            "After reset, window 3 must be fixed to last bank"
        );
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert!(!mapper.extended_mirroring);
        assert!(!mapper.irq_pending());
    }
}
