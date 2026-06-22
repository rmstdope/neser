use super::*;

impl Cpu {
    #[allow(dead_code)]
    pub fn state(&self) -> CpuRegisters {
        CpuRegisters {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            pc: self.pc,
            p: self.p,
        }
    }

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn x(&self) -> u8 {
        self.x
    }

    pub fn y(&self) -> u8 {
        self.y
    }

    pub fn sp(&self) -> u8 {
        self.sp
    }

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn p(&self) -> u8 {
        self.p
    }

    /// Get the total number of cycles executed since last reset
    pub fn get_total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Returns the address of the most recent non-dummy CPU write during the last instruction,
    /// or `None` if no write occurred.
    pub fn last_cpu_write_addr(&self) -> Option<u16> {
        self.last_cpu_write_addr
    }

    #[cfg(test)]
    pub fn set_total_cycles(&mut self, cycles: u64) {
        self.total_cycles = cycles;
    }

    /// Update `mapper_has_expansion_audio` and `mapper_has_irq` from the currently
    /// inserted cartridge's mapper capabilities.
    ///
    /// Must be called whenever a new cartridge is inserted. These flags gate
    /// expensive per-cycle nested `RefCell` borrow chains so they are only paid
    /// when the mapper actually requires them.
    pub fn update_mapper_capability_flags(&mut self) {
        if let Some(caps) = self.bus.borrow().cartridge_mapper_capabilities() {
            self.mapper_has_expansion_audio = caps.has_expansion_audio;
            self.mapper_has_irq = caps.has_irq;
        } else {
            self.mapper_has_expansion_audio = false;
            self.mapper_has_irq = false;
        }
    }

    #[cfg(test)]
    pub fn test_mapper_has_expansion_audio(&self) -> bool {
        self.mapper_has_expansion_audio
    }

    #[cfg(test)]
    pub fn test_mapper_has_irq(&self) -> bool {
        self.mapper_has_irq
    }

    #[cfg(test)]
    pub fn set_a_register(&mut self, value: u8) {
        self.a = value;
    }

    #[cfg(test)]
    pub fn set_x(&mut self, value: u8) {
        self.x = value;
    }

    #[cfg(test)]
    pub fn set_y(&mut self, value: u8) {
        self.y = value;
    }

    #[cfg(test)]
    pub fn set_sp(&mut self, value: u8) {
        self.sp = value;
    }

    #[cfg(test)]
    pub fn set_pc(&mut self, value: u16) {
        self.pc = value;
    }

    #[cfg(test)]
    pub fn set_p(&mut self, value: u8) {
        self.p = value;
    }

    /// Simulate a JSR to $7003 for trainer execution.
    /// Pushes `(game_vector − 1)` onto the stack (hi byte first) and sets PC to $7003.
    /// The trainer must end with RTS to return execution to `game_vector`.
    pub fn divert_to_trainer(&mut self, game_vector: u16) {
        let return_addr = game_vector.wrapping_sub(1);
        let hi = (return_addr >> 8) as u8;
        let lo = return_addr as u8;
        let addr_hi = 0x0100 | self.sp as u16;
        self.bus.borrow_mut().write(addr_hi, hi, false);
        self.sp = self.sp.wrapping_sub(1);
        let addr_lo = 0x0100 | self.sp as u16;
        self.bus.borrow_mut().write(addr_lo, lo, false);
        self.sp = self.sp.wrapping_sub(1);
        self.pc = 0x7003;
    }

    pub fn add_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;
    }

    /// Capture the current CPU state for save-state.
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            pc: self.pc,
            p: self.p,
            total_cycles: self.total_cycles,
            halted: self.halted,
            nmi_pending: self.nmi_pending,
            irq_pending: self.irq_pending,
            prev_need_nmi: self.prev_need_nmi,
            prev_run_irq: self.prev_run_irq,
            run_irq: self.run_irq,
            delayed_i_flag: self.delayed_i_flag,
            forced_irq_pending: self.forced_irq_pending,
            skip_interrupt_latch_this_cycle: self.skip_interrupt_latch_this_cycle,
            master_clock: self.master_clock.master_cycles(),
            master_clock_ppu: self.master_clock.ppu_cycles(),
            dmc_dma_phase: self.dmc_dma_phase,
            interrupt_stack: self.interrupt_stack.clone(),
            current_tick_info: self.current_tick_info,
        }
    }

    /// Restore CPU state from a save-state.
    pub fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        self.sp = state.sp;
        self.pc = state.pc;
        self.p = state.p;
        self.total_cycles = state.total_cycles;
        self.halted = state.halted;
        self.nmi_pending = state.nmi_pending;
        self.irq_pending = state.irq_pending;
        self.prev_need_nmi = state.prev_need_nmi;
        self.prev_run_irq = state.prev_run_irq;
        self.run_irq = state.run_irq;
        self.delayed_i_flag = state.delayed_i_flag;
        self.forced_irq_pending = state.forced_irq_pending;
        self.skip_interrupt_latch_this_cycle = state.skip_interrupt_latch_this_cycle;
        self.master_clock.set_master_cycles(state.master_clock);
        self.master_clock.set_ppu_cycles(state.master_clock_ppu);
        self.dmc_dma_phase = state.dmc_dma_phase;
        self.interrupt_stack = state.interrupt_stack.clone();
        self.current_tick_info = state.current_tick_info;
    }
}
