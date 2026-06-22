use super::*;

impl Cpu {
    /// Read a byte from memory at the specified address
    pub(super) fn read(&mut self, addr: u16) -> u8 {
        self.read_with_dummy_flag(addr, false)
    }

    /// Dummy read a byte from memory at the specified address
    pub(super) fn dummy_read(&mut self, addr: u16) -> u8 {
        self.read_with_dummy_flag(addr, true)
    }

    fn read_with_dummy_flag(&mut self, addr: u16, is_dummy_read: bool) -> u8 {
        loop {
            if let Some((ref mut tick, total)) = self.current_tick_info {
                trace_cpu!(2;
                    "tick ({}/{}) cyc={} [read] addr=0x{:04X}",
                    *tick,
                    total,
                    self.total_cycles,
                    addr
                );
                let _ = total;
                *tick += 1;
            } else {
                trace_cpu!(2; "tick cyc={} [read] addr=0x{:04X}", self.total_cycles, addr);
            }

            self.before_cpu_cycle(false);

            // Process any pending DMA (OAM and/or DMC)
            match self.process_pending_dma(addr) {
                DmaReadOutcome::NoDma => {}
                DmaReadOutcome::RetryRead => {
                    // DMA was processed; retry the read from the beginning
                    continue;
                }
                DmaReadOutcome::ReturnValue(value) => return value,
            }

            let value = self.bus.borrow_mut().read(addr, is_dummy_read);

            self.after_cpu_cycle(false);
            return value;
        }
    }

    /// Read a 16-bit word from memory at the specified address
    pub(super) fn read_u16(&mut self, addr: u16) -> u16 {
        self.read(addr) as u16 | ((self.read(addr + 1) as u16) << 8)
    }

    /// Write a byte to memory at the specified address
    pub(super) fn write(&mut self, addr: u16, value: u8, dummy: bool) {
        if let Some((ref mut tick, total)) = self.current_tick_info {
            trace_cpu!(2;
                "tick ({}/{}) cyc={} [write{}] addr=0x{:04X} value=0x{:02X}",
                *tick,
                total,
                self.total_cycles,
                if dummy { " (dummy)" } else { "" },
                addr,
                value
            );
            let _ = total;
            *tick += 1;
        } else {
            trace_cpu!(2;
                "tick cyc={} [write{}] addr=0x{:04X} value=0x{:02X}",
                self.total_cycles,
                if dummy { " (dummy)" } else { "" },
                addr,
                value
            );
        }
        self.before_cpu_cycle(true);
        self.bus.borrow_mut().write(addr, value, dummy);
        if !dummy {
            self.last_cpu_write_addr = Some(addr);
        }
        self.after_cpu_cycle(true);
    }

    /// Dummy write a byte to memory at the specified address
    pub(super) fn dummy_write(&mut self, addr: u16, value: u8) {
        self.write(addr, value, true);
    }

    /// Read a byte from memory at PC and increment PC
    pub(super) fn read_byte_from_pc(&mut self) -> u8 {
        let value = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    /// Perform a read-modify-write operation with dummy write
    /// All RMW instructions on the 6502 first write the original value back,
    /// Read a 16-bit word from memory at PC (little-endian) and increment PC
    pub(super) fn read_word_from_pc(&mut self) -> u16 {
        let lo = self.read_byte_from_pc() as u16;
        let hi = self.read_byte_from_pc() as u16;
        (hi << 8) | lo
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    pub(super) fn read_reset_vector(&mut self) -> u16 {
        self.read_u16(RESET_VECTOR)
    }

    /// Read a 16-bit word from zero page (wraps at page boundary)
    pub(super) fn read_word_from_zp(&mut self, addr: u8) -> u16 {
        let lo = self.read(addr as u16) as u16;
        let hi = self.read(addr.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }

    /// Read a word from an indirect address with 6502 page boundary bug
    /// If the address is at a page boundary (e.g., 0x10FF), the high byte
    /// is read from the start of the same page (0x1000) instead of the next page (0x1100)
    pub(super) fn read_word_indirect(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi_addr = if addr & 0xFF == 0xFF {
            // Page boundary bug: wrap within the same page
            addr & 0xFF00
        } else {
            addr + 1
        };
        let hi = self.read(hi_addr) as u16;
        (hi << 8) | lo
    }

    /// Push a byte onto the stack
    pub(super) fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.sp as u16);
        self.write(addr, value, false);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    pub(super) fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte first
        self.push_byte(value as u8); // Low byte second
    }

    /// Pull a byte from the stack
    pub(super) fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let addr = 0x0100 | (self.sp as u16);
        self.read(addr)
    }

    /// Pull a word from the stack (low byte first)
    pub(super) fn pop_word(&mut self) -> u16 {
        let lo = self.pop_byte() as u16; // Low byte first
        let hi = self.pop_byte() as u16; // High byte second
        (hi << 8) | lo
    }
}
