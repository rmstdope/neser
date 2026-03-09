//! Mapper 105 - NES-EVENT (Nintendo World Championships)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES-EVENT>
//! - Mapper family: MMC1-derived serial register interface
//!
//! Known Limitations:
//! - IRQ timing is modeled as a configurable CPU-cycle countdown driven by CHR bank 0 register writes.
//!   Real hardware uses an event-board timer circuit with DIP-switch-selected duration.
//! - This implementation focuses on mapper-level correctness needed for bank/mirroring control and IRQ
//!   signaling, not full discrete-logic cycle accuracy.

use crate::cartridge::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::mmc1::MMC1Mapper;

/// Mapper 105 (NES-EVENT).
///
/// This mapper reuses MMC1 banking/mirroring behavior and adds a CPU-cycle IRQ timer.
/// The timer is controlled through CHR bank 0 register writes:
/// - bit 4 set: timer reset/disabled, pending IRQ cleared
/// - bit 4 clear: timer armed
/// - bits 0-3: countdown value (in CPU cycles, with 0 treated as 1)
pub struct Mapper105 {
    inner: MMC1Mapper,
    irq_counter: u32,
    irq_reload: u32,
    irq_enabled: bool,
    irq_pending: bool,
    last_chr_bank_0: u8,
}

impl Mapper105 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mut inner = MMC1Mapper::new(ctx);
        Self::force_chr_i_bit_high(&mut inner);
        let chr_bank_0 = Self::chr_bank_0_from_mapper(&inner);

        let mut mapper = Self {
            inner,
            irq_counter: 0,
            irq_reload: 0,
            irq_enabled: false,
            irq_pending: false,
            last_chr_bank_0: chr_bank_0,
        };
        mapper.apply_timer_control(chr_bank_0);
        mapper
    }

    fn force_chr_i_bit_high(inner: &mut MMC1Mapper) {
        let mut regs = inner.registers_snapshot();
        if regs.len() >= 4 {
            regs[3] |= 0x10;
            inner.restore_registers(&regs);
        }
    }

    fn chr_bank_0_from_mapper(inner: &MMC1Mapper) -> u8 {
        inner.registers_snapshot().get(3).copied().unwrap_or(0) & 0x1F
    }

    fn apply_timer_control(&mut self, chr_bank_0: u8) {
        self.irq_reload = (chr_bank_0 & 0x0F) as u32;
        if (chr_bank_0 & 0x10) != 0 {
            self.irq_enabled = false;
            self.irq_pending = false;
            self.irq_counter = 0;
        } else {
            self.irq_enabled = true;
            self.irq_pending = false;
            self.irq_counter = self.irq_reload.max(1);
        }
    }

    fn maybe_update_timer_from_chr_register(&mut self) {
        let chr_bank_0 = Self::chr_bank_0_from_mapper(&self.inner);
        if chr_bank_0 != self.last_chr_bank_0 {
            self.last_chr_bank_0 = chr_bank_0;
            self.apply_timer_control(chr_bank_0);
        }
    }

    fn tick_irq_timer(&mut self) {
        if !self.irq_enabled {
            return;
        }
        if self.irq_counter > 0 {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 {
            self.irq_pending = true;
            self.irq_enabled = false;
        }
    }
}

impl Mapper for Mapper105 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
        self.maybe_update_timer_from_chr_register();
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.inner.write_chr(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.inner.read_chr(addr)
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
        self.tick_irq_timer();
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn get_mirroring(&self) -> crate::cartridge::NametableLayout {
        self.inner.get_mirroring()
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(16 + self.inner.registers_snapshot().len());
        snap.push(self.irq_enabled as u8);
        snap.push(self.irq_pending as u8);
        snap.push(self.last_chr_bank_0);
        snap.push(self.irq_counter as u8);
        snap.push((self.irq_counter >> 8) as u8);
        snap.push((self.irq_counter >> 16) as u8);
        snap.push((self.irq_counter >> 24) as u8);
        snap.push(self.irq_reload as u8);
        snap.push((self.irq_reload >> 8) as u8);
        snap.push((self.irq_reload >> 16) as u8);
        snap.push((self.irq_reload >> 24) as u8);
        snap.extend(self.inner.registers_snapshot());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 11 {
            self.irq_enabled = data[0] != 0;
            self.irq_pending = data[1] != 0;
            self.last_chr_bank_0 = data[2] & 0x1F;
            self.irq_counter = u32::from_le_bytes([data[3], data[4], data[5], data[6]]);
            self.irq_reload = u32::from_le_bytes([data[7], data[8], data[9], data[10]]);
            self.inner.restore_registers(&data[11..]);
            self.maybe_update_timer_from_chr_register();
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        Self::force_chr_i_bit_high(&mut self.inner);
        self.last_chr_bank_0 = Self::chr_bank_0_from_mapper(&self.inner);
        self.apply_timer_control(self.last_chr_bank_0);
    }

    fn capabilities(&self) -> MapperCapabilities {
        let mut caps = self.inner.capabilities();
        caps.has_irq = true;
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_16K: usize = 11;
    const CHR_BANKS_4K: usize = 9;

    fn write_mmc1_register<M: Mapper + ?Sized>(mapper: &mut M, addr: u16, value: u8) {
        for bit in 0..5 {
            mapper.cpu_cycle();
            mapper.cpu_cycle();
            mapper.write_prg(addr, (value >> bit) & 0x01);
        }
    }

    fn make_mapper() -> Box<dyn Mapper> {
        let prg_rom = banked_data(16 * 1024, PRG_BANKS_16K);
        let chr_rom = banked_data(4 * 1024, CHR_BANKS_4K);
        create_mapper(MapperContext::new_for_test(
            105,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 105 should be implemented")
    }

    #[test]
    fn mapper_105_is_registered() {
        let prg_rom = banked_data(16 * 1024, PRG_BANKS_16K);
        let chr_rom = banked_data(4 * 1024, CHR_BANKS_4K);
        let result = create_mapper(MapperContext::new_for_test(
            105,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 105 must be registered");
    }

    #[test]
    fn mapper_105_prg_bank_switching_matches_mmc1_mode_3_at_8000_bfff() {
        let mut mapper = make_mapper();
        write_mmc1_register(mapper.as_mut(), 0xE000, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn mapper_105_mirroring_modes_are_selectable_via_control_register() {
        let mut mapper = make_mapper();

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00001);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00010);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00011);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mapper_105_irq_fires_after_configured_cycle_count() {
        let mut mapper = make_mapper();

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00011);
        assert!(!mapper.irq_pending());

        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "IRQ should not fire before countdown");

        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ should fire when countdown reaches zero");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10011);
        assert!(
            !mapper.irq_pending(),
            "setting timer control bit must clear pending IRQ"
        );
    }
}
