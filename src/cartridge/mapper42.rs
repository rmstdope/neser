//! Mapper 042 - FDS Conversions (Ai Senshi Nicol, Bio Miracle Bokutte Upa)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_042>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 042 - FDS game conversions
///
/// Hardware: Used for hacked FDS games converted to cartridge form.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_042>
/// - PRG-ROM: 128 KiB (8 KiB switchable window at $6000-$7FFF, last 32 KiB fixed at $8000-$FFFF)
/// - PRG-RAM: None
/// - CHR: 128 KiB ROM or 8 KiB RAM (single 8 KiB switchable bank)
/// - Mirroring: Mapper controlled (H/V)
/// - IRQ: 15-bit CPU-cycle counter; asserted while counter >= $6000
///
/// Known games: Ai Senshi Nicol, Bio Miracle Bokutte Upa ("Mario Baby")
pub struct Mapper42 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
    irq_enabled: bool,
    irq_counter: u16,
    irq_pending: bool,
}

impl Mapper42 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB

    // IRQ: 15-bit counter; asserted when bits 14 and 13 are set (counter >= 0x6000)
    const IRQ_ASSERT_THRESHOLD: u16 = 0x6000;
    const IRQ_COUNTER_MASK: u16 = 0x7FFF; // 15-bit

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
            irq_enabled: false,
            irq_counter: 0,
            irq_pending: false,
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // $8000-$FFFF: fixed last 4 × 8KB banks
        self.base.select_prg_page(0, -4);
        self.base.select_prg_page(1, -3);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR: single 8KB bank
        self.base.select_chr_page(0, self.chr_bank as i16);
    }

    fn read_prg_6000(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        let bank_count = prg.len() / Self::PRG_BANK_SIZE;
        if bank_count == 0 {
            return 0;
        }
        let bank = (self.prg_bank as usize) % bank_count;
        let offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);
        prg.get(bank * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }
}

impl Mapper for Mapper42 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_6000(addr),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xE003 {
            0x8000 => {
                // CHR Select: bits 3:0
                self.chr_bank = value & 0x0F;
                self.update_banks();
            }
            0xE000 => {
                // PRG Select: bits 3:0
                self.prg_bank = value & 0x0F;
            }
            0xE001 => {
                // Mirroring: bit 3 (0=Vertical, 1=Horizontal)
                self.base.set_mirroring(if (value & 0x08) != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                });
            }
            0xE002 => {
                // IRQ Control: bit 1 (0=Disable+Ack+Reset, 1=Enable)
                self.irq_enabled = (value & 0x02) != 0;
                if !self.irq_enabled {
                    self.irq_pending = false;
                    self.irq_counter = 0;
                }
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.base.read_chr_banked(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.base.write_chr_banked(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.base.mirroring()
    }

    fn mapper_number(&self) -> u8 {
        self.base.mapper_number()
    }

    fn wram_size(&self) -> usize {
        // No WRAM: $6000-$7FFF is mapped to PRG ROM, not RAM.
        0
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }
        self.irq_counter = (self.irq_counter + 1) & Self::IRQ_COUNTER_MASK;
        self.irq_pending = self.irq_counter >= Self::IRQ_ASSERT_THRESHOLD;
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.base.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.base.restore_chr_ram(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.base.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout: [0] prg_bank, [1] chr_bank,
        //         [2] flags (irq_enabled | irq_pending<<1),
        //         [3-4] irq_counter (little-endian),
        //         [5] mirroring (0=Vertical, 1=Horizontal)
        let flags = (self.irq_enabled as u8) | ((self.irq_pending as u8) << 1);
        let mirror_byte = matches!(self.base.mirroring(), NametableLayout::Horizontal) as u8;
        vec![
            self.prg_bank,
            self.chr_bank,
            flags,
            (self.irq_counter & 0xFF) as u8,
            (self.irq_counter >> 8) as u8,
            mirror_byte,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.irq_enabled = (data[2] & 1) != 0;
            self.irq_pending = (data[2] & 2) != 0;
            self.irq_counter = (data[3] as u16) | ((data[4] as u16) << 8);
            self.base.set_mirroring(if data[5] != 0 {
                NametableLayout::Horizontal
            } else {
                NametableLayout::Vertical
            });
            self.update_banks();
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.base.capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper42;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use 11 banks (non-power-of-two) to prevent false-pass wrapping.
    const PRG_BANKS: usize = 11;
    const CHR_BANKS: usize = 11;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            42,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 42 should be implemented")
    }

    fn make_mapper_direct() -> Mapper42 {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        Mapper42::new(MapperContext::new_for_test(
            42,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    // --- Factory ---

    #[test]
    fn mapper_42_is_registered_in_factory() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            42,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 42 must be registered in the factory"
        );
    }

    // --- PRG banking ---

    #[test]
    fn prg_6000_window_defaults_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "$6000 window must default to PRG bank 0"
        );
    }

    #[test]
    fn prg_6000_window_is_switchable_via_e000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 3);
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "$6000 window must map to the bank selected by $E000 register"
        );
    }

    #[test]
    fn prg_6000_register_uses_only_low_nibble() {
        let mut mapper = make_mapper();
        // High nibble bits must be ignored; only bits 3:0 select the bank
        mapper.write_prg(0xE000, 0xF3); // high nibble 0xF should be ignored
        assert_eq!(
            mapper.read_prg(0x6000),
            3,
            "$E000 register must only use bits 3:0 for PRG bank selection"
        );
    }

    #[test]
    fn prg_8000_is_fixed_to_fourth_from_last_bank() {
        // With 11 banks, fourth-from-last is bank 7
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "$8000 window must be fixed to 4th-from-last PRG bank"
        );
    }

    #[test]
    fn prg_a000_is_fixed_to_third_from_last_bank() {
        // With 11 banks, third-from-last is bank 8
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            8,
            "$A000 window must be fixed to 3rd-from-last PRG bank"
        );
    }

    #[test]
    fn prg_c000_is_fixed_to_second_from_last_bank() {
        // With 11 banks, second-from-last is bank 9
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            9,
            "$C000 window must be fixed to 2nd-from-last PRG bank"
        );
    }

    #[test]
    fn prg_e000_is_fixed_to_last_bank() {
        // With 11 banks, last bank is 10
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            10,
            "$E000 window must be fixed to last PRG bank"
        );
    }

    #[test]
    fn prg_bank_offset_is_correct() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 2);
        // Last byte of the $6000 window should still be bank 2
        assert_eq!(
            mapper.read_prg(0x7FFF),
            2,
            "last byte of $6000 window must still read from the selected bank"
        );
    }

    // --- CHR banking ---

    #[test]
    fn chr_defaults_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must default to bank 0");
    }

    #[test]
    fn chr_bank_is_switchable_via_8000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must be selected by $8000 register"
        );
    }

    #[test]
    fn chr_register_uses_only_low_nibble() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xF3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$8000 register must only use bits 3:0 for CHR bank selection"
        );
    }

    #[test]
    fn chr_ram_is_writable_when_no_chr_rom() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let mut mapper = Mapper42::new(MapperContext::new_for_test(
            42,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ));
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_defaults_from_header() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let mapper = Mapper42::new(MapperContext::new_for_test(
            42,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_set_to_vertical_via_e001_register() {
        let mut mapper = make_mapper_direct();
        // bit 3 = 0 → Vertical
        mapper.write_prg(0xE001, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$E001 bit3=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn mirroring_set_to_horizontal_via_e001_register() {
        let mut mapper = make_mapper_direct();
        // bit 3 = 1 → Horizontal
        mapper.write_prg(0xE001, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$E001 bit3=1 must select Horizontal mirroring"
        );
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_initially() {
        let mapper = make_mapper_direct();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper_direct();
        for _ in 0..0x7FFF {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire while disabled");
    }

    #[test]
    fn irq_fires_after_0x6000_cpu_cycles_when_enabled() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE002, 0x02); // enable IRQ
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after 0x6000 CPU cycles when enabled"
        );
    }

    #[test]
    fn irq_does_not_fire_before_0x6000_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE002, 0x02); // enable IRQ
        for _ in 0..0x5FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must NOT fire before 0x6000 CPU cycles"
        );
    }

    #[test]
    fn irq_acknowledged_by_writing_0_to_e002() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE002, 0x02); // enable
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending before ack");
        mapper.write_prg(0xE002, 0x00); // disable + ack + reset
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after $E002 write with bit1=0"
        );
    }

    #[test]
    fn irq_counter_resets_on_disable() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE002, 0x02); // enable
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0xE002, 0x00); // disable + reset
        mapper.write_prg(0xE002, 0x02); // re-enable
        // 0x5FFF cycles should not fire
        for _ in 0..0x5FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire until 0x6000 cycles after re-enable (counter reset on disable)"
        );
    }

    #[test]
    fn irq_clears_after_counter_wraps_past_0x7fff() {
        // Counter is 15-bit; after 0x8000 cycles it wraps to 0, clearing IRQ
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE002, 0x02); // enable
        for _ in 0..0x8000 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after 15-bit counter wraps (0x8000 cycles)"
        );
    }

    // --- Registers snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = Mapper42::new(MapperContext::new_for_test(
            42,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0xE000, 2); // prg bank = 2
        mapper.write_prg(0x8000, 5); // chr bank = 5
        mapper.write_prg(0xE001, 0x08); // horizontal mirroring
        mapper.write_prg(0xE002, 0x02); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }

        let snap = mapper.registers_snapshot();
        let mut restored = Mapper42::new(MapperContext::new_for_test(
            42,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x6000), 2, "Restored PRG bank must match");
        assert_eq!(restored.read_chr(0x0000), 5, "Restored CHR bank must match");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "Restored mirroring must match"
        );
    }
}
