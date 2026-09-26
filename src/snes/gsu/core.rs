//! The GSU's execution pipeline: catch-up against the master clock, the one-byte program
//! prefetch, code-cache and Game Pak fetches, and the ROM read and RAM write buffers.
//!
//! The prefetch is what gives the GSU its branch delay slot (fullsnes "Jump Notes": "the next
//! BYTE after the jump opcode is fetched as opcode byte, and is executed before continuing at the
//! jump-target address"). While an opcode executes, `program_prefetch` already holds the byte at
//! R15, so R15 is "the address of the next opcode" as fullsnes says of every R15 read. The model
//! and its costs follow Mesen2 `Gsu::Exec`/`ReadOpCode`/`ReadOperand`/`ReadProgramByte`/`Step`.

use super::memory::{gsu_ram_index, gsu_rom_index};
use super::{CODE_CACHE_LINES, CODE_CACHE_SIZE, Gsu};

impl Gsu {
    /// Advances the GSU by one master clock: executes whole instructions while the GSU's own
    /// cycle count is behind, then lets the buffers run out the idle time.
    pub fn tick_one_master_clock(&mut self) {
        self.state.master_clock += 1;
        while self.is_executing() && self.state.cycle_count < self.state.master_clock {
            self.exec();
        }
        if self.state.cycle_count < self.state.master_clock {
            self.step(self.state.master_clock - self.state.cycle_count);
        }
    }

    fn is_executing(&self) -> bool {
        self.state.go && !self.state.waiting_for_rom && !self.state.waiting_for_ram
    }

    fn exec(&mut self) {
        let opcode = self.state.program_prefetch;
        self.state.program_prefetch = self.fetch_program_byte();
        self.r15_changed = false;
        self.execute(opcode);
        if !self.r15_changed {
            self.state.r[15] = self.state.r[15].wrapping_add(1);
        }
    }

    /// Consumes the prefetched byte as an operand and prefetches the next one.
    pub(super) fn read_operand(&mut self) -> u8 {
        let operand = self.state.program_prefetch;
        self.state.r[15] = self.state.r[15].wrapping_add(1);
        self.state.program_prefetch = self.fetch_program_byte();
        operand
    }

    /// Master clocks per GSU cycle: one at 21.4 MHz, two at 10.7 MHz.
    fn cycle_cost(&self) -> u64 {
        if self.state.clock_21mhz { 1 } else { 2 }
    }

    /// Master clocks for one Game Pak ROM/RAM byte access. fullsnes "CPU Misc": "ROM Read: 5
    /// cycles per byte at 21MHz, or 3 cycles per byte at 10MHz", i.e. 5 or 6 master clocks.
    pub(super) fn memory_cost(&self) -> u8 {
        if self.state.clock_21mhz { 5 } else { 6 }
    }

    fn fetch_program_byte(&mut self) -> u8 {
        let pc = self.state.r[15];
        if pc.wrapping_sub(self.state.cbr) < CODE_CACHE_SIZE as u16 {
            let slot = usize::from(pc) & (CODE_CACHE_SIZE - 1);
            if !self.state.code_cache_valid[slot >> 4] {
                self.fill_code_cache_line(pc);
            }
            self.step(self.cycle_cost());
            return self.state.code_cache[slot];
        }
        self.wait_for_program_bus();
        self.step(u64::from(self.memory_cost()));
        self.read_program_bus(self.state.pbr, pc)
    }

    /// Loads the 16-byte cache line holding `pc` from `[PBR:line]`. fullsnes describes lines
    /// loading alongside execution byte by byte; the whole line loads at once here, charged 16
    /// memory accesses, as in Mesen2's `InitProgramCache`.
    fn fill_code_cache_line(&mut self, pc: u16) {
        self.wait_for_program_bus();
        let line = pc & 0xFFF0;
        let slot = usize::from(line) & (CODE_CACHE_SIZE - 1);
        for i in 0..16 {
            self.state.code_cache[slot + i] =
                self.read_program_bus(self.state.pbr, line.wrapping_add(i as u16));
        }
        self.step(16 * u64::from(self.memory_cost()));
        self.state.code_cache_valid[slot >> 4] = true;
    }

    /// Before a program fetch from ROM or RAM: let the pending buffer operation on that bus
    /// finish, and halt if the S-CPU holds it.
    fn wait_for_program_bus(&mut self) {
        if self.program_bank_is_rom() {
            self.finish_rom_buffer();
            self.wait_for_rom_access();
        } else {
            self.finish_ram_buffer();
            self.wait_for_ram_access();
        }
    }

    fn program_bank_is_rom(&self) -> bool {
        self.state.pbr <= 0x5F
    }

    fn read_program_bus(&self, bank: u8, offset: u16) -> u8 {
        if let Some(index) = gsu_rom_index(bank, offset) {
            self.rom_byte(index)
        } else if let Some(index) = gsu_ram_index(bank, offset) {
            self.ram_byte(index)
        } else {
            // Nothing is mapped there on the GSU side; Mesen2 reads 0 as well.
            0
        }
    }

    pub(super) fn rom_byte(&self, index: usize) -> u8 {
        if self.rom.is_empty() {
            0
        } else {
            self.rom[index % self.rom.len()]
        }
    }

    pub(super) fn ram_byte(&self, index: usize) -> u8 {
        let ram = self.ram.borrow();
        if ram.is_empty() {
            0
        } else {
            ram[index % ram.len()]
        }
    }

    pub(super) fn write_ram_byte(&self, index: usize, value: u8) {
        let mut ram = self.ram.borrow_mut();
        let len = ram.len();
        if len != 0 {
            ram[index % len] = value;
        }
    }

    /// Accounts `clocks` master clocks of GSU time, completing the ROM and RAM buffer operations
    /// whose delay runs out within them (Mesen2 `Gsu::Step`).
    pub(super) fn step(&mut self, clocks: u64) {
        self.state.cycle_count += clocks;
        if self.state.rom_delay > 0 {
            self.state.rom_delay -= clocks.min(u64::from(self.state.rom_delay)) as u8;
            if self.state.rom_delay == 0 {
                self.wait_for_rom_access();
                let index = gsu_rom_index(self.state.rombr, self.state.r[14]);
                self.state.rom_buffer = index.map_or(0, |index| self.rom_byte(index));
                self.state.rom_read_pending = false;
            }
        }
        if self.state.ram_delay > 0 {
            self.state.ram_delay -= clocks.min(u64::from(self.state.ram_delay)) as u8;
            if self.state.ram_delay == 0 {
                self.wait_for_ram_access();
                let index = ram_offset(self.state.rambr, self.state.ram_write_address);
                self.write_ram_byte(index, self.state.ram_write_value);
            }
        }
    }

    /// Waits out a ROM buffer fill still in flight.
    pub(super) fn finish_rom_buffer(&mut self) {
        if self.state.rom_delay > 0 {
            self.step(u64::from(self.state.rom_delay));
        }
    }

    /// Waits out a RAM buffer write still in flight.
    pub(super) fn finish_ram_buffer(&mut self) {
        if self.state.ram_delay > 0 {
            self.step(u64::from(self.state.ram_delay));
        }
    }

    /// fullsnes SCMR: clearing RON/RAN "causes the GSU to enter WAIT status (if it accesses ROM
    /// or RAM), and continues when RON/RAN are changed back to 1". As in Mesen2, the access that
    /// found the bus taken still completes and the GSU halts before its next instruction.
    ///
    /// Only a running GSU can be made to wait: a buffer operation that completes while the GSU is
    /// stopped (an S-CPU write of R14, or a store landing after STOP) must not leave it halted,
    /// or a later start on cached code, which needs neither bus, would never run.
    pub(super) fn wait_for_rom_access(&mut self) {
        if self.state.go && !self.ron() {
            self.state.waiting_for_rom = true;
        }
    }

    pub(super) fn wait_for_ram_access(&mut self) {
        if self.state.go && !self.ran() {
            self.state.waiting_for_ram = true;
        }
    }

    /// Writes register `reg`, starting a ROM buffer fill when it is R14 (fullsnes "ROM-Read-Data
    /// Cache": "Loading the cache is invoked by any opcodes that do change R14") and noting a
    /// jump when it is R15.
    pub(super) fn write_reg(&mut self, reg: u8, value: u16) {
        self.state.r[usize::from(reg)] = value;
        match reg {
            14 => self.start_rom_buffer_fill(),
            15 => self.r15_changed = true,
            _ => {}
        }
    }

    pub(super) fn start_rom_buffer_fill(&mut self) {
        self.state.rom_read_pending = true;
        self.state.rom_delay = self.memory_cost();
    }

    pub(super) fn invalidate_all_code_cache_lines(&mut self) {
        self.state.code_cache_valid = [false; CODE_CACHE_LINES];
    }
}

/// Linear Game Pak RAM offset of `[RAMBR:address]`.
pub(super) fn ram_offset(rambr: u8, address: u16) -> usize {
    (usize::from(rambr & 0x01) << 16) | usize::from(address)
}
