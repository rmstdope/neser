use super::*;

impl Cpu {
    fn is_controller_port2_read(addr: u16) -> bool {
        addr == 0x4017
    }

    pub(super) fn should_skip_first_input_clock(read_address: u16, dmc_address: u16) -> bool {
        let is_controller_read = matches!(read_address, 0x4016 | 0x4017);
        is_controller_read && (dmc_address & 0x1F) == (read_address & 0x1F)
    }

    fn dmc_pending_single_byte_fetch(&self) -> bool {
        let mut apu = self.apu.borrow_mut();
        let dmc = apu.dmc_mut().capture_state();
        dmc.sample_length == 1 && dmc.bytes_remaining == 1
    }

    /// If an OAM DMA is pending (triggered by a write to $4014), execute it and
    /// return the number of CPU cycles consumed.
    ///
    /// This is a cycle-accurate implementation that:
    /// - Allows DMC DMA to interrupt the OAM DMA mid-transfer
    /// - Properly handles alignment cycles
    /// - Maintains correct get/put cycle semantics for DMC DMA collision
    ///
    /// Cycle cost (without DMC collision):
    /// - 513 cycles when DMA starts on a "put" cycle (odd total_cycles at DMA start)
    /// - 514 cycles when DMA starts on a "get" cycle (even total_cycles at DMA start)
    ///
    /// NES DMA timing:
    /// - DMA begins on the next CPU cycle after $4014 write
    /// - If $4014 written on even cycle: DMA starts on odd cycle = "put" = no extra align = 513
    /// - If $4014 written on odd cycle: DMA starts on even cycle = "get" = needs align = 514
    ///
    /// With DMC DMA collision:
    /// - DMC and OAM share cycles where possible
    /// - Extra 2-4 cycles depending on when collision occurs
    pub fn handle_oam_dma_if_pending(&mut self) -> Option<u16> {
        let page = self.bus.borrow_mut().take_oam_dma_page()?;

        let cycles_before = self.total_cycles;

        // Halt cycle - hijack the read the CPU was trying to do
        // This is shared by both OAM and DMC DMA if both are pending
        self.tick_single_dma_cycle();

        // Run the OAM DMA transfer with DMC collision handling
        self.run_oam_dma_internal_no_nmi(page);

        let dma_cycles = (self.total_cycles - cycles_before) as u16;

        // Check for NMI after DMA (outside of cycle count for backward compatibility)
        if self.ppu.borrow_mut().poll_nmi() {
            self.trigger_nmi_without_bus_cycles();
            self.tick_ppu_apu_for_cpu_cycles(7);
            self.add_cycles(7);
        }

        Some(dma_cycles)
    }

    /// Tick a single DMA cycle (advances PPU/APU by one CPU cycle).
    /// Also increments the internal CPU cycle counter for get/put cycle tracking.
    fn tick_single_dma_cycle(&mut self) {
        self.master_clock.advance_cpu_cycles(1);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        let expansion = if self.mapper_has_expansion_audio {
            self.bus.borrow().mapper_expansion_audio_sample()
        } else {
            0.0
        };
        self.apu.borrow_mut().clock_with_expansion(expansion);

        // Synthetic cycles (DMA stalls) still advance mapper IRQ counters and expansion audio.
        self.bus.borrow_mut().mapper_cpu_cycle();
        self.end_cpu_cycle_latch_irq_line_only();

        // Increment internal cycle counter for get/put cycle tracking
        self.total_cycles += 1;
    }

    /// Start a DMC DMA transfer.
    /// Called when the DMC sample buffer becomes empty and needs refilling.
    pub(super) fn start_dmc_dma(&mut self) {
        self.dmc_dma_phase = DmcDmaPhase::Halt;
    }

    fn cpu_visible_dmc_dma_pending(&self) -> bool {
        matches!(self.dmc_dma_phase, DmcDmaPhase::Idle) && {
            let mut apu = self.apu.borrow_mut();
            apu.dmc_mut().cpu_dma_pending()
        }
    }

    /// Process any pending DMC DMA during a CPU read cycle.
    /// This is called from `read()` and handles the DMA state machine.
    ///
    /// DMC DMA sequence (per NESdev wiki):
    /// 1. Halt cycle: CPU read is repeated (discarded), consumes _needHalt
    /// 2. Dummy read cycle: CPU read is repeated (discarded), consumes _needDummyRead
    /// 3. Optional alignment cycle: if not on a "get" cycle, repeat read
    /// 4. Get cycle: actual DMC sample byte read
    pub(super) fn process_pending_dmc_dma(&mut self, read_address: u16) -> Option<u8> {
        let mut observed_bus_value = None;

        // Loop until DMC DMA completes
        while !matches!(self.dmc_dma_phase, DmcDmaPhase::Idle) {
            match self.dmc_dma_phase {
                DmcDmaPhase::Idle => break,
                DmcDmaPhase::Halt => {
                    self.before_cpu_cycle(false);
                    let _ = self.bus.borrow_mut().read(read_address, true);
                    self.after_cpu_cycle(false);
                    self.dmc_dma_phase = DmcDmaPhase::Dummy;
                }
                DmcDmaPhase::Dummy => {
                    self.before_cpu_cycle(false);
                    let _ = self.bus.borrow_mut().read(read_address, true);
                    self.after_cpu_cycle(false);
                    self.dmc_dma_phase = DmcDmaPhase::Aligning;
                }
                DmcDmaPhase::Aligning => {
                    if !self.total_cycles.is_multiple_of(2) {
                        self.before_cpu_cycle(false);
                        let _ = self.bus.borrow_mut().read(read_address, true);
                        self.after_cpu_cycle(false);
                    }
                    self.dmc_dma_phase = DmcDmaPhase::Reading;
                }
                DmcDmaPhase::Reading => {
                    let dma_addr = {
                        let mut apu = self.apu.borrow_mut();
                        apu.dmc_mut().dma_address()
                    };

                    if let Some(addr) = dma_addr {
                        self.before_cpu_cycle(false);
                        let value = self.bus.borrow_mut().read(addr, false);
                        self.after_cpu_cycle(false);
                        self.apu.borrow_mut().dmc_mut().complete_dma_read(value);

                        if Self::is_controller_port2_read(read_address) {
                            observed_bus_value = Some(value);
                        }
                    }

                    self.dmc_dma_phase = DmcDmaPhase::Idle;
                }
            }
        }

        observed_bus_value
    }

    /// Process any pending DMA (OAM and/or DMC) during a CPU read cycle.
    /// Returns a DMA read outcome indicating whether to retry the read or return a bus value.
    pub(super) fn process_pending_dma(&mut self, read_address: u16) -> DmaReadOutcome {
        // Check if OAM DMA is pending
        let oam_dma_pending = self.bus.borrow().oam_dma_pending();
        let dmc_dma_pending = self.cpu_visible_dmc_dma_pending();

        if !oam_dma_pending && !dmc_dma_pending {
            return DmaReadOutcome::NoDma;
        }

        // Note: The caller (read()) has already called before_cpu_cycle().
        // We complete that cycle here with the halt read and after_cpu_cycle().

        // If only DMC is pending (no OAM), handle it separately with existing logic
        if !oam_dma_pending && dmc_dma_pending {
            self.start_dmc_dma();

            let dmc_dma_address = {
                let mut apu = self.apu.borrow_mut();
                apu.dmc_mut().dma_address()
            };

            let is_controller_read = matches!(read_address, 0x4016 | 0x4017);
            let single_byte_dmc_fetch = self.dmc_pending_single_byte_fetch();
            let skip_first_input_clock = dmc_dma_address
                .map(|address| Self::should_skip_first_input_clock(read_address, address))
                .unwrap_or(false);
            let use_dummy_halt_read =
                is_controller_read && (!single_byte_dmc_fetch || skip_first_input_clock);

            // Halt cycle: complete the CPU cycle started by read() - the read value is discarded
            let halted_read_value = self
                .bus
                .borrow_mut()
                .read(read_address, use_dummy_halt_read);
            self.after_cpu_cycle(false);
            self.dmc_dma_phase = DmcDmaPhase::Dummy;

            // Process remaining DMC DMA cycles
            let observed_bus_value = self.process_pending_dmc_dma(read_address);

            if read_address == 0x4016 {
                if observed_bus_value.is_none() {
                    return DmaReadOutcome::RetryRead;
                }
                return DmaReadOutcome::ReturnValue(halted_read_value);
            }

            if let Some(value) = observed_bus_value {
                return DmaReadOutcome::ReturnValue(value);
            }

            return DmaReadOutcome::RetryRead;
        }

        // OAM DMA is pending (possibly with DMC collision)
        // Halt cycle: complete the CPU cycle started by read() - the read value is discarded
        let _ = self.bus.borrow_mut().read(read_address, false);
        self.after_cpu_cycle(false);

        // Now run the OAM DMA (which handles DMC collision internally)
        let page = self.bus.borrow_mut().take_oam_dma_page();
        if let Some(page) = page {
            self.run_oam_dma_internal(page);
        }

        DmaReadOutcome::RetryRead
    }

    /// Run OAM DMA internally (called from process_pending_dma or handle_oam_dma_if_pending).
    /// This is the actual OAM DMA loop with DMC collision handling.
    ///
    /// If `handle_nmi` is true, checks for and handles NMI after DMA completes.
    ///
    /// Note: The caller is responsible for the halt cycle. If DMC is pending at start,
    /// it shares that halt cycle which the caller has already consumed.
    fn run_oam_dma_internal_impl(&mut self, page: u8, handle_nmi: bool) {
        let source_base = (page as u16) << 8;

        // Check if DMC DMA is also pending at the start
        let dmc_pending_at_start = self.cpu_visible_dmc_dma_pending();

        // DMC DMA progress states (like Pinky)
        const DMC_IDLE: u8 = 0;
        const DMC_HALT_DONE: u8 = 1;
        const DMC_READY_TO_READ: u8 = 2;

        // OAM size in bytes
        const OAM_SIZE: u16 = 256;

        // Track DMC DMA progress
        // If DMC is pending at start, it shares the halt cycle that the caller consumed
        let mut dmc_progress: u8 = if dmc_pending_at_start {
            DMC_HALT_DONE
        } else {
            DMC_IDLE
        };
        let mut sprite_dma_value: Option<u8> = None;
        let mut sprite_offset: u16 = 0;
        let mut sprite_dma_done = false;

        loop {
            // Check DMC progress
            let dmc_pending = self.cpu_visible_dmc_dma_pending();

            // If DMC becomes pending during OAM DMA, start tracking it
            if dmc_pending && dmc_progress == DMC_IDLE {
                dmc_progress = DMC_HALT_DONE; // halt done (shared with OAM cycle)
            }

            // Helper: are we on a get or put phase?
            let on_get_phase = self.total_cycles.is_multiple_of(2);
            let on_put_phase = !on_get_phase;

            // Can DMC read this cycle?
            let dmc_ready_to_read =
                dmc_pending && dmc_progress >= DMC_READY_TO_READ && on_get_phase;

            // Can OAM read this cycle?
            let oam_ready_to_read = !sprite_dma_done && sprite_dma_value.is_none() && on_get_phase;

            // Can OAM write this cycle?
            let oam_ready_to_write = !sprite_dma_done && sprite_dma_value.is_some() && on_put_phase;

            if dmc_ready_to_read {
                // DMC takes priority - do the DMC read
                let dma_addr = {
                    let mut apu = self.apu.borrow_mut();
                    apu.dmc_mut().dma_address()
                };

                if let Some(addr) = dma_addr {
                    let value = self.bus.borrow_mut().read(addr, false);
                    self.apu.borrow_mut().dmc_mut().complete_dma_read(value);
                }
                self.tick_single_dma_cycle();
                dmc_progress = DMC_IDLE; // DMC done
            } else if oam_ready_to_read {
                // OAM read cycle - also counts as DMC dummy if DMC is waiting
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ; // dummy done
                }
                let addr = source_base.wrapping_add(sprite_offset);
                let value = self.bus.borrow_mut().read(addr, false);
                sprite_dma_value = Some(value);
                self.tick_single_dma_cycle();
            } else if oam_ready_to_write {
                // OAM write cycle - also counts as DMC alignment if DMC is waiting
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ; // alignment done
                }
                self.ppu
                    .borrow_mut()
                    .write_oam_data_dma(sprite_dma_value.take().unwrap());
                self.tick_single_dma_cycle();
                sprite_offset += 1;
                if sprite_offset >= OAM_SIZE {
                    sprite_dma_done = true;
                }
            } else if dmc_pending || !sprite_dma_done {
                // Alignment/dummy cycle
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ;
                }
                self.tick_single_dma_cycle();
            } else {
                // All done
                break;
            }
        }

        // Check for NMI after DMA (only if requested)
        if handle_nmi && self.ppu.borrow_mut().poll_nmi() {
            self.trigger_nmi_without_bus_cycles();
            self.tick_ppu_apu_for_cpu_cycles(7);
            self.add_cycles(7);
        }
    }

    /// Run OAM DMA without NMI handling (for handle_oam_dma_if_pending which handles NMI separately)
    fn run_oam_dma_internal_no_nmi(&mut self, page: u8) {
        self.run_oam_dma_internal_impl(page, false);
    }

    /// Run OAM DMA with NMI handling (for process_pending_dma)
    fn run_oam_dma_internal(&mut self, page: u8) {
        self.run_oam_dma_internal_impl(page, true);
    }
}
