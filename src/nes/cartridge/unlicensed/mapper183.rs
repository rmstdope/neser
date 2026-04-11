//! Mapper 183 – Custom mapper with VRC4-style CHR and CPU-cycle IRQ
//!
//! Specifications:
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper183.h`
//!
//! ## Overview
//!
//! An unlicensed board with VRC4-style nibble CHR banking, four switchable
//! 8 KiB PRG-ROM slots, an extra 8 KiB PRG-ROM window at $6000–$7FFF, and a
//! simple CPU-cycle based IRQ.
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: 8 KiB PRG-ROM, bank selected by `_prgReg`
//! * `CPU $8000–$9FFF`: 8 KiB PRG-ROM, slot 0 (switchable)
//! * `CPU $A000–$BFFF`: 8 KiB PRG-ROM, slot 1 (switchable)
//! * `CPU $C000–$DFFF`: 8 KiB PRG-ROM, slot 2 (switchable)
//! * `CPU $E000–$FFFF`: 8 KiB PRG-ROM, slot 3 (fixed to last bank)
//! * `PPU $0000–$1FFF`: 8 × 1 KiB CHR-ROM, each slot switchable via nibble writes
//!
//! ## PRG Registers
//!
//! All writes to `$6000–$FFFF` are intercepted:
//!
//! | Condition                     | Effect |
//! |-------------------------------|--------|
//! | `(addr & 0xF800) == 0x6800`   | `prgReg = addr & 0x3F` → update $6000 bank |
//! | `(addr & 0xF80C) == 0x8800`   | Slot 0 (`$8000–$9FFF`) ← `value` |
//! | `(addr & 0xF80C) == 0xA800`   | Slot 1 (`$A000–$BFFF`) ← `value` |
//! | `(addr & 0xF80C) == 0xA000`   | Slot 2 (`$C000–$DFFF`) ← `value` |
//!
//! ## Mirroring Register – `(addr & 0xF80C) == 0x9800`
//!
//! ```text
//! value[1:0]: 0=V, 1=H, 2=screen A only, 3=screen B only
//! ```
//!
//! ## CHR Registers – `$B000–$E7FF`
//!
//! 8 × 1 KiB CHR bank registers, each updated as a byte built from two nibble
//! writes.  The slot index and nibble are derived from the address:
//!
//! ```text
//! slot  = (((addr >> 11) - 6) | (addr >> 3)) & 0x07
//! nibble = addr & 0x04   (0 = low, 4 = high)
//! ```
//!
//! A nibble 0 write stores `value & 0x0F` in the lower nibble.
//! A nibble 4 write stores `(value & 0x0F) << 4` in the upper nibble.
//!
//! ## IRQ
//!
//! The IRQ is driven by a CPU clock divider.  Every 114 CPU cycles the 8-bit
//! `irqCounter` increments; on overflow (wrap to 0) an IRQ is requested
//! one CPU cycle later.
//!
//! | `(addr & 0xF80C)` | Effect |
//! |-------------------|--------|
//! | `0xF000`          | `irqCounter[3:0] = value & 0x0F` |
//! | `0xF004`          | `irqCounter[7:4] = (value & 0x0F) << 4` |
//! | `0xF008`          | `irqEnabled = value != 0`; if disabled resets scaler; always clears IRQ |

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 183;
const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;

/// Mapper 183 – custom VRC4-style mapper with CPU-cycle IRQ.
///
/// See the module-level documentation for hardware details.
pub struct Mapper183 {
    base: BaseMapper,
    chr_regs: [u8; 8],
    /// PRG-ROM bank selection for slots 0–2 ($8000–$DFFF).
    prg_banks: [u8; 3],
    prg_reg: u8,
    irq_counter: u8,
    irq_scaler: u8,
    irq_enabled: bool,
    irq_active: bool,
    need_irq: bool,
}

impl Mapper183 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        base.configure_prg_6000_banking();

        let mut mapper = Self {
            base,
            chr_regs: [0; 8],
            prg_banks: [0; 3],
            prg_reg: 0,
            irq_counter: 0,
            irq_scaler: 0,
            irq_enabled: false,
            irq_active: false,
            need_irq: false,
        };
        mapper.update_prg();
        mapper
    }

    fn update_prg(&mut self) {
        self.base.select_prg_6000_page(self.prg_reg as i16);
        self.base.select_prg_page(3, -1);
    }

    fn update_chr(&mut self, slot: usize) {
        self.base.select_chr_page(slot, self.chr_regs[slot] as i16);
    }

    fn write_chr_nibble(&mut self, addr: u16, value: u8) {
        let addr32 = addr as u32;
        let slot = ((((addr32 >> 11) - 6) | (addr32 >> 3)) & 0x07) as usize;
        if (addr & 0x04) == 0 {
            // low nibble
            self.chr_regs[slot] = (self.chr_regs[slot] & 0xF0) | (value & 0x0F);
        } else {
            // high nibble
            self.chr_regs[slot] = (self.chr_regs[slot] & 0x0F) | ((value & 0x0F) << 4);
        }
        self.update_chr(slot);
    }
}

impl Mapper for Mapper183 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.base.try_read_prg_6000(addr) {
            return value;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_banked(addr);
        }
        0
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // $6800–$6FFF: set the $6000 PRG-ROM window bank
        if (addr & 0xF800) == 0x6800 {
            self.prg_reg = (addr & 0x3F) as u8;
            self.update_prg();
            return;
        }

        // $B000–$E7FF: CHR nibble writes
        if (addr & 0xF800) >= 0xB000 && (addr & 0xF800) <= 0xE000 {
            self.write_chr_nibble(addr, value);
            return;
        }

        // All other registers use the masked address
        match addr & 0xF80C {
            0x8800 => {
                self.prg_banks[0] = value;
                self.base.select_prg_page(0, value as i16);
            }
            0xA800 => {
                self.prg_banks[1] = value;
                self.base.select_prg_page(1, value as i16);
            }
            0xA000 => {
                self.prg_banks[2] = value;
                self.base.select_prg_page(2, value as i16);
            }
            0x9800 => {
                let mirroring = match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    _ => NametableLayout::SingleScreenUpper,
                };
                self.base.set_mirroring(mirroring);
            }
            0xF000 => {
                self.irq_counter = (self.irq_counter & 0xF0) | (value & 0x0F);
            }
            0xF004 => {
                self.irq_counter = (self.irq_counter & 0x0F) | ((value & 0x0F) << 4);
            }
            0xF008 => {
                self.irq_enabled = value != 0;
                if !self.irq_enabled {
                    self.irq_scaler = 0;
                }
                self.irq_active = false;
                self.need_irq = false;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if self.need_irq {
            self.irq_active = true;
            self.need_irq = false;
        }
        self.irq_scaler = self.irq_scaler.wrapping_add(1);
        if self.irq_scaler == 114 {
            self.irq_scaler = 0;
            if self.irq_enabled {
                self.irq_counter = self.irq_counter.wrapping_add(1);
                if self.irq_counter == 0 {
                    self.need_irq = true;
                }
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_active
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = vec![self.prg_reg, self.irq_counter, self.irq_scaler];
        let flags = (self.irq_enabled as u8)
            | ((self.irq_active as u8) << 1)
            | ((self.need_irq as u8) << 2);
        snap.push(flags);
        snap.extend_from_slice(&self.prg_banks);
        snap.extend_from_slice(&self.chr_regs);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 15 {
            return;
        }
        self.prg_reg = data[0];
        self.irq_counter = data[1];
        self.irq_scaler = data[2];
        self.irq_enabled = (data[3] & 0x01) != 0;
        self.irq_active = (data[3] & 0x02) != 0;
        self.need_irq = (data[3] & 0x04) != 0;
        self.prg_banks.copy_from_slice(&data[4..7]);
        self.chr_regs.copy_from_slice(&data[7..15]);
        self.update_prg();
        for slot in 0..3 {
            self.base.select_prg_page(slot, self.prg_banks[slot] as i16);
        }
        for slot in 0..8 {
            self.update_chr(slot);
        }
    }

    fn reset(&mut self) {
        self.chr_regs = [0; 8];
        self.prg_banks = [0; 3];
        self.prg_reg = 0;
        self.irq_counter = 0;
        self.irq_scaler = 0;
        self.irq_enabled = false;
        self.irq_active = false;
        self.need_irq = false;
        self.update_prg();
        for slot in 0..8 {
            self.update_chr(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 8 × 8KB PRG banks, 16 × 1KB CHR banks (non-power-of-two to catch wrap bugs)
    const PRG_BANKS: usize = 8;
    const CHR_1K_BANKS: usize = 16;

    fn make_mapper() -> Mapper183 {
        Mapper183::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_183_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 183 must be creatable via factory");
    }

    #[test]
    fn power_on_e000_is_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "slot 3 ($E000) must be fixed to last bank"
        );
    }

    #[test]
    fn power_on_6000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "$6000 must read bank 0 at power-on"
        );
    }

    #[test]
    fn prg_slot0_write_via_8800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8800, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8800 write must set slot 0");
    }

    #[test]
    fn prg_slot1_write_via_a800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA800, 2);
        assert_eq!(mapper.read_prg(0xA000), 2, "$A800 write must set slot 1");
    }

    #[test]
    fn prg_slot2_write_via_a000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 4);
        assert_eq!(mapper.read_prg(0xC000), 4, "$A000 write must set slot 2");
    }

    #[test]
    fn prg_6000_window_set_via_6800() {
        let mut mapper = make_mapper();
        // Write to $6803: addr & 0x3F = 3 → prg_reg=3
        mapper.write_prg(0x6803, 0);
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "$6803 write must set $6000 bank to 3"
        );
    }

    #[test]
    fn chr_nibble_write_slot0_low_then_high() {
        let mut mapper = make_mapper();
        // $B000: slot 0, low nibble; $B004: slot 0, high nibble
        mapper.write_prg(0xB000, 0x05); // low nibble = 5
        mapper.write_prg(0xB004, 0x03); // high nibble = 3 → chr_reg[0] = 0x35
        assert_eq!(mapper.chr_regs[0], 0x35);
        // CHR bank 0x35 with 16 banks → 0x35 % 16 = 5
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    #[test]
    fn chr_nibble_write_slot7() {
        let mut mapper = make_mapper();
        // Slot 7: first occurrence at $B038/$B03C
        mapper.write_prg(0xB038, 0x09); // low nibble = 9
        mapper.write_prg(0xB03C, 0x00); // high nibble = 0 → chr_reg[7] = 0x09
        assert_eq!(mapper.chr_regs[7], 0x09);
    }

    #[test]
    fn mirroring_via_9800() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9800, 0x01); // Horizontal
        assert_eq!(mapper.base().mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x9800, 0x00); // Vertical
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn irq_counter_nibble_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x05); // low nibble = 5
        mapper.write_prg(0xF004, 0x0A); // high nibble = A → counter = 0xA5
        assert_eq!(mapper.irq_counter, 0xA5);
    }

    #[test]
    fn irq_fires_after_counter_overflow() {
        let mut mapper = make_mapper();
        // Set counter to 0xFF, enable IRQ
        mapper.write_prg(0xF000, 0x0F); // low nibble = F
        mapper.write_prg(0xF004, 0x0F); // high nibble = F → counter = 0xFF
        mapper.write_prg(0xF008, 0x01); // enable IRQ

        assert!(!mapper.irq_pending(), "IRQ must not be pending yet");

        // Advance until scaler wraps (114 cycles) to increment counter from 0xFF → 0x00 (overflow)
        for _ in 0..114 {
            mapper.cpu_cycle();
        }
        // need_irq is now set; one more cycle fires it
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must fire after counter overflow");
    }

    #[test]
    fn irq_cleared_by_f008_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x0F);
        mapper.write_prg(0xF004, 0x0F);
        mapper.write_prg(0xF008, 0x01);
        for _ in 0..115 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xF008, 0x00); // disable and clear
        assert!(!mapper.irq_pending(), "IRQ must be cleared by $F008 write");
    }

    #[test]
    fn reset_clears_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8800, 5);
        mapper.write_prg(0x6803, 0);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x6000), 0);
        assert_eq!(mapper.prg_reg, 0);
        assert!(!mapper.irq_active);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8800, 3);
        mapper.write_prg(0xB000, 0x05);
        mapper.write_prg(0xB004, 0x02);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.prg_reg, 0);
        assert_eq!(restored.chr_regs[0], 0x25);
        assert_eq!(restored.read_prg(0x8000), 3);
    }
}
