use crate::cartridge::Cartridge;
use crate::cpu;
use crate::memory;
use crate::opcode;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Nes {
    pub memory: Rc<RefCell<memory::Memory>>,
    pub cpu: cpu::Cpu,
}

impl Nes {
    pub fn new() -> Self {
        let memory = Rc::new(RefCell::new(memory::Memory::new()));
        let cpu = cpu::Cpu::new(memory.clone());
        Self { memory, cpu }
    }

    /// Insert a cartridge and map it into memory
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.memory.borrow_mut().map_cartridge(cartridge);
    }

    /// Generate a trace line for the current CPU state
    pub fn trace(&self) -> String {
        let pc = self.cpu.pc;
        let memory = self.memory.borrow();

        // Read the opcode and determine instruction size
        let opcode_byte = memory.read(pc);
        let instruction = opcode::lookup(opcode_byte)
            .unwrap_or_else(|| panic!("Invalid opcode: 0x{:02X}", opcode_byte));

        // Read operand bytes
        let byte1 = if instruction.bytes() > 1 {
            memory.read(pc.wrapping_add(1))
        } else {
            0
        };
        let byte2 = if instruction.bytes() > 2 {
            memory.read(pc.wrapping_add(2))
        } else {
            0
        };

        // Build the hex dump string based on instruction bytes
        let hex_dump = match instruction.bytes() {
            1 => format!("{:02X}      ", opcode_byte),
            2 => format!("{:02X} {:02X}   ", opcode_byte, byte1),
            3 => format!("{:02X} {:02X} {:02X}", opcode_byte, byte1, byte2),
            _ => panic!("Invalid instruction byte count"),
        };

        // Build the assembly instruction string
        let asm = match instruction.mode {
            "IMP" => format!("{}", instruction.mnemonic),
            "ACC" => format!("{} A", instruction.mnemonic),
            "IMM" => format!("{} #${:02X}", instruction.mnemonic, byte1),
            "ZP" => {
                let addr = byte1 as u16;
                let value = memory.read(addr);
                format!("{} ${:02X} = {:02X}", instruction.mnemonic, byte1, value)
            }
            "ZPX" => {
                let addr = byte1.wrapping_add(self.cpu.x) as u16;
                let value = memory.read(addr);
                format!(
                    "{} ${:02X},X @ {:02X} = {:02X}",
                    instruction.mnemonic, byte1, addr as u8, value
                )
            }
            "ZPY" => {
                let addr = byte1.wrapping_add(self.cpu.y) as u16;
                let value = memory.read(addr);
                format!(
                    "{} ${:02X},Y @ {:02X} = {:02X}",
                    instruction.mnemonic, byte1, addr as u8, value
                )
            }
            "ABS" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                // JMP and JSR don't show memory value for ABS addressing
                if instruction.mnemonic == "JMP" || instruction.mnemonic == "JSR" {
                    format!("{} ${:04X}", instruction.mnemonic, addr)
                } else {
                    let value = memory.read(addr);
                    format!("{} ${:04X} = {:02X}", instruction.mnemonic, addr, value)
                }
            }
            "ABSX" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                let effective_addr = addr.wrapping_add(self.cpu.x as u16);
                let value = memory.read(effective_addr);
                format!(
                    "{} ${:04X},X @ {:04X} = {:02X}",
                    instruction.mnemonic, addr, effective_addr, value
                )
            }
            "ABSY" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                let effective_addr = addr.wrapping_add(self.cpu.y as u16);
                let value = memory.read(effective_addr);
                format!(
                    "{} ${:04X},Y @ {:04X} = {:02X}",
                    instruction.mnemonic, addr, effective_addr, value
                )
            }
            "INDX" => {
                let zp_addr = byte1.wrapping_add(self.cpu.x);
                let addr_lo = memory.read(zp_addr as u16);
                let addr_hi = memory.read(zp_addr.wrapping_add(1) as u16);
                let addr = u16::from_le_bytes([addr_lo, addr_hi]);
                let value = memory.read(addr);
                format!(
                    "{} (${:02X},X) @ {:02X} = {:04X} = {:02X}",
                    instruction.mnemonic, byte1, zp_addr, addr, value
                )
            }
            "INDY" => {
                let addr_lo = memory.read(byte1 as u16);
                let addr_hi = memory.read(byte1.wrapping_add(1) as u16);
                let base_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                let effective_addr = base_addr.wrapping_add(self.cpu.y as u16);
                let value = memory.read(effective_addr);
                format!(
                    "{} (${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                    instruction.mnemonic, byte1, base_addr, effective_addr, value
                )
            }
            "IND" => {
                let ptr_addr = u16::from_le_bytes([byte1, byte2]);
                let addr_lo = memory.read(ptr_addr);
                // 6502 bug: if ptr_addr is at page boundary (e.g., $02FF),
                // high byte wraps within same page instead of crossing to next page
                let hi_addr = if ptr_addr & 0xFF == 0xFF {
                    ptr_addr & 0xFF00 // Wrap to start of same page
                } else {
                    ptr_addr.wrapping_add(1)
                };
                let addr_hi = memory.read(hi_addr);
                let target_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                format!(
                    "{} (${:04X}) = {:04X}",
                    instruction.mnemonic, ptr_addr, target_addr
                )
            }
            "REL" => {
                let offset = byte1 as i8;
                let target = pc.wrapping_add(2).wrapping_add(offset as u16);
                format!("{} ${:04X}", instruction.mnemonic, target)
            }
            _ => panic!("Unknown addressing mode"),
        };

        // Adjust spacing for 4-character mnemonics (starts one character earlier)
        let (pad_before, width) = if instruction.mnemonic.len() == 4 {
            (" ", 32)
        } else {
            ("  ", 31)
        };

        format!(
            "{:04X}  {}{}{:<width$} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X}",
            pc,
            hex_dump,
            pad_before,
            asm,
            self.cpu.a,
            self.cpu.x,
            self.cpu.y,
            self.cpu.p,
            self.cpu.sp,
            width = width
        )
    }
}
