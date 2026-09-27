//! The CPU's IO-register write path: one route for every access width.
//!
//! `Bus::write8/16/32` hand every write to region 0x4 to [`GbaBus::write_io`]. It splits the write
//! into the halfword registers it touches ([`IoLane`]), runs each side-effect hook once per lane
//! before and after storing, and stores with the width's own semantics. A hook added here reaches
//! byte, halfword and word writes alike.

use super::addressing::{dma_control_index, timer_control_index};
use super::gba_bus::GbaBus;

/// The access width of a CPU write to the IO region.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum IoWidth {
    Byte,
    Half,
    Word,
}

/// One halfword register an IO write touches.
#[derive(Clone, Copy)]
struct IoLane {
    /// The register's halfword-aligned address.
    addr: u32,
    /// The bytes of the register this write stores.
    mask: u16,
    /// The register value the write produces: the written bytes merged into the current value.
    value: u16,
    /// A timer this write enables, and the prescaler phase to align it to once stored.
    timer_phase: Option<(usize, u32)>,
}

impl GbaBus {
    /// Write `value` of the given width to the IO region at `addr`, with every side effect.
    pub(super) fn write_io(&mut self, addr: u32, value: u32, width: IoWidth) {
        let addr = match width {
            IoWidth::Byte => addr,
            IoWidth::Half => addr & !1,
            IoWidth::Word => addr & !3,
        };
        let debug_register = match width {
            IoWidth::Byte => self.write_mgba_debug8(addr, value as u8),
            IoWidth::Half => self.write_mgba_debug16(addr, value as u16),
            IoWidth::Word => self.write_mgba_debug32(addr, value),
        };
        if debug_register {
            return;
        }

        let mut lanes = self.io_lanes(addr, value, width);
        for lane in lanes.iter_mut().flatten() {
            lane.timer_phase = self.io_write_before(*lane);
        }
        self.io_store(addr, value, width);
        for lane in lanes.into_iter().flatten() {
            self.io_write_after(lane);
        }

        if self.dma.any_pending() && self.dma_start_delay_cycles == 0 {
            self.run_pending_dma();
        }
    }

    fn io_lanes(&self, addr: u32, value: u32, width: IoWidth) -> [Option<IoLane>; 2] {
        let full = |addr: u32, value: u16| IoLane {
            addr,
            mask: 0xFFFF,
            value,
            timer_phase: None,
        };
        match width {
            IoWidth::Byte => {
                let aligned = addr & !1;
                let shift = (addr & 1) * 8;
                let mask = 0xFFu16 << shift;
                let written = ((value & 0xFF) as u16) << shift;
                let merged = (self.io_lane_current(aligned) & !mask) | written;
                [
                    Some(IoLane {
                        addr: aligned,
                        mask,
                        value: merged,
                        timer_phase: None,
                    }),
                    None,
                ]
            }
            IoWidth::Half => [Some(full(addr, value as u16)), None],
            IoWidth::Word => [
                Some(full(addr, value as u16)),
                Some(full(addr + 2, (value >> 16) as u16)),
            ],
        }
    }

    /// The current value of the halfword register a byte write merges into: the live timer
    /// control and DMA CNT_H (write-only or component-owned), the IO backing store otherwise.
    fn io_lane_current(&self, aligned: u32) -> u16 {
        if let Some(timer) = timer_control_index(aligned) {
            self.timers.channels[timer].control
        } else if let Some(channel) = dma_control_index(aligned) {
            self.dma.channels[channel].cnt_h
        } else {
            self.io.backing_u16(aligned)
        }
    }

    /// Side effects that must see the register's value before the write stores it.
    fn io_write_before(&mut self, lane: IoLane) -> Option<(usize, u32)> {
        // HALTCNT is the high byte of 0x0400_0300: bit 7 clear requests halt mode.
        if lane.addr == 0x0400_0300 && lane.mask & 0xFF00 != 0 && lane.value & 0x8000 == 0 {
            self.halt_requested = true;
        }
        let timer_phase = self.timer_enable_phase_for_write16(lane.addr, lane.value);
        self.defer_active_timer_reload_write_cycle(lane.addr);
        self.prestep_timer_disable_for_write16(lane.addr, lane.value);
        self.mark_timer_start_delay_for_write16(lane.addr, lane.value);
        self.mark_dma_start_delay_for_write16(lane.addr, lane.value);
        timer_phase
    }

    /// Store the write with its width's own register semantics.
    fn io_store(&mut self, addr: u32, value: u32, width: IoWidth) {
        match width {
            IoWidth::Byte => {
                let byte = value as u8;
                if addr == 0x0400_0410 {
                    self.undoc_0x410 = byte;
                } else if addr == 0x0400_0301 {
                    // HALTCNT: handled before the store, and not kept in the backing store.
                } else if (0x0400_0060..=0x0400_00A7).contains(&addr) {
                    self.apu.write8(addr, byte);
                } else {
                    self.io.write8(
                        addr,
                        byte,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                }
            }
            IoWidth::Half => {
                let half = value as u16;
                if (0x0400_0060..=0x0400_00A6).contains(&addr) {
                    self.apu.write16(addr, half);
                } else {
                    self.io.write16(
                        addr,
                        half,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                }
            }
            IoWidth::Word => {
                // FIFO A and B need full 32-bit word writes.
                if addr == 0x0400_00A0 {
                    self.apu.write_fifo_a_word(value);
                } else if addr == 0x0400_00A4 {
                    self.apu.write_fifo_b_word(value);
                } else if (0x0400_0060..=0x0400_00A6).contains(&addr) {
                    self.apu.write16(addr, value as u16);
                    // Only write the upper halfword if it is also within range.
                    if addr + 2 <= 0x0400_00A6 {
                        self.apu.write16(addr + 2, (value >> 16) as u16);
                    }
                } else {
                    self.io.write32(
                        addr,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                }
            }
        }
    }

    /// Side effects that follow the stored register value.
    fn io_write_after(&mut self, lane: IoLane) {
        if let Some((timer, phase)) = lane.timer_phase {
            self.timers.align_prescaler_phase(timer, phase);
        }
        self.trace_dma_cnt_h_write(lane.addr, lane.value);
        match lane.addr {
            0x0400_0204 => {
                let waitcnt = self.io.backing_u16(lane.addr);
                self.waitstates.recalculate(waitcnt);
            }
            0x0400_0128 => {
                let siocnt = self.io.backing_u16(lane.addr);
                self.write_siocnt(siocnt);
            }
            0x0400_0134 => {
                let rcnt = self.io.backing_u16(lane.addr);
                self.sio.write_rcnt(rcnt);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::gba::bus::GbaBus;
    use crate::gba::cpu::bus::Bus;

    /// The tests' own width, not `IoWidth`: they write through the `Bus` trait so the address
    /// alignment in `cpu_bus.rs` stays in the path under test.
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

    #[test]
    fn siocnt_start_reaches_the_serial_port_for_every_width() {
        for width in WIDTHS {
            let mut bus = GbaBus::new();
            // SIOCNT: start (bit 7) with the internal clock (bit 0), IRQ enabled.
            write_reg(&mut bus, width, 0x0400_0128, 0x4081, 0x00FF, 0x0000);
            assert!(
                bus.sio_start_delay_cycles > 0,
                "{width:?}: a SIOCNT start write must reach the serial port"
            );
        }
    }
}
