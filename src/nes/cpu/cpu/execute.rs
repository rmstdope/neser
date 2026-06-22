use super::*;

impl Cpu {
    fn get_operand_value(&mut self, op: &OpCode, operand: u16) -> u8 {
        match op.mode {
            AddrMode::IMM => operand as u8,
            AddrMode::ZP
            | AddrMode::ZPX
            | AddrMode::ZPY
            | AddrMode::ABS
            | AddrMode::ABSX
            | AddrMode::ABSY
            | AddrMode::IND
            | AddrMode::INDX
            | AddrMode::INDY => self.read(operand),
            AddrMode::IMP | AddrMode::ACC | AddrMode::REL => operand as u8,
            _ => panic!("Unhandled addressing mode: {}", op.mode),
        }
    }

    pub(super) fn set_a(&mut self, value: u8) {
        self.a = value;
        self.update_zero_and_negative_flags(self.a);
    }

    fn exec_arr_illegal(&mut self, imm: u8) {
        // ARR (undocumented): AND with immediate, then ROR, with special flag handling.
        // Flags on 2A03:
        // - C = bit 6 of result
        // - V = bit 6 XOR bit 5 of result
        self.a &= imm;

        let old_carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        self.a = (self.a >> 1) | (old_carry << 7);

        self.update_zero_and_negative_flags(self.a);

        if (self.a & 0x40) != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        let bit6 = (self.a >> 6) & 1;
        let bit5 = (self.a >> 5) & 1;
        if (bit6 ^ bit5) != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    fn exec_sya_illegal(&mut self, addr: u16) {
        // *SYA/SHY (undocumented): Store Y AND (high byte of BASE address + 1).
        // Quirk: on page crossing, the high byte of the target address is ANDed with Y.
        let base_addr = addr.wrapping_sub(self.x as u16);
        let base_high_byte = (base_addr >> 8) as u8;
        let value = self.y & base_high_byte.wrapping_add(1);

        let page_crossed = Self::page_crossed(base_addr, addr);
        let final_addr = if page_crossed {
            let modified_high = ((addr >> 8) as u8) & self.y;
            ((modified_high as u16) << 8) | (addr & 0x00FF)
        } else {
            addr
        };

        self.write(final_addr, value, false);
    }

    fn exec_sxa_illegal(&mut self, addr: u16) {
        // *SXA/SHX (undocumented): Store X AND (high byte of BASE address + 1).
        // Quirk: on page crossing, the high byte of the target address is ANDed with X.
        let base_addr = addr.wrapping_sub(self.y as u16);
        let base_high_byte = (base_addr >> 8) as u8;
        let value = self.x & base_high_byte.wrapping_add(1);

        let page_crossed = Self::page_crossed(base_addr, addr);
        let final_addr = if page_crossed {
            let modified_high = ((addr >> 8) as u8) & self.x;
            ((modified_high as u16) << 8) | (addr & 0x00FF)
        } else {
            addr
        };

        self.write(final_addr, value, false);
    }

    pub fn execute(&mut self) {
        if self.halted {
            return;
        }

        self.last_cpu_write_addr = None;

        // The CPU's IRQ inhibit flag (I) has a one-instruction delay behavior for
        // CLI/SEI and (conditionally) PLP. We model that using `delayed_i_flag`:
        // when set, `should_poll_irq()` uses the old I value for one instruction.
        let had_delayed_i_flag = self.delayed_i_flag.is_some();
        let mut new_delayed_i_flag: Option<bool> = None;

        // Trace CPU tick before reading opcode (so PC is correct for the instruction)
        // Read instruction bytes for tracing without advancing PC
        // Use read_for_testing to avoid affecting the open bus state
        // Only execute this code when CPU tracing is actually enabled
        #[cfg(debug_assertions)]
        if crate::platform::debugging::is_cpu_tracing_enabled() {
            let pc = self.pc;
            let mut memory = self.bus.borrow_mut();
            let opcode_byte = memory.read_for_testing(pc);
            let op = crate::nes::cpu::opcode::lookup(opcode_byte);
            let byte1 = if op.bytes() > 1 {
                memory.read_for_testing(pc.wrapping_add(1))
            } else {
                0
            };
            let byte2 = if op.bytes() > 2 {
                memory.read_for_testing(pc.wrapping_add(2))
            } else {
                0
            };
            drop(memory); // Release borrow before trace macro may do other operations
            let hex_dump = match op.bytes() {
                1 => format!("{:02X}", opcode_byte),
                2 => format!("{:02X} {:02X}", opcode_byte, byte1),
                _ => format!("{:02X} {:02X} {:02X}", opcode_byte, byte1, byte2),
            };
            let asm = match op.mode {
                AddrMode::IMP => op.mnemonic.to_string(),
                AddrMode::ACC => format!("{} A", op.mnemonic),
                AddrMode::IMM => format!("{} #${:02X}", op.mnemonic, byte1),
                AddrMode::ZP => format!("{} ${:02X}", op.mnemonic, byte1),
                AddrMode::ZPX => format!("{} ${:02X},X", op.mnemonic, byte1),
                AddrMode::ZPY => format!("{} ${:02X},Y", op.mnemonic, byte1),
                AddrMode::ABS => format!(
                    "{} ${:04X}",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::ABSX | AddrMode::ABSXW => format!(
                    "{} ${:04X},X",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::ABSY | AddrMode::ABSYW => format!(
                    "{} ${:04X},Y",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::IND => format!(
                    "{} (${:04X})",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::INDX => format!("{} (${:02X},X)", op.mnemonic, byte1),
                AddrMode::INDY | AddrMode::INDYW => format!("{} (${:02X}),Y", op.mnemonic, byte1),
                AddrMode::REL => {
                    let offset = byte1 as i8;
                    let target = pc.wrapping_add(2).wrapping_add(offset as u16);
                    format!("{} ${:04X}", op.mnemonic, target)
                }
            };
            // Set up tick tracking for this instruction
            self.current_tick_info = Some((1, op.cycles));
            trace_cpu!(1;
                "exec PC={:04X} {:08} {:14} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} cyc={:<3} F/S/P={}/{:03}/{:03}",
                pc,
                hex_dump,
                asm,
                self.a,
                self.x,
                self.y,
                self.p,
                self.sp,
                self.total_cycles,
                self.ppu.borrow().timing().frame_count(),
                self.ppu.borrow().timing().scanline(),
                self.ppu.borrow().timing().pixel()
            );
        }

        let opcode = self.read_byte_from_pc();
        let op = crate::nes::cpu::opcode::lookup(opcode);
        let operand = self.get_operand(*op);

        match op.mnemonic {
            Mnemonic::BRK => {
                // BRK pushes (PC + 1), which corresponds to BRK+2 overall.
                // At this point, PC points to the padding byte, so add 1.
                self.push_word(self.pc.wrapping_add(1));

                let flags = self.p | FLAG_BREAK | FLAG_UNUSED;

                if self.nmi_pending {
                    self.nmi_pending = false;
                    self.push_byte(flags);
                    self.p |= FLAG_INTERRUPT;
                    self.pc = self.read_u16(NMI_VECTOR);
                } else {
                    self.push_byte(flags);
                    self.p |= FLAG_INTERRUPT;
                    self.pc = self.read_u16(IRQ_VECTOR);
                }

                // Ensure we don't start an NMI immediately after BRK.
                self.prev_need_nmi = false;
            }
            Mnemonic::ORA => {
                let value = self.get_operand_value(op, operand);
                self.ora(value);
            }
            Mnemonic::HLT | Mnemonic::KIL => {
                self.halted = true;
                // Halt on instruction, not after
                self.pc -= 1;
            }
            Mnemonic::USLO => {
                self.slo(operand);
            }
            Mnemonic::NOP | Mnemonic::UNOP => {
                // Consume one cycle
                self.get_operand_value(op, operand);
            }
            Mnemonic::ASL => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.asl(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.asl(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PHP => {
                // Push processor status with BREAK and UNUSED flags set
                let flags = self.p | FLAG_BREAK | FLAG_UNUSED;
                self.push_byte(flags);
            }
            Mnemonic::UAAC => {
                // Undocumented: AND with accumulator, then copy bit 7 to carry
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.update_zero_and_negative_flags(self.a);
                // Copy bit 7 to carry flag (same pattern as ASL)
                let carry = if self.a & 0x80 != 0 { FLAG_CARRY } else { 0 };
                self.p = (self.p & !FLAG_CARRY) | carry;
            }
            Mnemonic::BPL => {
                // Branch if negative flag is clear
                let offset = operand as i8;
                if self.p & FLAG_NEGATIVE == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        // Branch taken - do a dummy read
                        self.dummy_read(self.pc);
                        // Page crossing: extra dummy read
                        self.dummy_read(self.pc);
                    } else {
                        // Taken non-page-crossing branches ignore interrupts during their last
                        // clock (blargg cpu_interrupts_v2/5-branch_delays_irq).
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLC => {
                self.p &= !FLAG_CARRY;
            }
            Mnemonic::JSR => {
                // JSR takes 6 cycles:
                // 1. Fetch opcode
                // 2. Fetch low byte of address
                // 3. Internal operation (dummy read from stack)
                // 4. Push PCH to stack
                // 5. Push PCL to stack
                // 6. Fetch high byte of address

                // Dummy read from stack pointer for cycle 3
                self.dummy_read(0x0100 | (self.sp as u16));

                // Push return address (PC - 1) to stack
                // PC is already pointing to the next instruction, so PC - 1 is the last byte of JSR
                let return_addr = self.pc.wrapping_sub(1);
                self.push_word(return_addr);

                // Set PC to target address
                self.pc = operand;
            }
            Mnemonic::AND => {
                let value = self.get_operand_value(op, operand);
                self.and(value);
            }
            Mnemonic::URLA => {
                self.rla(operand);
            }
            Mnemonic::BIT => {
                let value = self.read(operand);
                self.bit(value);
            }
            Mnemonic::ROL => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.rol(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.rol(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PLP => {
                // Dummy read from current SP (cycle 2)
                self.dummy_read(0x0100 | (self.sp as u16));
                // Pop status from stack
                let status = self.pop_byte();
                // Restore flags, but always set UNUSED and clear BREAK
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p = (status & !FLAG_BREAK) | FLAG_UNUSED;
                let new_i_flag = (self.p & FLAG_INTERRUPT) != 0;

                // If PLP changes I, IRQ polling uses the OLD value for the next instruction.
                if old_i_flag != new_i_flag {
                    new_delayed_i_flag = Some(old_i_flag);
                }
            }
            Mnemonic::BMI => {
                // Branch if negative flag is set
                let offset = operand as i8;
                if self.p & FLAG_NEGATIVE != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SEC => {
                self.p |= FLAG_CARRY;
            }
            Mnemonic::RTI => {
                // RTI (Return from Interrupt) - 6 cycles
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read from current PC
                self.dummy_read(self.pc);

                // Cycle 3: Increment SP (dummy read happens in pop_byte)
                // Cycle 4: Pull status from stack
                let status = self.pop_byte();
                // Restore flags, ignoring BREAK, always setting UNUSED
                self.p = (status & !FLAG_BREAK) | FLAG_UNUSED;

                // RTI clears the delayed I flag immediately (special case)
                self.delayed_i_flag = None;

                // Cycle 5-6: Pull PC from stack (low byte, then high byte)
                self.pc = self.pop_word();

                // Leaving interrupt handler.
                let _ = self.interrupt_stack.pop();
            }
            Mnemonic::EOR => {
                let value = self.get_operand_value(op, operand);
                self.eor(value);
            }
            Mnemonic::USRE => {
                self.sre(operand);
            }
            Mnemonic::LSR => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.lsr(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.lsr(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PHA => {
                // Push accumulator to stack
                self.push_byte(self.a);
            }
            Mnemonic::UASR => {
                // ASR/ALR (undocumented): AND with immediate, then LSR
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.a = self.lsr(self.a);
            }
            Mnemonic::JMP => {
                // Jump to address (operand is already the target address)
                self.pc = operand;
            }
            Mnemonic::BVC => {
                // Branch on overflow clear
                let offset = operand as i8;
                if (self.p & FLAG_OVERFLOW) == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLI => {
                // Save old I before clearing. IRQ polling uses the OLD value for the next instruction.
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p &= !FLAG_INTERRUPT;
                new_delayed_i_flag = Some(old_i_flag);
            }
            Mnemonic::RTS => {
                // Return from subroutine - 6 cycles:
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read from current SP
                self.dummy_read(0x0100 | (self.sp as u16));

                // Cycle 3-4: Pull return address from stack
                let addr = self.pop_word();

                // Cycle 5: Increment PC (PC = popped_value + 1)
                self.pc = addr.wrapping_add(1);

                // Cycle 6: Dummy read at incremented PC
                self.dummy_read(self.pc);
            }
            Mnemonic::ADC => {
                let value = self.get_operand_value(op, operand);
                self.adc(value);
            }
            Mnemonic::URRA => {
                self.rra(operand);
            }
            Mnemonic::ROR => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.ror(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.ror(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PLA => {
                // Pull accumulator from stack - 4 cycles:
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read at current PC
                self.dummy_read(self.pc);

                // Cycle 3: Increment SP (dummy read happens in pop_byte)
                // Cycle 4: Pull value from stack
                self.a = self.pop_byte();
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::UARR => {
                let value = self.get_operand_value(op, operand);
                self.exec_arr_illegal(value);
            }
            Mnemonic::BVS => {
                // Branch on overflow set
                let offset = operand as i8;
                if (self.p & FLAG_OVERFLOW) != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SEI => {
                // Save old I before setting. IRQ polling uses the OLD value for the next instruction.
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p |= FLAG_INTERRUPT;
                new_delayed_i_flag = Some(old_i_flag);
            }
            Mnemonic::STA => {
                self.write(operand, self.a, false);
            }
            Mnemonic::USAX => {
                // SAX: Store A AND X (undocumented)
                let value = self.a & self.x;
                self.write(operand, value, false);
            }
            Mnemonic::STY => {
                self.write(operand, self.y, false);
            }
            Mnemonic::STX => {
                // Store X Register
                self.write(operand, self.x, false);
            }
            Mnemonic::DEY => {
                // Decrement Y Register - already implemented as helper method
                self.dey();
            }
            Mnemonic::TXA => {
                // Transfer X to Accumulator - already implemented as helper method
                self.txa();
            }
            Mnemonic::UXAA => {
                // *XAA (undocumented) - Transfer X to A, then AND with immediate
                self.a = self.x;
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::BCC => {
                // Branch on Carry Clear
                let offset = operand as i8;
                if self.p & FLAG_CARRY == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::UXAS => {
                // *XAS / TAS (undocumented) - SP = A & X, then store SP & (high byte of address + 1)
                self.sp = self.a & self.x;
                let high_byte = (operand >> 8) as u8;
                let value = self.sp & high_byte.wrapping_add(1);
                self.write(operand, value, false);
            }
            Mnemonic::TYA => {
                // Transfer Y to Accumulator - already implemented as helper method
                self.tya();
            }
            Mnemonic::TXS => {
                // Transfer X to Stack Pointer - does not affect flags
                self.sp = self.x;
            }
            Mnemonic::USYA => {
                self.exec_sya_illegal(operand);
            }
            Mnemonic::USXA => {
                self.exec_sxa_illegal(operand);
            }
            Mnemonic::UAXA => {
                // *AXA (undocumented) - Store A AND X AND (high byte of address + 1)
                let high_byte = (operand >> 8) as u8;
                let value = self.a & self.x & high_byte.wrapping_add(1);
                self.write(operand, value, false);
            }
            Mnemonic::LDY => {
                let value = self.get_operand_value(op, operand);
                self.ldy(value);
            }
            Mnemonic::LDA => {
                let value = self.get_operand_value(op, operand);
                self.lda(value);
            }
            Mnemonic::LDX => {
                let value = self.get_operand_value(op, operand);
                self.ldx(value);
            }
            Mnemonic::ULAX => {
                // LAX (undocumented): Load A and X with the same value
                let value = self.get_operand_value(op, operand);
                self.lda(value);
                self.ldx(value);
            }
            Mnemonic::TAY => {
                // Transfer Accumulator to Y - already implemented as helper method
                self.tay();
            }
            Mnemonic::TAX => {
                // Transfer Accumulator to X - already implemented as helper method
                self.tax();
            }
            Mnemonic::UATX => {
                // *ATX (undocumented): Load A and X with immediate value
                // Also known as *LAX immediate or *OAL
                let value = self.get_operand_value(op, operand);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::BCS => {
                // Branch on Carry Set
                let offset = operand as i8;
                if self.p & FLAG_CARRY != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLV => {
                // Clear overflow flag
                self.p &= !FLAG_OVERFLOW;
            }
            Mnemonic::TSX => {
                // Transfer Stack pointer to X
                self.x = self.sp;
                self.update_zero_and_negative_flags(self.x);
            }
            Mnemonic::ULAR => {
                // Undocumented: AND memory with stack pointer, store in A, X, and SP
                let value = self.get_operand_value(op, operand);
                self.lar(value);
            }
            Mnemonic::CPY => {
                let value = self.get_operand_value(op, operand);
                self.cpy(value);
            }
            Mnemonic::CMP => {
                let value = self.get_operand_value(op, operand);
                self.cmp(value);
            }
            Mnemonic::UDCP => {
                // Undocumented: Decrement memory then compare with A
                self.dcp(operand);
            }
            Mnemonic::INY => {
                self.iny();
            }
            Mnemonic::DEX => {
                // Decrement X Register
                self.dex();
            }
            Mnemonic::UAXS => {
                // *AXS (undocumented): (A & X) - immediate -> X
                let value = self.get_operand_value(op, operand);
                self.axs(value);
            }
            Mnemonic::BNE => {
                // Branch if Not Equal (zero flag clear)
                let offset = operand as i8;
                if self.p & FLAG_ZERO == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLD => {
                // Clear Decimal flag
                self.p &= !FLAG_DECIMAL;
            }
            Mnemonic::CPX => {
                // Compare X with memory
                let value = self.get_operand_value(op, operand);
                self.cpx(value);
            }
            Mnemonic::SBC | Mnemonic::USBC => {
                // Subtract with Carry
                let value = self.get_operand_value(op, operand);
                self.sbc(value);
            }
            Mnemonic::UISB => {
                // *ISB (undocumented): Increment memory then SBC
                self.isb(operand);
            }
            Mnemonic::INX => {
                // Increment X Register
                self.inx();
            }
            Mnemonic::BEQ => {
                // Branch if Equal (zero flag set)
                let offset = operand as i8;
                if self.p & FLAG_ZERO != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SED => {
                // Set Decimal flag
                self.p |= FLAG_DECIMAL;
            }
            Mnemonic::INC => {
                // Increment memory
                let value = self.read(operand);
                //   (cycle accurate)
                self.dummy_write(operand, value);
                // Increment and write back
                let result = self.inc(value);
                self.write(operand, result, false);
            }
            Mnemonic::DEC => {
                // Decrement memory
                let value = self.read(operand);
                //   (cycle accurate)
                self.dummy_write(operand, value);
                // Decrement and write back
                let result = self.dec(value);
                self.write(operand, result, false);
            }
        }

        // Clear tick tracking after instruction
        self.current_tick_info = None;

        // Clear the previous delayed-I state after exactly one instruction.
        // If this instruction introduced a new delay, keep that for the next instruction.
        let cleared_delayed_i_flag_this_instruction = had_delayed_i_flag;
        if had_delayed_i_flag {
            self.delayed_i_flag = None;
        }
        if new_delayed_i_flag.is_some() {
            self.delayed_i_flag = new_delayed_i_flag;
        }

        // IRQ/NMI are taken after the instruction completes.
        //
        // Special case: when the delayed-I state just expired (e.g., the instruction after CLI),
        // IRQ recognition must reflect the *new* I state immediately at this boundary.
        // We accomplish this by re-evaluating `should_poll_irq()` only in that case.
        let irq_after_delayed_i_expires =
            cleared_delayed_i_flag_this_instruction && self.should_poll_irq();

        if self.prev_need_nmi || self.prev_run_irq || irq_after_delayed_i_expires {
            self.service_irq_or_nmi_sequence();
        }
    }

    /// Fetch the operand address or value for an instruction
    ///
    /// For memory-accessing modes (ZP, ABS, etc.), returns the effective address.
    /// For immediate mode (IMM), returns the immediate value (low byte only).
    /// For implied/accumulator modes, performs dummy read and returns 0.
    /// For relative mode (REL), returns the immediate byte (offset).
    ///
    /// # Arguments
    /// * `opcode` - The opcode byte to fetch the operand for
    ///
    /// # Returns
    /// The operand address or value (depending on addressing mode)
    pub fn get_operand(&mut self, op: OpCode) -> u16 {
        match op.mode {
            // Implied and Accumulator - perform dummy read
            AddrMode::IMP | AddrMode::ACC => {
                self.dummy_read(self.pc);
                0
            }

            // Immediate, Zero Page and Relative - return the immediate byte
            AddrMode::IMM | AddrMode::REL | AddrMode::ZP => self.read_byte_from_pc() as u16,

            // Zero Page,X - read base, dummy read at base, return base+X
            AddrMode::ZPX => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                base.wrapping_add(self.x) as u16
            }

            // Zero Page,Y - read base, dummy read at base, return base+Y
            AddrMode::ZPY => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                base.wrapping_add(self.y) as u16
            }

            // Absolute - return 16-bit address
            AddrMode::ABS => self.read_word_from_pc(),

            // Absolute,X - return address + X
            // Note: the page-crossing dummy read is performed inline below.
            AddrMode::ABSX => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.x as u16);
                // Dummy read at base + X with the wrong high byte when a page is crossed.
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Absolute,X (Write/RMW) - return address + X, always do dummy read
            AddrMode::ABSXW => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.x as u16);
                // Always do dummy read at base+X with wrong high byte (no carry into high byte)
                // for write/RMW indexed addressing.
                let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                self.dummy_read(dummy_addr);
                addr
            }

            // Absolute,Y - return address + Y
            // Note: the page-crossing dummy read is performed inline below.
            AddrMode::ABSY => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.y as u16);
                // Dummy read at base + Y with the wrong high byte when a page is crossed.
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Absolute,Y (Write/RMW) - return address + Y, always do dummy read
            AddrMode::ABSYW => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read at base + Y with wrong high byte if page crossed
                let page_crossed = Self::page_crossed(base, addr);
                let dummy_addr = if page_crossed { addr - 0x100 } else { addr };
                self.dummy_read(dummy_addr);
                addr
            }

            // Indirect - JMP ($addr) with 6502 page boundary bug
            AddrMode::IND => {
                let ptr = self.read_word_from_pc();
                self.read_word_indirect(ptr)
            }

            // Indexed Indirect - (ZP,X)
            // Always does dummy read at base address during indexing
            AddrMode::INDX => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                let ptr = base.wrapping_add(self.x);
                self.read_word_from_zp(ptr)
            }

            // Indirect Indexed - (ZP),Y (Read-only)
            // Note: Page crossing means dummy read
            AddrMode::INDY => {
                let ptr = self.read_byte_from_pc();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read at base + Y with wrong high byte if page crossed
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Indirect Indexed - (ZP),Y (Write/RMW)
            // Always do dummy read at base + Y with wrong high byte if page crossed
            AddrMode::INDYW => {
                let ptr = self.read_byte_from_pc();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read - with wrong high byte if page crossed
                let dummy_addr = if Self::page_crossed(base, addr) {
                    (base & 0xFF00) | (addr & 0x00FF)
                } else {
                    addr
                };
                self.dummy_read(dummy_addr);
                addr
            }
        }
    }
}
