use super::*;

impl Cpu {
    pub(super) fn tick_ppu_apu_for_cpu_cycles(&mut self, cpu_cycles: u16) {
        self.master_clock.advance_cpu_cycles(cpu_cycles as u64);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        for _ in 0..cpu_cycles {
            let expansion = if self.mapper_has_expansion_audio {
                self.bus.borrow().mapper_expansion_audio_sample()
            } else {
                0.0
            };
            self.apu.borrow_mut().clock_with_expansion(expansion);

            // Synthetic cycles still advance mapper IRQ counters and expansion audio.
            self.bus.borrow_mut().mapper_cpu_cycle();
            self.end_cpu_cycle_latch_irq_line_only();
        }
    }

    pub(super) fn internal_cycle(&mut self) {
        // Advance the CPU/PPU/APU by one CPU cycle without performing a bus read/write.
        if let Some((ref mut tick, total)) = self.current_tick_info {
            trace_cpu!(2;
                "tick ({}/{}) cyc={} [internal]",
                *tick,
                total,
                self.total_cycles
            );
            let _ = total;
            *tick += 1;
        } else {
            trace_cpu!(2; "tick cyc={} [internal]", self.total_cycles);
        }
        self.before_cpu_cycle(false);
        self.after_cpu_cycle(false);
    }

    /// Check if two addresses are on different pages
    pub(super) fn page_crossed(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    pub(super) fn before_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock.before_cpu_cycle(is_write);
        self.total_cycles += 1;
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);
        let expansion = if self.mapper_has_expansion_audio {
            self.bus.borrow().mapper_expansion_audio_sample()
        } else {
            0.0
        };
        self.apu.borrow_mut().clock_with_expansion(expansion);
    }

    pub(super) fn after_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock.after_cpu_cycle(is_write);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        // Some mappers (e.g., Konami VRC) use CPU-cycle-driven IRQ counters.
        self.bus.borrow_mut().mapper_cpu_cycle();

        // Latch interrupt lines at the end of each CPU cycle.
        // Some edge cases require skipping latching for a single cycle.
        if self.skip_interrupt_latch_this_cycle {
            self.skip_interrupt_latch_this_cycle = false;
        } else {
            self.end_cpu_cycle_latch_interrupt_lines();
        }
    }
}
