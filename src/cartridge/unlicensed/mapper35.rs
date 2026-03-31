//! Mapper 035 - J.Y. Company (simple variant)
//!
//! Specifications:
//! - Fallback: <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/JyCompany/Mapper35.h>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::A12RisingEdgeDetector;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 035 - J.Y. Company (simple variant)
///
/// Hardware: J.Y. Company ASIC (simplified variant distinct from mapper 90/211)
///
/// Specifications (Mesen2 reference):
/// - PRG-ROM: Up to 2 MiB (256 × 8 KiB banks)
/// - CHR: Up to 512 KiB (512 × 1 KiB banks)
/// - Mirroring: Programmable (H/V)
/// - IRQ: A12 (scanline-style) counter; fires when counter reaches 0
///
/// Register map:
/// - $8000-$8003: PRG bank select for windows 0-3 (bits 2-0 of address)
/// - $9000-$9007: CHR bank select for 1KB windows 0-7 (bits 2-0 of address)
/// - $C002: Disable IRQ and acknowledge
/// - $C003: Enable IRQ
/// - $C005: Load IRQ counter
/// - $D001: Mirroring; bit 0: 0=Vertical, 1=Horizontal
///
/// Power-on state: PRG window 3 fixed to last bank.
pub struct Mapper35 {
    base: BaseMapper,
    prg_regs: [u8; 4],
    chr_regs: [u8; 8],
    irq_counter: u8,
    irq_enabled: bool,
    irq_pending: bool,
    a12_detector: A12RisingEdgeDetector,
    multiplicand: u8,
    multiplier: u8,
}

impl Mapper35 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: ctx.prg_ram_banks_8k as usize * 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(NametableLayout::Vertical);

        let mut mapper = Self {
            base,
            prg_regs: [0; 4],
            chr_regs: [0; 8],
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            a12_detector: A12RisingEdgeDetector::new(3),
            multiplicand: 0,
            multiplier: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        for i in 0..3 {
            self.base.select_prg_page(i, self.prg_regs[i] as i16);
        }
        self.base.select_prg_page(3, -1);

        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_regs[i] as i16);
        }
    }

    fn clock_irq(&mut self) {
        if self.irq_enabled {
            if self.irq_counter == 0 {
                self.irq_enabled = false;
                self.irq_pending = true;
            } else {
                self.irq_counter -= 1;
                if self.irq_counter == 0 {
                    self.irq_enabled = false;
                    self.irq_pending = true;
                }
            }
        }
    }
}

impl Mapper for Mapper35 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5800 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                result as u8
            }
            0x5801 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                (result >> 8) as u8
            }
            _ => {
                if let Some(value) = self.base.try_read_prg_6000(addr) {
                    return value;
                }
                if let Some(value) = self.base.try_read_prg_ram(addr) {
                    return value;
                }
                match addr {
                    0x8000..=0xFFFF => self.base.read_prg_rom(addr),
                    _ => 0,
                }
            }
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5800 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                result as u8
            }
            0x5801 => {
                let result = (self.multiplicand as u16) * (self.multiplier as u16);
                (result >> 8) as u8
            }
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x5800 => {
                self.multiplicand = value;
                return;
            }
            0x5801 => {
                self.multiplier = value;
                return;
            }
            _ => {}
        }
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        let high = addr & 0xF000;
        let bit11_set = addr & 0x0800 != 0;

        match high {
            // PRG bank select — mask $F803 (bit 11 must be clear)
            0x8000 if !bit11_set => {
                let slot = (addr & 0x03) as usize;
                self.prg_regs[slot] = value;
                self.update_banks();
            }
            // CHR bank select LSB — mask $F807 (bit 11 must be clear)
            0x9000 if !bit11_set => {
                let slot = (addr & 0x07) as usize;
                self.chr_regs[slot] = value;
                self.update_banks();
            }
            // IRQ registers — mask $F007 (bit 11 does NOT matter)
            0xC000 => match addr & 0x07 {
                0x02 => {
                    self.irq_enabled = false;
                    self.irq_pending = false;
                }
                0x03 => {
                    self.irq_enabled = true;
                }
                0x05 => {
                    self.irq_counter = value;
                }
                _ => {}
            },
            // Mode registers — mask $F803 (bit 11 must be clear)
            0xD000 if !bit11_set => {
                match addr & 0x03 {
                    0x00 => {
                        // Mode register — currently only 8K PRG + 1K CHR mode is tested
                    }
                    0x01 => match value & 0x03 {
                        0 => self.base.set_mirroring(NametableLayout::Vertical),
                        1 => self.base.set_mirroring(NametableLayout::Horizontal),
                        2 => self.base.set_mirroring(NametableLayout::SingleScreenLower),
                        3 => self.base.set_mirroring(NametableLayout::SingleScreenUpper),
                        _ => unreachable!(),
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.a12_detector.update(addr) {
            self.clock_irq();
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12_detector.cpu_tick();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirror_byte = match self.base.mirroring() {
            NametableLayout::Horizontal => 1u8,
            _ => 0u8,
        };
        let flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        let mut v = vec![
            mirror_byte,
            flags,
            self.irq_counter,
            self.multiplicand,
            self.multiplier,
        ];
        v.extend_from_slice(&self.prg_regs);
        v.extend_from_slice(&self.chr_regs);
        v
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 17 {
            return;
        }
        self.base.set_mirroring_hv(data[0] != 0);
        let flags = data[1];
        self.irq_enabled = (flags & 1) != 0;
        self.irq_pending = (flags & 2) != 0;
        self.irq_counter = data[2];
        self.multiplicand = data[3];
        self.multiplier = data[4];
        self.prg_regs.copy_from_slice(&data[5..9]);
        self.chr_regs.copy_from_slice(&data[9..17]);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 4];
        self.chr_regs = [0; 8];
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.multiplicand = 0;
        self.multiplier = 0;
        self.a12_detector = A12RisingEdgeDetector::new(3);
        self.base.set_mirroring(NametableLayout::Vertical);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 11; // non-power-of-two to catch wrap bugs
    const CHR_1K_BANKS: usize = 13; // non-power-of-two

    fn make_mapper() -> Mapper35 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        Mapper35::new(MapperContext::new_for_test(
            35,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_35_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            35,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 35 must be registered in the factory"
        );
    }

    // --- PRG banking ---

    #[test]
    fn prg_window3_is_fixed_to_last_bank_at_poweron() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Window 3 ($E000) must be fixed to last bank at power-on"
        );
    }

    #[test]
    fn prg_write_8000_selects_window0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Write $8000 must select PRG window 0"
        );
    }

    #[test]
    fn prg_write_8001_selects_window1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "Write $8001 must select PRG window 1"
        );
    }

    #[test]
    fn prg_write_8002_selects_window2() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 7);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "Write $8002 must select PRG window 2"
        );
    }

    #[test]
    fn prg_window3_stays_fixed_last_after_write_8003() {
        let mut mapper = make_mapper();
        // Write to $8003 should be ignored (window 3 always last)
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Window 3 must remain fixed to last bank even after $8003 write"
        );
    }

    // --- CHR banking ---

    #[test]
    fn chr_write_9000_selects_chr_window0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "Write $9000 must select CHR window 0"
        );
    }

    #[test]
    fn chr_write_9007_selects_chr_window7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9007, 9);
        assert_eq!(
            mapper.read_chr(0x1C00),
            9,
            "Write $9007 must select CHR window 7"
        );
    }

    #[test]
    fn chr_all_windows_individually_selectable() {
        let mut mapper = make_mapper();
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

    // --- Mirroring ---

    #[test]
    fn mirroring_vertical_by_default() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Default mirroring must be Vertical"
        );
    }

    #[test]
    fn mirroring_d001_bit0_0_is_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_bit0_1_is_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_at_poweron() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn irq_fires_after_n_a12_rising_edges() {
        let mut mapper = make_mapper();
        // Load counter = 3, enable
        mapper.write_prg(0xC005, 3);
        mapper.write_prg(0xC003, 0); // enable

        let mut fired = false;
        for _ in 0..10 {
            // Drive A12 low then high (simulate scanline)
            mapper.ppu_address_changed(0x0000);
            for _ in 0..4 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000); // A12 rising edge
            if mapper.irq_pending() {
                fired = true;
                break;
            }
        }
        assert!(fired, "IRQ must fire after counter reaches zero");
    }

    #[test]
    fn irq_c002_disables_and_clears_irq() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC005, 1);
        mapper.write_prg(0xC003, 0);
        // Trigger one A12 rising edge
        mapper.ppu_address_changed(0x0000);
        for _ in 0..4 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
        assert!(
            mapper.irq_pending(),
            "IRQ must be pending after counter fires"
        );
        mapper.write_prg(0xC002, 0); // disable + ack
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after $C002 write"
        );
        assert!(
            !mapper.irq_enabled,
            "IRQ must be disabled after $C002 write"
        );
    }

    #[test]
    fn irq_does_not_fire_when_disabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC005, 1);
        // Do NOT enable IRQ
        for _ in 0..10 {
            mapper.ppu_address_changed(0x0000);
            for _ in 0..4 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when not enabled");
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut a = make_mapper();
        a.write_prg(0x8000, 2);
        a.write_prg(0x8001, 4);
        a.write_prg(0x8002, 6);
        a.write_prg(0x9000, 3);
        a.write_prg(0x9001, 5);
        a.write_prg(0xD001, 0x01); // horizontal
        a.write_prg(0xC005, 7);
        a.write_prg(0xC003, 0); // enable IRQ

        let snap = a.registers_snapshot();
        let mut b = make_mapper();
        b.restore_registers(&snap);

        assert_eq!(b.read_prg(0x8000), a.read_prg(0x8000));
        assert_eq!(b.read_prg(0xA000), a.read_prg(0xA000));
        assert_eq!(b.read_prg(0xC000), a.read_prg(0xC000));
        assert_eq!(b.read_chr(0x0000), a.read_chr(0x0000));
        assert_eq!(b.read_chr(0x0400), a.read_chr(0x0400));
        assert_eq!(b.get_mirroring(), a.get_mirroring());
        assert_eq!(b.irq_counter, a.irq_counter);
        assert_eq!(b.irq_enabled, a.irq_enabled);
    }
}
