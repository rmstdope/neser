//! Mapper 084 – PC-SMB2J (Super Mario Bros. 2 Japan FDS conversion)
//!
//! Specifications:
//! - Primary: NesDev wiki (<https://www.nesdev.org/wiki/INES_Mapper_084>) — details unknown.
//!   The wiki notes this mapper is "reportedly PC-SMB2J" and may be "same FDS port as Mapper 40 or 50".
//! - Fallback: No Mesen2 implementation exists (absent from MapperFactory.cpp).
//!
//! This mapper is implemented as an alias for [Mapper 040 (NTDEC 2722)][`super::ntdec_2722::Ntdec2722Mapper`],
//! which is the closest documented mapper for SMB2J FDS conversions per the NesDev note.
//! FCEUX historically supported this mapper but dropped it; no authoritative source is available.

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

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
        let mapper = create_mapper(MapperContext::new_for_test(
            84,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(8 * 1024, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 84 should be implemented");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- IRQ ---

    #[test]
    fn irq_not_pending_initially() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper();
        for _ in 0..8192 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    #[test]
    fn irq_fires_after_4096_cycles_when_enabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn irq_does_not_fire_before_4096_cycles() {
        let mut mapper = make_mapper();
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
        let mut mapper = make_mapper();
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
        let mut mapper = make_mapper();
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
        let mut mapper = make_mapper();
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
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 2);
        mapper.write_prg(0xA000, 0); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
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
