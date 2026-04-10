//! Mapper 153 - Bandai FCG (LZ93D50 with 8 KiB battery-backed WRAM)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_153>
//!
//! Only one game uses this mapper: Famicom Jump II: Saikyou no 7-nin.
//!
//! Memory map:
//! - CPU `$6000-$7FFF`: 8 KiB battery-backed WRAM (enable via `$800D`)
//! - CPU `$8000-$BFFF`: 16 KiB switchable PRG-ROM bank (inner × outer)
//! - CPU `$C000-$FFFF`: 16 KiB PRG-ROM fixed to the last bank of the outer block
//! - PPU `$0000-$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! Registers (all at `$8000-$800F`, mask `$800F`):
//!
//! | Address       | Description |
//! |---|---|
//! | `$8000-$8003` | Outer PRG-ROM bank select (bit 0 = 256 KiB outer bank) |
//! | `$8008`       | Inner 16 KiB PRG-ROM bank select |
//! | `$8009`       | Nametable mirroring |
//! | `$800A`       | IRQ control (copy latch → counter, ack, enable) |
//! | `$800B`       | IRQ latch low byte |
//! | `$800C`       | IRQ latch high byte |
//! | `$800D`       | PRG-RAM chip enable (bit 6) |
//!
//! Registers `$8009`–`$800C` behave the same as INES Mapper 016 submapper 5.
//!
//! Known Limitations:
//! - When WRAM is all-zero on first boot, Famicom Jump II freezes.  A soft
//!   reset will fix it (the game initialises save data in RAM on second boot).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 16 * 1024;
const OUTER_BANK_BLOCK: usize = 16; // 256 KB / 16 KB = 16 inner banks per outer bank

/// Mapper 153 – Bandai LZ93D50 with battery-backed 8 KiB WRAM
pub struct Mapper153 {
    base: BaseMapper,

    outer_bank: u8, // 1-bit; selects 256 KiB PRG-ROM region
    inner_bank: u8, // 4-bit; 16 KiB page within the outer region
    prg_ram_enabled: bool,

    irq: CpuCycleIrq,
    irq_latch: u16,
}

impl Mapper153 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        let mut mapper = Self {
            base,
            outer_bank: 0,
            inner_bank: 0,
            prg_ram_enabled: false,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownToZero),
            irq_latch: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let base = (self.outer_bank as i16) * (OUTER_BANK_BLOCK as i16);
        self.base.select_prg_page(0, base + self.inner_bank as i16);
        // Fixed to last bank of the outer 256 KB block
        self.base
            .select_prg_page(1, base + (OUTER_BANK_BLOCK as i16) - 1);
    }

    fn handle_register(&mut self, reg: u8, value: u8) {
        match reg {
            0x00..=0x03 => {
                // Outer PRG bank select (bit 0 = 256 KB outer block)
                self.outer_bank = value & 0x01;
            }
            0x04..=0x07 => {} // unused (CHR registers in FCG-1/2, but not here)
            0x08 => {
                // Inner 16 KB PRG bank
                self.inner_bank = value & 0x0F;
            }
            0x09 => {
                // Nametable mirroring
                self.base.set_mirroring(match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    3 => NametableLayout::SingleScreenUpper,
                    _ => unreachable!(),
                });
            }
            0x0A => {
                // IRQ control: acknowledge, copy latch → counter, set enable.
                // Match LZ93D50 behavior: enabling with a zero counter asserts
                // IRQ immediately after reload.
                let enabled = (value & 0x01) != 0;
                self.irq.acknowledge();
                self.irq.set_counter(self.irq_latch);
                self.irq.set_enabled(enabled);
                if enabled && self.irq_latch == 0 {
                    self.irq.set_pending(true);
                }
            }
            0x0B => {
                self.irq_latch = (self.irq_latch & 0xFF00) | (value as u16);
            }
            0x0C => {
                self.irq_latch = (self.irq_latch & 0x00FF) | ((value as u16) << 8);
            }
            0x0D => {
                // PRG-RAM chip enable: bit 6
                self.prg_ram_enabled = (value & 0x40) != 0;
            }
            _ => {}
        }
        self.update_banks();
    }
}

impl Mapper for Mapper153 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) && !self.prg_ram_enabled {
            return open_bus;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.prg_ram_enabled {
                return self.base.try_read_prg_ram(addr).unwrap_or(0);
            }
            return 0; // WRAM disabled; open-bus-aware callers use read_prg_open_bus
        }
        self.base.read_prg_rom(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.prg_ram_enabled {
                self.base.try_write_prg_ram(addr, value);
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let reg = (addr & 0x000F) as u8;
        self.handle_register(reg, value);
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn wram_size(&self) -> usize {
        self.base.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.base.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.base.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(8);
        snap.push(self.outer_bank);
        snap.push(self.inner_bank);
        snap.push(self.prg_ram_enabled as u8);
        let flags = (self.irq.enabled() as u8) | ((self.irq.is_pending() as u8) << 1);
        snap.push(flags);
        snap.push((self.irq.counter() & 0xFF) as u8);
        snap.push((self.irq.counter() >> 8) as u8);
        snap.push((self.irq_latch & 0xFF) as u8);
        snap.push((self.irq_latch >> 8) as u8);
        snap.push(self.base.mirroring().to_snapshot_byte());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 9 {
            return;
        }
        self.outer_bank = data[0] & 0x01;
        self.inner_bank = data[1] & 0x0F;
        self.prg_ram_enabled = data[2] != 0;
        self.irq.set_enabled((data[3] & 0x01) != 0);
        self.irq.set_pending((data[3] & 0x02) != 0);
        self.irq
            .set_counter((data[4] as u16) | ((data[5] as u16) << 8));
        self.irq_latch = (data[6] as u16) | ((data[7] as u16) << 8);
        self.base
            .set_mirroring(NametableLayout::from_snapshot_byte(data[8]));
        self.update_banks();
    }

    fn reset(&mut self) {
        self.outer_bank = 0;
        self.inner_bank = 0;
        self.prg_ram_enabled = false;
        self.irq = CpuCycleIrq::new(CpuCycleIrqMode::DownToZero);
        self.irq_latch = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 512 KB PRG (32 × 16KB banks) = 2 outer blocks × 16 inner banks
    const PRG_BANKS: usize = 32;

    fn make_mapper() -> Mapper153 {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        Mapper153::new(MapperContext::new_for_test(
            153,
            prg,
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_153_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            153,
            prg,
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 153 must be creatable via factory");
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg_8000_is_inner_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "inner bank 0 at $8000 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_fixed_to_outer_block_last() {
        let mapper = make_mapper();
        // outer=0, last inner = 15 → bank 15
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "$C000 must be fixed to bank 15 (last in outer block 0)"
        );
    }

    // --- Outer PRG bank ---

    #[test]
    fn outer_bank_register_selects_256k_block() {
        let mut mapper = make_mapper();
        // Select outer bank 1: $8000-$BFFF should be inner bank 0 of outer block 1 → bank 16
        mapper.write_prg(0x8000, 0x01); // outer bank select reg
        mapper.write_prg(0x8008, 0x00); // inner bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "outer=1, inner=0 → absolute bank 16"
        );
    }

    #[test]
    fn outer_bank_all_four_regs_select_same_bit() {
        let mut mapper = make_mapper();
        for reg_offset in 0..4u16 {
            mapper.write_prg(0x8000 + reg_offset, 0x00); // reset outer
            mapper.write_prg(0x8000 + reg_offset, 0x01); // set outer=1
            assert_eq!(
                mapper.read_prg(0x8000),
                16,
                "outer bank reg ${:04X} must set outer block",
                0x8000 + reg_offset
            );
        }
    }

    #[test]
    fn c000_fixed_to_last_bank_of_outer_block() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // outer=1
        // Last bank of outer block 1: 16 + 15 = 31
        assert_eq!(
            mapper.read_prg(0xC000),
            31,
            "$C000 must be fixed to last bank of the current outer block"
        );
    }

    // --- Inner PRG bank ---

    #[test]
    fn inner_bank_register_8008_selects_16k_page() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "inner bank 5 at $8000");
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_register_8009_controls_nametable_layout() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8009, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8009, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x8009, 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
        mapper.write_prg(0x8009, 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    // --- IRQ ---

    #[test]
    fn irq_fires_immediately_when_enabled_with_zero_latch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 0); // latch = 0
        mapper.write_prg(0x800C, 0);
        mapper.write_prg(0x800A, 1); // enable with counter=0 → immediate IRQ
        assert!(
            mapper.irq_pending(),
            "IRQ must assert immediately when enabled with zero counter"
        );
    }

    #[test]
    fn irq_fires_after_counter_counts_down_to_zero() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 3); // latch low = 3
        mapper.write_prg(0x800C, 0); // latch high = 0
        mapper.write_prg(0x800A, 1); // enable + copy latch

        assert!(!mapper.irq_pending(), "IRQ not pending before countdown");
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ must fire after 3 cycles");
    }

    #[test]
    fn irq_acknowledged_by_800a_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800B, 1);
        mapper.write_prg(0x800C, 0);
        mapper.write_prg(0x800A, 1);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        mapper.write_prg(0x800A, 0); // write to $800A acknowledges
        assert!(!mapper.irq_pending(), "IRQ must be cleared by $800A write");
    }

    // --- PRG-RAM (WRAM) ---

    #[test]
    fn prg_ram_disabled_by_default() {
        let mut mapper = make_mapper();
        // Write to RAM should be ignored; read should return open bus
        mapper.write_prg(0x6000, 0xAB);
        assert_ne!(
            mapper.read_prg(0x6000),
            0xAB,
            "WRAM must be disabled by default"
        );
    }

    #[test]
    fn prg_ram_enabled_by_bit6_of_800d() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800D, 0x40); // bit 6 = enable
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "WRAM reads must work when enabled"
        );
    }

    #[test]
    fn prg_ram_disabled_when_bit6_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800D, 0x40);
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x800D, 0x00); // disable
        assert_ne!(
            mapper.read_prg(0x6000),
            0xAB,
            "WRAM must be inaccessible when disabled"
        );
    }

    // --- CHR-RAM ---

    #[test]
    fn chr_ram_is_writable_and_readable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0xCD);
        assert_eq!(mapper.read_chr(0x0100), 0xCD, "CHR-RAM must be writable");
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // outer = 1
        mapper.write_prg(0x8008, 0x05); // inner = 5
        mapper.write_prg(0x8009, 2); // mirroring 1ScA
        mapper.write_prg(0x800D, 0x40); // RAM enable

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG bank preserved"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring preserved"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8008, 0x07);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "inner bank 0 at $8000 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            15,
            "last bank 15 at $C000 after reset"
        );
    }
}
