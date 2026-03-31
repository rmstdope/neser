//! Mapper 69 - Sunsoft FME-7 (Sunsoft 5A/5B)
//!
//! Hardware: Sunsoft's advanced mapper with IRQ counter and optional expansion audio
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/Sunsoft_FME-7>
//! - Audio: <https://www.nesdev.org/wiki/Sunsoft_5B_audio> (5B variant only)
//! - PRG-ROM: Up to 512KB with 8KB banking
//! - PRG-RAM: Up to 512KB (unusual, can be banked at $6000-$7FFF)
//! - CHR: Up to 256KB (eight 1KB switchable banks)
//! - Mirroring: Programmable (horizontal, vertical, one-screen A/B)
//! - IRQ: 16-bit CPU-cycle countdown timer
//!
//! Common boards: Sunsoft FME-7 (5A without audio, 5B with audio)
//!
//! Memory Map:
//! - $6000-$7FFF: Bank 0 (can be PRG-RAM or PRG-ROM)
//! - $8000-$9FFF: Bank 1 (PRG-ROM)
//! - $A000-$BFFF: Bank 2 (PRG-ROM)
//! - $C000-$DFFF: Bank 3 (PRG-ROM)
//! - $E000-$FFFF: Bank 4 (PRG-ROM, usually fixed to last bank)
//!
//! Registers (two-step access):
//! 1. Write command number to $8000-$9FFF
//! 2. Write parameter to $A000-$BFFF
//!
//! Commands:
//! - $00-$07: CHR bank select (1KB each)
//! - $08-$0B: PRG bank select (8KB each at $6000, $8000, $A000, $C000)
//! - $0C: Mirroring control
//! - $0D: IRQ control (enable/disable, acknowledge)
//! - $0E: IRQ counter low byte
//! - $0F: IRQ counter high byte
//!
//! Notes:
//! - Used in Gimmick! (with 5B audio), Batman: Return of the Joker, Hebereke
//! - 5B variant adds YM2149F-compatible expansion audio (3 square waves)
//!
//! Known Limitations:
//! - **Expansion audio not implemented** (5B audio chip)

use crate::cartridge::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};
use crate::trace_mapper;

use crate::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

pub struct SunsoftFme7Mapper {
    base: BaseMapper,

    // Register selection
    command: u8,

    // PRG banking (4 x 8KB switchable banks)
    prg_banks: [u8; 4], // Banks for $6000-$7FFF, $8000-$9FFF, $A000-$BFFF, $C000-$DFFF
    // Derived from wram_chip_enabled && prg_slot_selects_ram; kept as a field so
    // read_prg/write_prg can check a single bool in the hot path. All three are
    // always updated together in write_parameter ($08).
    prg_ram_enabled: bool,
    // Bit 7 of reg $08: WRAM chip-enable line. Set means the chip is powered.
    wram_chip_enabled: bool,
    // Bit 6 of reg $08: RAM/ROM select. Set means the $6000-$7FFF slot routes to RAM.
    prg_slot_selects_ram: bool,

    // CHR banking (8 x 1KB banks)
    chr_banks: [u8; 8],

    // IRQ
    irq: CpuCycleIrq,
    irq_counter_enabled: bool,
}

impl SunsoftFme7Mapper {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x2000);
        base.configure_prg_6000_banking();
        base.configure_chr_banking(0x0400);
        let mut mapper = Self {
            base,
            command: 0,
            prg_banks: [0, 0, 0, 0],
            prg_ram_enabled: false,
            wram_chip_enabled: false,
            prg_slot_selects_ram: false,
            chr_banks: [0, 1, 2, 3, 4, 5, 6, 7],
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownUnderflow),
            irq_counter_enabled: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: $6000-$7FFF = switchable 8KB ROM bank (when RAM not enabled)
        self.base.select_prg_6000_page(self.prg_banks[0] as i16);
        // PRG: $8000-$FFFF = 4 x 8KB slots
        self.base.select_prg_page(0, self.prg_banks[1] as i16);
        self.base.select_prg_page(1, self.prg_banks[2] as i16);
        self.base.select_prg_page(2, self.prg_banks[3] as i16);
        self.base.select_prg_page(3, -1); // fixed last
        // CHR: 8 x 1KB slots
        for i in 0..8 {
            self.base.select_chr_page(i, self.chr_banks[i] as i16);
        }
    }

    fn write_command(&mut self, value: u8) {
        self.command = value & 0x0F;
        trace_mapper!(1; "[fme7] Command set to ${:02X}", self.command);
    }

    fn write_parameter(&mut self, value: u8) {
        trace_mapper!(1; "[fme7] Command ${:02X} <- ${:02X}", self.command, value);

        match self.command {
            0x00..=0x07 => {
                // CHR bank select (1KB each)
                self.chr_banks[self.command as usize] = value;
            }
            0x08 => {
                // PRG bank 0 ($6000-$7FFF)
                // Bit 7: RAM chip-enable (E) — 6264 +CE line
                // Bit 6: RAM/ROM select (R) — 0 = ROM, 1 = RAM
                // Bits 5-0: Bank number (applied even when RAM is selected)
                // RAM accessible only when BOTH E=1 AND R=1.
                let chip_enabled = (value & 0x80) != 0;
                let ram_selected = (value & 0x40) != 0;
                self.wram_chip_enabled = chip_enabled;
                self.prg_slot_selects_ram = ram_selected;
                self.prg_ram_enabled = chip_enabled && ram_selected;
                self.prg_banks[0] = value & 0x3F;
            }
            0x09..=0x0B => {
                // PRG banks 1-3 ($8000-$9FFF, $A000-$BFFF, $C000-$DFFF)
                let bank_index = (self.command - 0x09 + 1) as usize;
                self.prg_banks[bank_index] = value & 0x3F;
            }
            0x0C => {
                // Mirroring
                self.base.set_mirroring(match value & 0x03 {
                    0 => NametableLayout::Vertical,
                    1 => NametableLayout::Horizontal,
                    2 => NametableLayout::SingleScreenLower,
                    3 => NametableLayout::SingleScreenUpper,
                    _ => unreachable!(),
                });
            }
            0x0D => {
                // IRQ control
                // Per NESdev: "All writes to this register acknowledge an active IRQ."
                self.irq.acknowledge();
                self.irq.set_enabled((value & 0x01) != 0);
                self.irq_counter_enabled = (value & 0x80) != 0;

                trace_mapper!(1; "[fme7] IRQ enabled={}, counter_enabled={}", 
                    self.irq.enabled(), self.irq_counter_enabled);
            }
            0x0E => {
                // IRQ counter low byte
                self.irq
                    .set_counter((self.irq.counter() & 0xFF00) | (value as u16));
                trace_mapper!(1; "[fme7] IRQ counter low <- ${:02X}, counter now ${:04X}", 
                    value, self.irq.counter());
            }
            0x0F => {
                // IRQ counter high byte
                self.irq
                    .set_counter((self.irq.counter() & 0x00FF) | ((value as u16) << 8));
                trace_mapper!(1; "[fme7] IRQ counter high <- ${:02X}, counter now ${:04X}", 
                    value, self.irq.counter());
            }
            _ => {}
        }
        self.update_banks();
    }

    /// Compute the wrapped PRG-RAM byte offset for a CPU address in $6000–$7FFF.
    ///
    /// FME-7 always drives the bank-number address lines even when RAM is selected,
    /// so bank N maps to offset `N * 8KB + page_offset`. The offset wraps modulo
    /// total RAM size when N >= the physical bank count.
    fn banked_ram_offset(&self, addr: u16) -> usize {
        let raw_offset = (self.prg_banks[0] as usize * 0x2000) + (addr as usize - 0x6000);
        let ram_size = self.base.wram_size();
        if ram_size > 0 {
            raw_offset % ram_size
        } else {
            raw_offset
        }
    }
}

impl Mapper for SunsoftFme7Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.base
                        .read_prg_ram_at_offset(self.banked_ram_offset(addr))
                } else {
                    self.base.try_read_prg_6000(addr).unwrap_or(0)
                }
            }
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        // Per NESdev FME-7 spec: when RAM is selected but the chip is disabled
        // ($40-$7F in reg $08), neither RAM nor ROM drives the bus.
        if matches!(addr, 0x6000..=0x7FFF) && self.prg_slot_selects_ram && !self.wram_chip_enabled {
            return open_bus;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // PRG-RAM writes (when RAM is fully enabled: E=1 AND R=1)
                if self.prg_ram_enabled {
                    let offset = self.banked_ram_offset(addr);
                    self.base.write_prg_ram_at_offset(offset, value);
                }
            }
            0x8000..=0x9FFF => {
                // Command register
                self.write_command(value);
            }
            0xA000..=0xBFFF => {
                // Parameter register
                self.write_parameter(value);
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        // IRQ counter decrements every CPU cycle when counter enable (bit 7) is set
        // IRQ triggers on underflow only if IRQ enable (bit 0) is also set
        if self.irq_counter_enabled {
            self.irq.tick();
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Serialize Sunsoft FME-7 internal registers:
        // [0]: command
        // [1-4]: prg_banks[0-3]
        // [5-12]: chr_banks[0-7]
        // [13]: flags byte — original IRQ bits preserved at 2-4 for save-state
        //         compatibility; new WRAM bits placed in previously-unused positions:
        //         bit 0 = prg_ram_enabled (E=1 AND R=1)
        //         bit 1 = wram_chip_enabled (E bit)   ← NEW (was unused)
        //         bit 2 = irq_enabled                 ← UNCHANGED from original
        //         bit 3 = irq_counter_enabled         ← UNCHANGED from original
        //         bit 4 = irq_pending                 ← UNCHANGED from original
        //         bit 5 = prg_slot_selects_ram (R bit) ← NEW (was unused)
        // [14-15]: irq_counter (little endian)
        // [16]: mirroring
        let mut snapshot = Vec::with_capacity(17);
        snapshot.push(self.command);
        snapshot.extend_from_slice(&self.prg_banks);
        snapshot.extend_from_slice(&self.chr_banks);
        let flags = (self.prg_ram_enabled as u8)
            | ((self.wram_chip_enabled as u8) << 1)
            | ((self.irq.enabled() as u8) << 2)
            | ((self.irq_counter_enabled as u8) << 3)
            | ((self.irq.is_pending() as u8) << 4)
            | ((self.prg_slot_selects_ram as u8) << 5);
        snapshot.push(flags);
        snapshot.push((self.irq.counter() & 0xFF) as u8);
        snapshot.push((self.irq.counter() >> 8) as u8);
        snapshot.push(self.base.mirroring().to_snapshot_byte());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            self.command = data[0];
            self.prg_banks.copy_from_slice(&data[1..5]);
            self.chr_banks.copy_from_slice(&data[5..13]);
            let flags = data[13];
            self.prg_ram_enabled = (flags & 0x01) != 0;
            self.wram_chip_enabled = (flags & 0x02) != 0;
            self.irq.set_enabled((flags & 0x04) != 0);
            self.irq_counter_enabled = (flags & 0x08) != 0;
            self.irq.set_pending((flags & 0x10) != 0);
            self.prg_slot_selects_ram = (flags & 0x20) != 0;
            self.irq
                .set_counter((data[14] as u16) | ((data[15] as u16) << 8));
            self.base
                .set_mirroring(NametableLayout::from_snapshot_byte(data[16]));
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    #[test]
    fn test_mapper_69_is_wired_in_factory() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 69 should be implemented");
    }

    #[test]
    fn test_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 16); // 16 x 8KB banks
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Set bank 1 ($8000-$9FFF) to bank 5
        mapper.write_prg(0x8000, 0x09); // Command 9 = PRG bank 1
        mapper.write_prg(0xA000, 0x05); // Parameter = bank 5

        // Read from $8000 should return data from bank 5
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_chr_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 64); // 64 x 1KB banks
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Set CHR bank 0 to bank 10
        mapper.write_prg(0x8000, 0x00); // Command 0 = CHR bank 0
        mapper.write_prg(0xA000, 0x0A); // Parameter = bank 10

        // Read from $0000 should return data from bank 10
        assert_eq!(mapper.read_chr(0x0000), 10);
    }

    #[test]
    fn test_prg_ram_access() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Initially RAM is disabled: write is ignored, $6000 returns ROM bank 0
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0); // ROM bank 0

        // Enable RAM: requires BOTH bit 7 (chip-enable) AND bit 6 (RAM select)
        mapper.write_prg(0x8000, 0x08); // Command 8 = PRG bank 0
        mapper.write_prg(0xA000, 0xC0); // E=1, R=1 → writable RAM

        // Now writes should work
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    /// FME-7 has no write-protect mode: when RAM is enabled ($C0), writes always succeed.
    /// Switching to $80 (E=1, R=0) exposes ROM at $6000 instead of RAM.
    #[test]
    fn test_prg_ram_switching_between_ram_and_rom() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Enable RAM with $C0 (E=1, R=1) and write
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0); // RAM enabled + selected
        mapper.write_prg(0x6000, 0x42);
        assert_eq!(mapper.read_prg(0x6000), 0x42);

        // Switch to ROM ($80: E=1, R=0) — $6000 maps to ROM bank 0
        mapper.write_prg(0xA000, 0x80);
        assert_eq!(mapper.read_prg(0x6000), 0); // ROM bank 0, not RAM

        // Back to RAM ($C0): previous write preserved
        mapper.write_prg(0xA000, 0xC0);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_mirroring() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Change to vertical
        mapper.write_prg(0x8000, 0x0C); // Command C = mirroring
        mapper.write_prg(0xA000, 0x00); // 0 = vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Change to single screen lower
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x02); // 2 = single screen lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_irq_countdown() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Set counter to 10
        mapper.write_prg(0x8000, 0x0E); // Command E = IRQ counter low
        mapper.write_prg(0xA000, 0x0A); // Counter = 10
        mapper.write_prg(0x8000, 0x0F); // Command F = IRQ counter high
        mapper.write_prg(0xA000, 0x00); // Counter high = 0

        // Enable IRQ and counter
        mapper.write_prg(0x8000, 0x0D); // Command D = IRQ control
        mapper.write_prg(0xA000, 0x81); // Enable IRQ (bit 0) and counter (bit 7)

        assert!(!mapper.irq_pending());

        // Countdown should decrement each CPU cycle
        // After 10 cycles, counter goes from 10 -> 9 -> ... -> 1 -> 0
        for _ in 0..10 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }

        // After one more cycle, counter underflows from 0 to $FFFF and IRQ triggers
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
    }

    #[test]
    fn test_irq_disabled() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Set counter to 5
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0xA000, 0x05);
        mapper.write_prg(0x8000, 0x0F);
        mapper.write_prg(0xA000, 0x00);

        // Enable counter but not IRQ
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x80); // Counter enabled (bit 7), IRQ disabled (bit 0 = 0)

        // Counter should count down even with IRQ disabled
        // After 5 cycles: 5->4->3->2->1->0
        for _ in 0..5 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending()); // IRQ should not trigger
        }

        // After one more cycle, counter underflows to $FFFF but IRQ still shouldn't trigger
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());

        // Now enable IRQ - it should trigger immediately since counter already underflowed
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x81); // Enable both counter and IRQ

        // IRQ should not trigger immediately on enable (counter is already at $FFFF-1 or similar)
        // But on next underflow it will trigger
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn test_last_prg_bank_fixed() {
        let prg_rom = banked_data(8 * 1024, 16); // 16 x 8KB banks
        let chr_rom = banked_data(1024, 8);
        let mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // $E000-$FFFF should read from last bank (bank 15)
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    #[test]
    fn test_fme7_registers_snapshot_restores_mirroring_and_banks() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // Set PRG bank 1 and CHR bank 0.
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0xA000, 0x03);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0xA000, 0x04);

        // Set mirroring to single screen upper.
        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0xA000, 0x03);

        let regs = mapper.registers_snapshot();

        let mut restored = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&regs);

        assert_eq!(restored.get_mirroring(), NametableLayout::SingleScreenUpper);
        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 4);
    }

    // -----------------------------------------------------------------------
    // WRAM snapshot/restore
    // -----------------------------------------------------------------------

    #[test]
    fn test_wram_snapshot_round_trip() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        ));

        // Enable RAM (command 8, bit 7 = chip-enable AND bit 6 = RAM select)
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0); // E=1, R=1 → writable RAM

        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x6100, 0x22);
        mapper.write_prg(0x7FFF, 0x33);
        let snapshot = mapper.wram_snapshot();
        assert_eq!(snapshot.len(), 8192);
        assert_eq!(snapshot[0x0000], 0x11);
        assert_eq!(snapshot[0x0100], 0x22);
        assert_eq!(snapshot[0x1FFF], 0x33);

        let mut mapper2 = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        mapper2.load_wram_snapshot(&snapshot);
        // Enable RAM to read back
        mapper2.write_prg(0x8000, 0x08);
        mapper2.write_prg(0xA000, 0xC0); // E=1, R=1 → writable RAM
        assert_eq!(mapper2.read_prg(0x6000), 0x11);
        assert_eq!(mapper2.read_prg(0x6100), 0x22);
        assert_eq!(mapper2.read_prg(0x7FFF), 0x33);
    }

    // -----------------------------------------------------------------------
    // Bug-regression tests: correct register $08 and $0D behaviour per spec
    // -----------------------------------------------------------------------

    /// Per NESdev: bit 7 = RAM chip-enable (E), bit 6 = RAM/ROM select (R).
    /// RAM is accessible only when BOTH bits are set ($C0 or higher with bits 7+6).
    /// $C0 → E=1, R=1 → writable RAM at $6000.
    #[test]
    fn test_prg_ram_c0_value_enables_writable_ram() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0); // E=1, R=1 → writable RAM

        mapper.write_prg(0x6000, 0x55);
        assert_eq!(mapper.read_prg(0x6000), 0x55);
    }

    /// $80 → E=1, R=0 → ROM at $6000 (not RAM).
    #[test]
    fn test_prg_ram_80_value_maps_rom_not_ram() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Pre-load known data into RAM via snapshot so the test doesn't
        // depend on write-enable being correct yet.
        let snapshot = vec![0xAA_u8; 8192];
        mapper.load_wram_snapshot(&snapshot);

        // With $80: E=1, R=0 → ROM at $6000, not RAM
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x80);
        // ROM bank 0, byte 0 = 0 (banked_data pattern), not RAM value $AA
        assert_eq!(mapper.read_prg(0x6000), 0);
    }

    /// Per NESdev: "All writes to this register acknowledge an active IRQ."
    /// Writing to $0D with bit 0 = 0 (IRQ disabled) must still clear a
    /// pending IRQ — the current code's `if self.irq.enabled()` guard is wrong.
    #[test]
    fn test_irq_reg_0d_any_write_acknowledges() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Configure counter = 5 and fire IRQ
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0xA000, 0x05); // counter low = 5
        mapper.write_prg(0x8000, 0x0F);
        mapper.write_prg(0xA000, 0x00); // counter high = 0
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x81); // enable counter (bit 7) + enable IRQ (bit 0)
        for _ in 0..6 {
            mapper.cpu_cycle(); // counter 5→4→3→2→1→0→underflow
        }
        assert!(
            mapper.irq_pending(),
            "IRQ should be pending before acknowledge"
        );

        // Write $0D = $80 (IRQ disabled, counter enabled) — must acknowledge
        mapper.write_prg(0x8000, 0x0D);
        mapper.write_prg(0xA000, 0x80);
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared by any write to $0D"
        );
    }

    /// When 32KB of RAM is provided (4 banks of 8KB), each bank must be
    /// independently addressable via bits 5-0 of register $08.
    #[test]
    fn test_prg_ram_banked_access_with_32kb_ram() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(
            MapperContext::new_for_test(69, prg_rom, chr_rom, NametableLayout::Horizontal)
                .with_prg_ram_banks(4), // 4 × 8KB = 32KB
        );

        // Write distinct values to bank 0 and bank 1
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0); // bank 0, RAM enabled+selected
        mapper.write_prg(0x6000, 0xAA);

        mapper.write_prg(0xA000, 0xC1); // bank 1, RAM enabled+selected
        mapper.write_prg(0x6000, 0xBB);

        // Read back bank 0: must return 0xAA, not 0xBB
        mapper.write_prg(0xA000, 0xC0);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        // Read back bank 1: must return 0xBB
        mapper.write_prg(0xA000, 0xC1);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);
    }

    /// When the mapper's bank index exceeds the physical bank count, accesses
    /// must wrap modulo the RAM size — hardware drives the address lines and the
    /// RAM chip ignores the out-of-range upper bits.
    /// E.g. with 2 × 8KB banks (16KB): bank 2 wraps to bank 0.
    #[test]
    fn test_prg_ram_bank_index_wraps_at_ram_size() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(
            MapperContext::new_for_test(69, prg_rom, chr_rom, NametableLayout::Horizontal)
                .with_prg_ram_banks(2), // 2 × 8KB = 16KB, so bank 2 wraps to bank 0
        );

        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0); // bank 0, RAM enabled+selected
        mapper.write_prg(0x6000, 0xAA);

        // Bank 2 should wrap to physical bank 0 (2 mod 2 = 0)
        mapper.write_prg(0xA000, 0xC2); // bank 2
        mapper.write_prg(0x6000, 0xBB); // overwrites physical bank 0

        // Reading bank 0 must see BB (overwritten via bank 2)
        mapper.write_prg(0xA000, 0xC0);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);

        // Reading bank 2 must also see BB (same physical location)
        mapper.write_prg(0xA000, 0xC2);
        assert_eq!(mapper.read_prg(0x6000), 0xBB);
    }

    // --- WRAM chip-enable (E bit) open-bus tests ---

    /// When E=0 (chip-enable bit 7 of reg $08 is clear), $6000-$7FFF should
    /// return open bus — neither RAM nor ROM is driven onto the data bus.
    /// Per NESdev FME-7 spec: E=0 → open bus; E=1,R=0 → ROM; E=1,R=1 → RAM.
    #[test]
    fn test_wram_chip_disabled_returns_open_bus_not_ram_data() {
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Enable WRAM (E=1, R=1 = $C0) and write a sentinel value
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0xC0);
        mapper.write_prg(0x6000, 0x5A);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x5A,
            "precondition: RAM write visible"
        );

        // Disable chip-enable: E=0, R=1 ($40) — neither RAM nor ROM mapped
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x40);

        // When reading $6000-$7FFF via open-bus path
        let open_bus_value: u8 = 0xFF;
        let result = mapper.read_prg_open_bus(0x6000, open_bus_value);

        // Then: open bus is returned, not the written RAM value
        assert_eq!(result, open_bus_value, "E=0 should return open bus");
        assert_ne!(result, 0x5A, "E=0 must not expose RAM data");
    }

    #[test]
    fn test_wram_chip_disabled_with_rom_select_returns_open_bus() {
        // E=0, R=1 ($40-$7F): RAM selected but chip disabled → open bus.
        // Per NESdev spec: "Open bus occurs if the RAM/ROM Select Bit is 1 (RAM selected),
        // but the RAM Enable Bit is 0 (disabled), i.e. any value in the range $40-$7F."
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // $40: E=0, R=1 → open bus
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x40); // E=0, R=1

        let open_bus_value: u8 = 0xAB;
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus_value),
            open_bus_value,
            "E=0,R=1 ($40) should return open bus"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus_value),
            open_bus_value,
            "E=0,R=1 open bus at $7FFF"
        );

        // $00: E=0, R=0 → ROM bank at $6000 (NOT open bus)
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x00); // E=0, R=0 → ROM
        let result = mapper.read_prg_open_bus(0x6000, open_bus_value);
        assert_ne!(
            result, open_bus_value,
            "E=0,R=0 ($00) should map ROM not return open bus"
        );
    }

    #[test]
    fn test_wram_rom_select_still_reads_rom_when_chip_enabled() {
        // E=1, R=0 ($80): chip enabled but ROM selected — $6000 maps to ROM bank
        // This verifies that the E=1/R=0 path still returns ROM, not open bus.
        let prg_rom = banked_data(8 * 1024, 16);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = SunsoftFme7Mapper::new(MapperContext::new_for_test(
            69,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0xA000, 0x80); // E=1, R=0 → ROM bank 0

        let open_bus_value: u8 = 0xFF;
        // ROM bank 0 byte 0 is known (banked_data fills each bank with its index)
        let result = mapper.read_prg_open_bus(0x6000, open_bus_value);
        // Should NOT return open bus — chip is enabled, ROM is mapped
        assert_ne!(
            result, open_bus_value,
            "E=1,R=0 should NOT return open bus — ROM is mapped"
        );
    }
}
