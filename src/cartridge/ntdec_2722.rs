//! Mapper 040 - NTDEC 2722 (Super Mario Bros. 2 Japanese)
//!
//! Known Limitations:
//! - Submapper 1 (NTDEC 2752 multicart outer bank register) not implemented.
//!
//! Specification: <https://www.nesdev.org/wiki/INES_Mapper_040>

use crate::cartridge::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities};

use super::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

/// Mapper 040 - NTDEC 2722
///
/// Hardware: NTDEC 2722 PCB, used in Super Mario Bros. 2 (Japanese) conversions.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_040>
/// - PRG-ROM: 64KB (8 × 8KB banks, with specific fixed and switchable windows)
/// - CHR: 8KB fixed CHR-ROM (no banking)
/// - Mirroring: Fixed from ROM header
///
/// PRG window map (8KB each):
/// - $6000-$7FFF: Fixed bank 6
/// - $8000-$9FFF: Fixed bank 4
/// - $A000-$BFFF: Fixed bank 5
/// - $C000-$DFFF: Switchable (via $E000 write)
/// - $E000-$FFFF: Fixed bank 7
///
/// Register map (range $8000-$FFFF, mask $E000):
/// - $8000: Disable and acknowledge IRQ
/// - $A000: Enable IRQ (counter resets to 0, starts counting)
/// - $E000: Select 8KB PRG bank for $C000 window (bits 2:0)
///
/// IRQ: CD4020 CPU-cycle counter; fires after 4096 cycles, self-acks at 8192.
///
/// Known games: Super Mario Bros. 2 (Japanese, aka The Lost Levels)
pub struct Ntdec2722Mapper {
    base: BaseMapper,
    /// Switchable bank index for the $C000-$DFFF window.
    prg_bank: u8,
    irq: CpuCycleIrq,
}

impl Ntdec2722Mapper {
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB

    // Fixed PRG window bank assignments (hardwired per spec)
    const BANK_AT_6000: usize = 6;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::UpSelfAck {
                fire_count: 4096,
                ack_count: 8192,
            }),
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // $8000=bank4, $A000=bank5, $C000=switchable, $E000=bank7
        self.base.select_prg_page(0, 4);
        self.base.select_prg_page(1, 5);
        self.base.select_prg_page(2, self.prg_bank as i16);
        self.base.select_prg_page(3, 7);
    }

    fn read_prg_6000(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        let bank_count = prg.len() / Self::PRG_BANK_SIZE;
        if bank_count == 0 {
            return 0;
        }
        let bank = Self::BANK_AT_6000 % bank_count;
        let offset = (addr as usize) & (Self::PRG_BANK_SIZE - 1);
        prg.get(bank * Self::PRG_BANK_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }
}

impl Mapper for Ntdec2722Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_6000(addr),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xE000 {
            0x8000 => {
                // Disable and acknowledge IRQ
                self.irq.set_enabled(false);
                self.irq.acknowledge();
                self.irq.set_counter(0);
            }
            0xA000 => {
                // Enable IRQ, reset counter
                self.irq.set_enabled(true);
                self.irq.set_counter(0);
            }
            0xE000 => {
                // Select 8KB PRG bank for $C000 window
                self.prg_bank = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn wram_size(&self) -> usize {
        // No WRAM: $6000-$7FFF is mapped to PRG ROM bank 6, not RAM.
        0
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.irq.enabled() as u8) | ((self.irq.is_pending() as u8) << 1);
        vec![
            self.prg_bank,
            flags,
            (self.irq.counter() & 0xFF) as u8,
            (self.irq.counter() >> 8) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.prg_bank = data[0];
            self.irq.set_enabled((data[1] & 1) != 0);
            self.irq.set_pending((data[1] & 2) != 0);
            self.irq
                .set_counter((data[2] as u16) | ((data[3] as u16) << 8));
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ntdec2722Mapper;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 11 banks × 8KB = non-power-of-two to prevent false-pass wrapping.
    const PRG_BANKS: usize = 11;
    const CHR_BANKS: usize = 1; // 8KB fixed

    fn make_mapper() -> Box<dyn Mapper> {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            40,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 40 should be implemented")
    }

    fn make_mapper_direct() -> Ntdec2722Mapper {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        Ntdec2722Mapper::new(MapperContext::new_for_test(
            40,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    // --- Factory ---

    #[test]
    fn mapper_40_is_registered_in_factory() {
        // create_mapper should return Some for mapper id 40
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            40,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 40 must be registered in the factory"
        );
    }

    // --- PRG banking ---

    #[test]
    fn prg_bank4_is_fixed_at_8000() {
        // With 11-bank ROM, bank 4 is filled with 0x04.
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 window must be hardwired to PRG bank 4"
        );
    }

    #[test]
    fn prg_bank5_is_fixed_at_a000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 window must be hardwired to PRG bank 5"
        );
    }

    #[test]
    fn prg_bank6_is_fixed_at_6000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            6,
            "$6000 window must be hardwired to PRG bank 6"
        );
    }

    #[test]
    fn prg_bank7_is_fixed_at_e000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 window must be hardwired to PRG bank 7"
        );
    }

    #[test]
    fn prg_c000_is_switchable_via_e000_register() {
        let mut mapper = make_mapper();
        // Select bank 3 for the $C000 window
        mapper.write_prg(0xE000, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 window must map to the bank selected by $E000 register"
        );
    }

    #[test]
    fn prg_c000_bank_selection_wraps() {
        // With 11 banks, bank 11 wraps to bank 0.
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 11);
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 bank register must wrap around total bank count (11 % 11 = 0)"
        );
    }

    #[test]
    fn prg_read_offset_within_bank_is_correct() {
        // Bank 4 is all 0x04; the last byte of the window should also be 0x04.
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x9FFF),
            4,
            "last byte of $8000 window must still be bank 4"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_is_preserved_from_header() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let mapper = Ntdec2722Mapper::new(MapperContext::new_for_test(
            40,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_initially() {
        let mapper = make_mapper_direct();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_fires_after_4096_cpu_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn irq_does_not_fire_before_4096_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must NOT fire before 4096 CPU cycles have elapsed"
        );
    }

    #[test]
    fn irq_acknowledged_and_disabled_by_8000_write() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending before ack");
        mapper.write_prg(0x8000, 0); // disable + ack
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after $8000 write"
        );
    }

    #[test]
    fn irq_counter_resets_when_re_enabled() {
        // After ack, re-enabling should restart the 4096-cycle count from zero.
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x8000, 0); // ack
        mapper.write_prg(0xA000, 0); // re-enable
        // 4095 more cycles should NOT fire
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire until 4096 cycles after re-enable"
        );
    }

    #[test]
    fn irq_self_acknowledges_at_8192_cycles() {
        // The CD4020 counter bit 13 fires at 8192 and clears the IRQ line.
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..8192 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must self-acknowledge after 8192 CPU cycles (CD4020 bit 13)"
        );
    }

    // --- Registers snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, PRG_BANKS);
        let chr_rom = banked_data(8 * 1024, CHR_BANKS);
        let mut mapper = Ntdec2722Mapper::new(MapperContext::new_for_test(
            40,
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Vertical,
        ));

        // Set up state
        mapper.write_prg(0xE000, 2); // $C000 bank = 2
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }

        let snap = mapper.registers_snapshot();

        let mut restored = Ntdec2722Mapper::new(MapperContext::new_for_test(
            40,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0xC000),
            2,
            "Restored $C000 bank must match snapshotted value"
        );
        assert!(
            restored.irq_pending() == mapper.irq_pending(),
            "Restored IRQ pending state must match"
        );
    }
}
