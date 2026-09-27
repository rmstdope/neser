//! The CPU's IO-register write path: one route for every access width.

#[cfg(test)]
mod tests {
    use crate::gba::bus::GbaBus;
    use crate::gba::cpu::bus::Bus;

    #[derive(Clone, Copy, Debug)]
    enum Width {
        Byte,
        Half,
        Word,
    }

    const WIDTHS: [Width; 3] = [Width::Byte, Width::Half, Width::Word];

    /// Write the halfword register at `reg` with the given width. A byte write stores only the
    /// byte selected by `byte_mask` (0x00FF or 0xFF00); a word write stores `other` into the
    /// neighbouring halfword of the same word.
    fn write_reg(bus: &mut GbaBus, width: Width, reg: u32, value: u16, byte_mask: u16, other: u16) {
        match width {
            Width::Byte => {
                if byte_mask == 0x00FF {
                    bus.write8(reg, value as u8);
                } else {
                    bus.write8(reg + 1, (value >> 8) as u8);
                }
            }
            Width::Half => bus.write16(reg, value),
            Width::Word => {
                let word = if reg & 2 == 0 {
                    value as u32 | ((other as u32) << 16)
                } else {
                    other as u32 | ((value as u32) << 16)
                };
                bus.write32(reg & !3, word);
            }
        }
    }

    #[test]
    fn active_timer_reload_from_ffff_defers_current_instruction_tick_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            bus.write16(0x0400_0100, 0xFFFF);
            bus.write16(0x0400_0102, 0x00C0 | 0x0080);
            // Run past the enable start delay; with reload FFFF the counter stays at FFFF.
            bus.step(4);
            assert_eq!(bus.read16(0x0400_0100), 0xFFFF);

            bus.begin_cpu_instruction();
            // TM0CNT_L low byte = 0; a word write keeps TM0CNT_H enabled.
            write_reg(
                &mut bus,
                width,
                0x0400_0100,
                0x0000,
                0x00FF,
                0x00C0 | 0x0080,
            );
            bus.end_cpu_instruction();
            bus.step_after_cpu_instruction(1);

            assert_eq!(
                bus.read16(0x0400_0100),
                0xFFFF,
                "{width:?}: the active reload write cycle must not tick TM0 from FFFF"
            );
        }
    }

    #[test]
    fn cpu_timer_disable_samples_before_instruction_cycles_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            bus.write16(0x0400_0100, 0);
            bus.write16(0x0400_0102, 0x0080);
            bus.step(0xFFFF);
            assert_eq!(bus.read16(0x0400_0100), 0xFFFF);

            bus.begin_cpu_instruction();
            // TM0CNT_H low byte = 0 disables TM0; a word write also rewrites reload 0.
            write_reg(&mut bus, width, 0x0400_0102, 0x0000, 0x00FF, 0x0000);
            bus.end_cpu_instruction();

            assert_eq!(
                bus.read16(0x0400_0100),
                0,
                "{width:?}: the disable write samples the timer's last cycle first"
            );
            bus.step_after_cpu_instruction(3);
            assert_eq!(
                bus.read16(0x0400_0100),
                0,
                "{width:?}: a disabled timer must not tick for the rest of the instruction"
            );
        }
    }

    #[test]
    fn timer_enable_marks_start_delay_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            write_reg(&mut bus, width, 0x0400_0102, 0x0080, 0x00FF, 0x0000);
            assert!(
                bus.timer_start_delay_pending,
                "{width:?}: enabling TM0 must mark the timer start delay"
            );
        }
    }

    #[test]
    fn immediate_dma_enable_marks_start_delay_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            write_reg(&mut bus, width, 0x0400_00BA, 0x8000, 0xFF00, 0x0001);
            assert_eq!(
                bus.dma_start_delay_cycles, 2,
                "{width:?}: enabling an immediate DMA must mark the start delay"
            );
        }
    }

    #[test]
    fn haltcnt_requests_halt_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            write_reg(&mut bus, width, 0x0400_0300, 0x0000, 0xFF00, 0x0000);
            assert!(bus.halt_requested, "{width:?}: HALTCNT bit 7 clear halts");
        }
    }

    #[test]
    fn postflg_byte_write_does_not_halt() {
        let mut bus = GbaBus::new();
        bus.write8(0x0400_0300, 0x01);
        assert!(!bus.halt_requested);
    }

    #[test]
    fn waitcnt_recalculated_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            write_reg(&mut bus, width, 0x0400_0204, 0x0014, 0x00FF, 0x0000);
            assert_eq!(
                bus.waitstates.waitcnt & 0x00FF,
                0x0014,
                "{width:?}: WAITCNT writes reach the waitstate table"
            );
        }
    }
}
