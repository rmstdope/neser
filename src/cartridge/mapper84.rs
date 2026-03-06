//! Mapper 084 – PC-SMB2J (Super Mario Bros. 2 Japan FDS conversion)
//!
//! Specifications:
//! - Primary: NesDev wiki (https://www.nesdev.org/wiki/INES_Mapper_084) — details unknown.
//!   The wiki notes this mapper is "reportedly PC-SMB2J" and may be "same FDS port as Mapper 40 or 50".
//! - Fallback: No Mesen2 implementation exists (absent from MapperFactory.cpp).
//!
//! Known Limitations:
//! - The exact PCB behavior is undocumented. This implementation mirrors Mapper 040 (NTDEC 2722),
//!   which is the closest documented mapper for SMB2J FDS conversions per the NesDev note.
//! - FCEUX historically supported this mapper but dropped it; no authoritative source is available.

use crate::cartridge::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

use super::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

/// Mapper 084 – PC-SMB2J
///
/// Hardware: PC-SMB2J PCB (details unknown)
///
/// Specifications:
/// - Primary: <https://www.nesdev.org/wiki/INES_Mapper_084> (details unknown; possibly same as Mapper 040)
/// - PRG-ROM: 64 KiB (8 × 8 KiB banks)
/// - CHR: 8 KiB fixed (ROM or RAM)
/// - Mirroring: Fixed from header
///
/// Based on the NesDev note that this mapper "may be the same FDS port as Mapper 40",
/// the PRG window layout follows Mapper 040 (NTDEC 2722):
/// - $6000–$7FFF: Fixed bank 6
/// - $8000–$9FFF: Fixed bank 4
/// - $A000–$BFFF: Fixed bank 5
/// - $C000–$DFFF: Switchable via $E000 write (bits 2:0)
/// - $E000–$FFFF: Fixed bank 7
///
/// Register map (range $8000–$FFFF, mask $E000):
/// - $8000: Disable and acknowledge IRQ
/// - $A000: Enable IRQ (counter resets to 0)
/// - $E000: Select 8 KiB PRG bank for $C000 window
///
/// IRQ: CPU-cycle counter; fires after 4096 cycles, self-acks at 8192.
///
/// Known games: SMBJ2 (Super Mario Bros. 2 Japan)
pub struct Mapper84 {
    base: BaseMapper,
    /// Switchable bank index for the $C000–$DFFF window.
    prg_bank: u8,
    irq: CpuCycleIrq,
}

impl Mapper84 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB

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
        base.configure_prg_6000_banking();
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
        self.base.select_prg_6000_page(Self::BANK_AT_6000 as i16);
        self.base.select_prg_page(0, 4);
        self.base.select_prg_page(1, 5);
        self.base.select_prg_page(2, self.prg_bank as i16);
        self.base.select_prg_page(3, 7);
    }
}

impl Mapper for Mapper84 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xE000 {
            0x8000 => {
                self.irq.set_enabled(false);
                self.irq.acknowledge();
                self.irq.set_counter(0);
            }
            0xA000 => {
                self.irq.set_enabled(true);
                self.irq.set_counter(0);
            }
            0xE000 => {
                self.prg_bank = value;
                self.update_banks();
            }
            _ => {}
        }
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
    use super::Mapper84;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 11 banks × 8 KiB — non-power-of-two to prevent false-pass wrapping.
    const PRG_BANKS: usize = 11;
    const CHR_BANKS: usize = 1;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            84,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 84 should be implemented")
    }

    fn make_mapper_direct() -> Mapper84 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper84::new(MapperContext::new_for_test(
            84,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // --- Factory ---

    #[test]
    fn mapper_84_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            84,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 84 must be registered in the factory"
        );
    }

    // --- PRG fixed banks ---

    #[test]
    fn prg_6000_is_fixed_bank_6() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            6,
            "$6000 must read from fixed bank 6"
        );
    }

    #[test]
    fn prg_8000_is_fixed_bank_4() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 must read from fixed bank 4"
        );
    }

    #[test]
    fn prg_a000_is_fixed_bank_5() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 must read from fixed bank 5"
        );
    }

    #[test]
    fn prg_e000_is_fixed_bank_7() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 must read from fixed bank 7"
        );
    }

    // --- PRG switchable window ---

    #[test]
    fn prg_c000_defaults_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 defaults to bank 0 on power-on"
        );
    }

    #[test]
    fn prg_c000_selects_bank_via_e000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$E000 write must select bank 3 at $C000"
        );
    }

    #[test]
    fn prg_bank_selection_wraps_at_total_bank_count() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 11); // 11 % 11 = 0
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "bank index must wrap mod total banks"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_is_preserved_from_header() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mapper = Mapper84::new(MapperContext::new_for_test(
            84,
            prg,
            chr,
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
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper_direct();
        for _ in 0..8192 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    #[test]
    fn irq_fires_after_4096_cycles_when_enabled() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn irq_does_not_fire_before_4096_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles"
        );
    }

    #[test]
    fn irq_self_acknowledges_at_8192_cycles() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..8192 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must self-acknowledge after 8192 cycles"
        );
    }

    #[test]
    fn irq_acknowledged_and_disabled_by_8000_write() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0x8000, 0); // disable + ack
        assert!(!mapper.irq_pending(), "IRQ must clear after $8000 write");
    }

    #[test]
    fn irq_counter_resets_when_re_enabled() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xA000, 0);
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x8000, 0); // ack
        mapper.write_prg(0xA000, 0); // re-enable
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire until 4096 cycles after re-enable"
        );
    }

    // --- Snapshot ---

    #[test]
    fn registers_snapshot_and_restore() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0xE000, 2);
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored $C000 bank must match"
        );
        assert_eq!(
            restored.irq_pending(),
            mapper.irq_pending(),
            "Restored IRQ pending state must match"
        );
    }
}
