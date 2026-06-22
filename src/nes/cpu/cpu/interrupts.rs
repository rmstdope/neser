use super::*;

impl Cpu {
    pub fn current_interrupt(&self) -> Option<InterruptKind> {
        self.interrupt_stack.last().copied()
    }

    pub(super) fn end_cpu_cycle_latch_interrupt_lines(&mut self) {
        // Capture previous-cycle state, then update
        // edge/level detections based on the current end-of-cycle line status.

        self.prev_need_nmi = self.nmi_pending;
        if self.ppu.borrow_mut().poll_nmi() {
            self.nmi_pending = true;
        }

        self.prev_run_irq = self.run_irq;

        // Level-triggered IRQ line: sample hardware lines each CPU cycle.
        // Unit tests may force an asserted IRQ via `forced_irq_pending`.
        let irq_asserted_from_apu = self.apu.borrow().poll_irq();
        let irq_asserted_from_mapper = if self.mapper_has_irq {
            self.bus.borrow().mapper_irq_pending()
        } else {
            false
        };
        self.irq_pending =
            irq_asserted_from_apu || irq_asserted_from_mapper || self.forced_irq_pending;

        // The value that will be used for the *next* instruction's interrupt check.
        self.run_irq = self.should_poll_irq();
    }

    pub(super) fn service_irq_or_nmi_sequence(&mut self) {
        // Shared interrupt sequence used after an instruction completes.
        // Two dummy reads with suppressed PC increment, then push PC, push PS, set I, vector.
        // PAL DMA nuances omitted for now.
        // NMI can interrupt IRQ vectoring, but only if it becomes pending early enough.
        // This behavior is required for blargg cpu_interrupts_v2/3-nmi_and_irq.
        let mut nmi_hijack = self.nmi_pending;

        let pc = self.pc;
        let _sp = self.sp;
        let _p = self.p;
        let _cycle = self.total_cycles;
        let _frame = self.ppu.borrow().timing().frame_count();
        let _scanline = self.ppu.borrow().timing().scanline();
        let _pixel = self.ppu.borrow().timing().pixel();
        self.dummy_read(pc);
        nmi_hijack |= self.nmi_pending;
        self.dummy_read(pc);
        nmi_hijack |= self.nmi_pending;

        // Push PC (high then low). Sample NMI between the two stack writes.
        self.push_byte((self.pc >> 8) as u8);
        nmi_hijack |= self.nmi_pending;
        self.push_byte(self.pc as u8);
        nmi_hijack |= self.nmi_pending;

        // For IRQ/NMI, the pushed status has B cleared and bit 5 set.
        let flags = (self.p & !FLAG_BREAK) | FLAG_UNUSED;
        self.push_byte(flags);
        self.p |= FLAG_INTERRUPT;
        // Interrupt entry sets I immediately; any pending CLI/SEI/PLP "delay" state
        // must not leak into the handler.
        self.delayed_i_flag = None;

        if nmi_hijack {
            self.nmi_pending = false;
            self.interrupt_stack.push(InterruptKind::Nmi);
            self.pc = self.read_u16(NMI_VECTOR);
        } else {
            // IRQ has been serviced; clear any forced IRQ. Hardware IRQ remains asserted
            // only if the APU line is still high and will be re-sampled next cycle.
            self.forced_irq_pending = false;
            self.interrupt_stack.push(InterruptKind::Irq);
            self.pc = self.read_u16(IRQ_VECTOR);
        }
        trace_cpu!(1;
            "{} PC={:04X}                         A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} cyc={:<3} F/S/P={}/{:03}/{:03}",
            if nmi_hijack { "NMI " } else { "IRQ " },
            pc,
            self.a,
            self.x,
            self.y,
            _p,
            _sp,
            _cycle,
            _frame,
            _scanline,
            _pixel
        );
    }

    pub(super) fn end_cpu_cycle_latch_irq_line_only(&mut self) {
        // When ticking synthetic CPU cycles (e.g., DMA stalls), we need IRQ sampling to keep
        // running, but we must not poll/clear the PPU's edge-latched NMI flag.
        self.prev_run_irq = self.run_irq;

        let irq_asserted_from_apu = self.apu.borrow().poll_irq();
        let irq_asserted_from_mapper = if self.mapper_has_irq {
            self.bus.borrow().mapper_irq_pending()
        } else {
            false
        };
        self.irq_pending =
            irq_asserted_from_apu || irq_asserted_from_mapper || self.forced_irq_pending;

        self.run_irq = self.should_poll_irq();
    }

    pub(super) fn trigger_nmi_without_bus_cycles(&mut self) {
        // Replicates `trigger_nmi` semantics without using `read`/`write` helpers,
        // so we don't accidentally advance CPU cycles or PPU timing here.
        self.delayed_i_flag = None;
        let pc = self.pc;

        // Push PC (high then low)
        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, (pc >> 8) as u8, false);
        self.sp = self.sp.wrapping_sub(1);

        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, (pc & 0xFF) as u8, false);
        self.sp = self.sp.wrapping_sub(1);

        // Push status (B clear, unused set)
        let mut p_with_break = self.p & !FLAG_BREAK;
        p_with_break |= FLAG_UNUSED;
        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, p_with_break, false);
        self.sp = self.sp.wrapping_sub(1);

        // Read NMI vector and set PC
        let lo = self.bus.borrow_mut().read(NMI_VECTOR, false) as u16;
        let hi = self.bus.borrow_mut().read(NMI_VECTOR + 1, false) as u16;
        self.pc = (hi << 8) | lo;

        self.interrupt_stack.push(InterruptKind::Nmi);

        // Set Interrupt Disable flag
        self.p |= FLAG_INTERRUPT;
    }

    /// Reset the CPU.
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on.
    ///
    /// - On soft reset: preserve A/X/Y, decrement SP by 3, set I.
    /// - On hard reset: restore power-on register defaults, then run the reset sequence.
    /// - Takes 7 CPU cycles (5 internal + 2 vector reads)
    pub fn reset(&mut self, soft_reset: bool) {
        if !soft_reset {
            self.a = 0;
            self.x = 0;
            self.y = 0;
            self.sp = 0x00;
            self.p = FLAG_UNUSED;
        }

        // Set I flag (bit 2) and ensure unused bit is set.
        // Note: blargg cpu_reset/registers expects reset to not change other flags (e.g. D).
        self.p |= FLAG_INTERRUPT | FLAG_UNUSED;

        // Subtract 3 from SP (wrapping if necessary)
        self.sp = self.sp.wrapping_sub(3);

        // Clear cycle-accurate instruction state
        self.halted = false;
        self.delayed_i_flag = None;
        self.nmi_pending = false;
        self.irq_pending = false;
        self.forced_irq_pending = false;
        self.prev_need_nmi = false;
        self.prev_run_irq = false;
        self.run_irq = false;
        self.skip_interrupt_latch_this_cycle = false;
        self.interrupt_stack.clear();

        // Reset cycle counters and master clock alignment.
        self.total_cycles = 0;
        self.master_clock.reset();

        // Reset takes 7 CPU cycles total: 5 internal cycles + 2 reset-vector reads.
        for _ in 0..5 {
            self.internal_cycle();
        }

        // Read reset vector and set PC (2 CPU cycles via bus reads)
        self.pc = self.read_reset_vector();
    }

    /// Get the effective I flag value for IRQ polling
    /// If there's a delayed I flag value, use that; otherwise use the current I flag
    fn get_effective_i_flag(&self) -> bool {
        self.delayed_i_flag
            .unwrap_or((self.p & FLAG_INTERRUPT) != 0)
    }

    /// Check if IRQ should be allowed to trigger
    /// Returns true if an IRQ is pending and the effective I flag (considering delays) allows IRQs
    pub fn should_poll_irq(&self) -> bool {
        self.irq_pending && !self.get_effective_i_flag()
    }

    /// Set the NMI pending flag (test-only helper).
    ///
    /// Some unit tests need to force an NMI edge before the next instruction.
    #[cfg(test)]
    pub(crate) fn set_nmi_pending(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Set the IRQ pending flag
    /// This should be called by the NES loop when IRQ is detected
    #[cfg(test)]
    pub(crate) fn set_irq_pending(&mut self, pending: bool) {
        self.forced_irq_pending = pending;
        // Preserve prior unit-test behavior where `set_irq_pending(true)` makes
        // `should_poll_irq()` immediately reflect an asserted IRQ.
        self.irq_pending = pending;
    }
}
