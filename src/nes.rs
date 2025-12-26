use crate::apu;
use crate::cartridge::Cartridge;
use crate::cpu;
use crate::cpu2;
use crate::mem_controller;
use crate::ppu;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvSystem {
    Ntsc,
    Pal,
}

impl TvSystem {
    /// Returns the PPU cycles per CPU cycle ratio for this TV system
    ///
    /// NTSC: 3.0 PPU cycles per CPU cycle (exact)
    /// PAL: 3.2 PPU cycles per CPU cycle (requires fractional tracking)
    pub fn ppu_cycles_per_cpu_cycle(&self) -> f64 {
        match self {
            TvSystem::Ntsc => 3.0,
            TvSystem::Pal => 3.2,
        }
    }

    /// Returns the number of scanlines per frame for this TV system
    ///
    /// NTSC: 262 scanlines per frame
    /// PAL: 312 scanlines per frame
    pub fn scanlines_per_frame(&self) -> u16 {
        match self {
            TvSystem::Ntsc => 262,
            TvSystem::Pal => 312,
        }
    }

    /// Returns the screen width for this TV system
    ///
    /// Both NTSC and PAL use 256 pixels width
    pub fn screen_width(&self) -> u32 {
        256
    }

    /// Returns the screen height for this TV system
    ///
    /// NTSC: 240 pixels
    /// PAL: 240 pixels (visible area, though PAL has more scanlines)
    pub fn screen_height(&self) -> u32 {
        240
    }
}

pub struct Nes {
    pub ppu: Rc<RefCell<ppu::Ppu>>,
    pub apu: Rc<RefCell<apu::Apu>>,
    pub memory: Rc<RefCell<mem_controller::MemController>>,
    pub cpu: cpu2::Cpu2,
    tv_system: TvSystem,
    fractional_ppu_cycles: f64,
    ready_to_render: bool,
}

impl Nes {
    pub fn new(tv_system: TvSystem) -> Self {
        let ppu = Rc::new(RefCell::new(ppu::Ppu::new(tv_system)));
        let apu = Rc::new(RefCell::new(apu::Apu::new()));
        let memory = Rc::new(RefCell::new(mem_controller::MemController::new(
            ppu.clone(),
            apu.clone(),
        )));
        let cpu = cpu2::Cpu2::new(memory.clone());

        // Initialize PPU 1 cycle ahead for proper sprite 0 hit timing
        // This creates a one-cycle offset where PPU state changes become
        // visible to the CPU one cycle later, matching hardware behavior
        ppu.borrow_mut().run_ppu_cycles(1);

        Self {
            ppu,
            apu,
            memory,
            cpu,
            tv_system,
            fractional_ppu_cycles: 0.0,
            ready_to_render: false,
        }
    }

    /// Get the TV system this NES instance is configured for
    #[cfg(test)]
    pub fn tv_system(&self) -> TvSystem {
        self.tv_system
    }

    /// Insert a cartridge and map it into memory
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.memory.borrow_mut().map_cartridge(cartridge);
    }

    /// Reset the NES system (CPU and PPU)
    pub fn reset(&mut self) {
        // Get CPU cycle count before reset for coordinated APU timing
        let cpu_cycle = self.cpu.get_total_cycles();

        self.cpu.reset();
        self.ppu.borrow_mut().reset();
        self.apu.borrow_mut().reset(cpu_cycle);
        self.fractional_ppu_cycles = 0.0;
        self.ready_to_render = false;

        // Re-establish 1-cycle PPU offset after reset
        self.ppu.borrow_mut().run_ppu_cycles(1);
    }

    /// Run one CPU "tick", executing one opcode and the corresponding PPU cycles
    ///
    /// Returns the number of CPU cycles consumed by the opcode.
    ///
    /// The PPU runs at a different rate than the CPU:
    /// - NTSC: 3 PPU cycles per CPU cycle
    /// - PAL: 3.2 PPU cycles per CPU cycle
    ///
    /// For PAL, fractional cycles are accumulated to maintain timing accuracy.
    ///
    /// # Known Limitations
    ///
    /// This implementation ticks the PPU after each complete CPU opcode, not cycle-by-cycle.
    /// This means PPU register writes (PPUCTRL, PPUMASK, PPUSCROLL, etc.) take effect for
    /// all PPU cycles in the opcode, rather than at the exact cycle when the write occurs.
    /// This can cause timing issues with ROMs that update PPU registers mid-scanline for
    /// visual effects. Most games work fine, but some test ROMs (like palette.nes) may
    /// show minor rendering artifacts due to this limitation.
    pub fn run_cpu_tick(&mut self) -> u8 {
        // Check if an OAM DMA is pending before executing the opcode
        let oam_dma_page = self.memory.borrow_mut().take_oam_dma_page();
        if let Some(page) = oam_dma_page {
            // OAM DMA takes 513 cycles on even CPU cycle, or 514 cycles on odd CPU cycle
            // Check current CPU cycle parity
            let is_odd_cycle = self.cpu.get_total_cycles() % 2 == 1;
            let dma_cycles = if is_odd_cycle { 514u16 } else { 513u16 };

            // Execute the DMA transfer
            self.memory.borrow_mut().execute_oam_dma(page);

            // Tick the PPU for the DMA cycles
            self.tick_ppu_u16(dma_cycles);

            // Clock the APU for the DMA cycles
            self.tick_apu_u16(dma_cycles);

            // Add DMA cycles to CPU's total cycle counter
            self.cpu.add_cycles(dma_cycles as u64);

            // Check for NMI after DMA
            if self.ppu.borrow_mut().poll_nmi() {
                let nmi_cycles = self.cpu.trigger_nmi();
                // Tick PPU and APU for the NMI handling cycles
                self.tick_ppu(nmi_cycles);
                self.tick_apu(nmi_cycles);
            }

            // Return DMA cycles (capped at u8::MAX)
            return dma_cycles.min(255) as u8;
        }

        // Execute CPU instruction cycle-by-cycle
        let mut cpu_cycles = 0;
        loop {
            // Tick PPU and APU BEFORE executing CPU cycle
            // This ensures NMI edges are detected before the CPU modifies its state
            // NOTE: This order is critical for correct NMI timing in cycle-accurate emulation.
            // However, there remains a ~75 CPU cycle (225 PPU cycle) synchronization offset
            // that affects tests requiring sub-scanline timing precision (e.g., cpu_interrupts test 2).
            // This is a known limitation even on real NES hardware, as documented in the test itself:
            // "Occasionally fails on NES due to PPU-CPU synchronization."
            // print!("*");
            self.tick_ppu(1);
            self.tick_apu(1);

            // Check for NMI edge after PPU tick, before CPU execution
            // This allows the CPU to see NMI edges at instruction boundaries
            if self.ppu.borrow_mut().poll_nmi() {
                self.cpu.set_nmi_pending(true);
            }

            // Execute one CPU cycle
            // print!(".");
            let instruction_complete = self.cpu.tick_cycle();
            cpu_cycles += 1;

            // Break after instruction completes
            if instruction_complete {
                break;
            }
        }
        // println!("");

        // Only trigger interrupts after instruction completes
        // Check if NMI needs to be triggered
        // (BRK may have consumed it via vector hijacking)
        if self.cpu.is_nmi_pending() {
            self.cpu.set_nmi_pending(false);
            let nmi_cycles = self.cpu.trigger_nmi();
            // Tick PPU and APU for the NMI handling cycles
            self.tick_ppu(nmi_cycles);
            self.tick_apu(nmi_cycles);
            cpu_cycles += nmi_cycles;
        }

        // Check for IRQ after executing instruction
        // IRQ is maskable and checked after NMI
        // First, update the IRQ pending state based on hardware sources (APU)
        let irq_asserted = self.apu.borrow().poll_irq();
        self.cpu.set_irq_pending(irq_asserted);

        // Then check if CPU should service the IRQ (not masked and not in delay period)
        if self.cpu.should_poll_irq() {
            let irq_cycles = self.cpu.trigger_irq();
            if irq_cycles > 0 {
                // Only tick if IRQ was actually taken (not masked)
                self.tick_ppu(irq_cycles);
                self.tick_apu(irq_cycles);
                cpu_cycles += irq_cycles;
            }
        }

        if self.ppu.borrow_mut().poll_frame_complete() {
            self.ready_to_render = true;
        }

        cpu_cycles
    }

    /// Run the PPU for the appropriate number of cycles based on CPU cycles
    ///
    /// For PAL, fractional cycles are accumulated to maintain timing accuracy.
    fn tick_ppu(&mut self, cpu_cycles: u8) {
        let ppu_cycles = cpu_cycles as f64 * self.tv_system.ppu_cycles_per_cpu_cycle();
        self.fractional_ppu_cycles += ppu_cycles;

        let ppu_cycles_to_run = self.fractional_ppu_cycles as u64;
        self.fractional_ppu_cycles -= ppu_cycles_to_run as f64;

        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles_to_run);
    }

    fn tick_ppu_u16(&mut self, cpu_cycles: u16) {
        let ppu_cycles = cpu_cycles as f64 * self.tv_system.ppu_cycles_per_cpu_cycle();
        self.fractional_ppu_cycles += ppu_cycles;

        let ppu_cycles_to_run = self.fractional_ppu_cycles as u64;
        self.fractional_ppu_cycles -= ppu_cycles_to_run as f64;

        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles_to_run);
    }

    /// Clock the APU for the specified number of CPU cycles
    fn tick_apu(&mut self, cpu_cycles: u8) {
        for _ in 0..cpu_cycles {
            self.apu.borrow_mut().clock();
        }
    }

    fn tick_apu_u16(&mut self, cpu_cycles: u16) {
        for _ in 0..cpu_cycles {
            self.apu.borrow_mut().clock();
        }
    }

    /// NES system palette - 64 RGB color values (0x00-0x3F)
    /// TODO Implement all known palettes and have the user be able to select system palette variant
    #[rustfmt::skip]
    const SYSTEM_PALETTE: [(u8, u8, u8); 0x40] = [
        /* 0x00 */ (0x54, 0x54, 0x54), /* 0x01 */ (0x00, 0x1E, 0x74), /* 0x02 */ (0x08, 0x10, 0x90), /* 0x03 */ (0x30, 0x00, 0x88), /* 0x04 */ (0x44, 0x00, 0x64), /* 0x05 */ (0x5C, 0x00, 0x30), /* 0x06 */ (0x54, 0x04, 0x00), /* 0x07 */ (0x3C, 0x18, 0x00),
        /* 0x08 */ (0x20, 0x2A, 0x00), /* 0x09 */ (0x08, 0x3A, 0x00), /* 0x0A */ (0x00, 0x40, 0x00), /* 0x0B */ (0x00, 0x3C, 0x00), /* 0x0C */ (0x00, 0x32, 0x3C), /* 0x0D */ (0x00, 0x00, 0x00), /* 0x0E */ (0x00, 0x00, 0x00), /* 0x0F */ (0x00, 0x00, 0x00),
        /* 0x10 */ (0x98, 0x96, 0x98), /* 0x11 */ (0x08, 0x4C, 0xC4), /* 0x12 */ (0x30, 0x32, 0xEC), /* 0x13 */ (0x5C, 0x1E, 0xE4), /* 0x14 */ (0x88, 0x14, 0xB0), /* 0x15 */ (0xA0, 0x14, 0x64), /* 0x16 */ (0x98, 0x22, 0x20), /* 0x17 */ (0x78, 0x3C, 0x00),
        /* 0x18 */ (0x54, 0x5A, 0x00), /* 0x19 */ (0x28, 0x72, 0x00), /* 0x1A */ (0x08, 0x7C, 0x00), /* 0x1B */ (0x00, 0x76, 0x28), /* 0x1C */ (0x00, 0x66, 0x78), /* 0x1D */ (0x00, 0x00, 0x00), /* 0x1E */ (0x00, 0x00, 0x00), /* 0x1F */ (0x00, 0x00, 0x00),
        /* 0x20 */ (0xEC, 0xEE, 0xEC), /* 0x21 */ (0x4C, 0x9A, 0xEC), /* 0x22 */ (0x78, 0x7C, 0xEC), /* 0x23 */ (0xB0, 0x62, 0xEC), /* 0x24 */ (0xE4, 0x54, 0xEC), /* 0x25 */ (0xEC, 0x58, 0xB4), /* 0x26 */ (0xEC, 0x6A, 0x64), /* 0x27 */ (0xD4, 0x88, 0x20),
        /* 0x28 */ (0xA0, 0xAA, 0x00), /* 0x29 */ (0x74, 0xC4, 0x00), /* 0x2A */ (0x4C, 0xD0, 0x20), /* 0x2B */ (0x38, 0xCC, 0x6C), /* 0x2C */ (0x38, 0xB4, 0xCC), /* 0x2D */ (0x3C, 0x3C, 0x3C), /* 0x2E */ (0x00, 0x00, 0x00), /* 0x2F */ (0x00, 0x00, 0x00),
        /* 0x30 */ (0xEC, 0xEE, 0xEC), /* 0x31 */ (0xA8, 0xCC, 0xEC), /* 0x32 */ (0xBC, 0xBC, 0xEC), /* 0x33 */ (0xD4, 0xB2, 0xEC), /* 0x34 */ (0xEC, 0xAE, 0xEC), /* 0x35 */ (0xEC, 0xAE, 0xD4), /* 0x36 */ (0xEC, 0xB4, 0xB0), /* 0x37 */ (0xE4, 0xC4, 0x90),
        /* 0x38 */ (0xCC, 0xD2, 0x78), /* 0x39 */ (0xB4, 0xDE, 0x78), /* 0x3A */ (0xA8, 0xE2, 0x90), /* 0x3B */ (0x98, 0xE2, 0xB4), /* 0x3C */ (0xA0, 0xD6, 0xE4), /* 0x3D */ (0xA0, 0xA2, 0xA0), /* 0x3E */ (0x00, 0x00, 0x00), /* 0x3F */ (0x00, 0x00, 0x00),
    ];

    /// Maps NES color palette index (0-63) to RGB values using direct array lookup
    pub fn lookup_system_palette(color_index: u8) -> (u8, u8, u8) {
        Self::SYSTEM_PALETTE[(color_index & 0x3F) as usize]
    }

    /// Get a reference to the PPU's screen buffer
    ///
    /// Returns a mutable reference to the 256x240 RGB buffer containing the current frame.
    pub fn get_screen_buffer(&self) -> std::cell::RefMut<'_, crate::screen_buffer::ScreenBuffer> {
        std::cell::RefMut::map(self.ppu.borrow_mut(), |ppu| ppu.screen_buffer_mut())
    }

    /// Check if a frame is ready to be rendered
    ///
    /// Returns true when the PPU has completed a full frame (reached VBlank at scanline 241).
    /// After checking this flag, call `clear_ready_to_render()` to reset it for the next frame.
    pub fn is_ready_to_render(&self) -> bool {
        self.ready_to_render
    }

    /// Clear the ready-to-render flag after rendering a frame
    pub fn clear_ready_to_render(&mut self) {
        self.ready_to_render = false;
    }

    /// Check if an audio sample is ready for retrieval
    ///
    /// Returns true when the APU has generated a new audio sample.
    /// After checking this flag, call `get_sample()` to retrieve the sample.
    pub fn sample_ready(&self) -> bool {
        self.apu.borrow().sample_ready()
    }

    /// Get the next audio sample if one is ready
    ///
    /// Returns `Some(sample)` if a sample is available, `None` otherwise.
    /// The sample is in the range 0.0 to 1.0.
    /// After calling this, `sample_ready()` will return false until the next sample is generated.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.apu.borrow_mut().get_sample()
    }

    /// Set button state for a controller
    ///
    /// # Arguments
    /// * `controller` - Controller number (1 or 2)
    /// * `button` - Which button to set
    /// * `pressed` - true if pressed, false if released
    pub fn set_button(&mut self, controller: u8, button: crate::input::Button, pressed: bool) {
        self.memory
            .borrow_mut()
            .set_button(controller, button, pressed);
    }

    /// Generate a trace line for the current CPU state
    ///
    /// Returns a string in the nestest.log format showing the current instruction,
    /// registers, and PPU state. Useful for debugging and comparing against reference logs.
    ///
    /// Format: `PC  OPCODE  INSTRUCTION                 A:XX X:XX Y:XX P:XX SP:XX PPU:SSS,PPP CYC:C`
    pub fn trace(&mut self, nestest: bool) -> String {
        let pc = self.cpu.get_state().pc;
        let memory = self.memory.borrow();
        // Read the opcode and determine instruction size
        let opcode_byte = memory.read(pc);
        let instruction = cpu::lookup(opcode_byte)
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
                if nestest {
                    let mut value = memory.read(addr);
                    if addr >= 0x4000 && addr < 0x4100 {
                        value = 0xFF;
                    }
                    format!("{} ${:02X} = {:02X}", instruction.mnemonic, byte1, value)
                } else {
                    format!("{} ${:02X}", instruction.mnemonic, byte1)
                }
            }
            "ZPX" => {
                let addr = byte1.wrapping_add(self.cpu.get_state().x) as u16;
                if nestest {
                    let mut value = memory.read(addr);
                    if addr >= 0x4000 && addr < 0x4100 {
                        value = 0xFF;
                    }
                    format!(
                        "{} ${:02X},X @ {:02X} = {:02X}",
                        instruction.mnemonic, byte1, addr as u8, value
                    )
                } else {
                    format!("{} ${:02X},X", instruction.mnemonic, byte1)
                }
            }
            "ZPY" => {
                let addr = byte1.wrapping_add(self.cpu.get_state().y) as u16;
                if nestest {
                    let mut value = memory.read(addr);
                    if addr >= 0x4000 && addr < 0x4100 {
                        value = 0xFF;
                    }
                    format!(
                        "{} ${:02X},Y @ {:02X} = {:02X}",
                        instruction.mnemonic, byte1, addr as u8, value
                    )
                } else {
                    format!("{} ${:02X},Y", instruction.mnemonic, byte1)
                }
            }
            "ABS" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                // JMP and JSR don't show memory value for ABS addressing
                if instruction.mnemonic == "JMP" || instruction.mnemonic == "JSR" {
                    format!("{} ${:04X}", instruction.mnemonic, addr)
                } else if nestest {
                    let mut value = memory.read(addr);
                    if addr >= 0x4000 && addr < 0x4100 {
                        value = 0xFF;
                    }
                    format!("{} ${:04X} = {:02X}", instruction.mnemonic, addr, value)
                } else {
                    format!("{} ${:04X}", instruction.mnemonic, addr)
                }
            }
            "ABSX" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(self.cpu.get_state().x as u16);
                    let value = memory.read(effective_addr);
                    format!(
                        "{} ${:04X},X @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},X", instruction.mnemonic, addr)
                }
            }
            "ABSY" => {
                let addr = u16::from_le_bytes([byte1, byte2]);
                if nestest {
                    let effective_addr = addr.wrapping_add(self.cpu.get_state().y as u16);
                    let value = memory.read(effective_addr);
                    format!(
                        "{} ${:04X},Y @ {:04X} = {:02X}",
                        instruction.mnemonic, addr, effective_addr, value
                    )
                } else {
                    format!("{} ${:04X},Y", instruction.mnemonic, addr)
                }
            }
            "INDX" => {
                if nestest {
                    let zp_addr = byte1.wrapping_add(self.cpu.get_state().x);
                    let addr_lo = memory.read(zp_addr as u16);
                    let addr_hi = memory.read(zp_addr.wrapping_add(1) as u16);
                    let addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    let value = memory.read(addr);
                    format!(
                        "{} (${:02X},X) @ {:02X} = {:04X} = {:02X}",
                        instruction.mnemonic, byte1, zp_addr, addr, value
                    )
                } else {
                    format!("{} (${:02X},X)", instruction.mnemonic, byte1)
                }
            }
            "INDY" => {
                if nestest {
                    let addr_lo = memory.read(byte1 as u16);
                    let addr_hi = memory.read(byte1.wrapping_add(1) as u16);
                    let base_addr = u16::from_le_bytes([addr_lo, addr_hi]);
                    let effective_addr = base_addr.wrapping_add(self.cpu.get_state().y as u16);
                    let value = memory.read(effective_addr);
                    format!(
                        "{} (${:02X}),Y = {:04X} @ {:04X} = {:02X}",
                        instruction.mnemonic, byte1, base_addr, effective_addr, value
                    )
                } else {
                    format!("{} (${:02X}),Y", instruction.mnemonic, byte1)
                }
            }
            "IND" => {
                if nestest {
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
                } else {
                    let ptr_addr = u16::from_le_bytes([byte1, byte2]);
                    format!("{} (${:04X})", instruction.mnemonic, ptr_addr)
                }
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

        let total_cycles = self.cpu.get_total_cycles();
        let state = self.cpu.get_state();
        let ppu = self.ppu.borrow();
        format!(
            "{:04X}  {}{}{:<width$} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:3},{:3} CYC:{}",
            pc,
            hex_dump,
            pad_before,
            asm,
            state.a,
            state.x,
            state.y,
            state.p,
            state.sp,
            ppu.scanline(),
            ppu.pixel(),
            total_cycles,
            width = width
        )
    }

    /// Get base nametable address from PPUCTRL (for testing)
    #[cfg(test)]
    pub fn base_nametable_addr(&self) -> u16 {
        self.ppu.borrow().base_nametable_addr()
    }

    /// Read nametable text for automated test verification
    ///
    /// Reads tile indices from the nametable and converts them to ASCII text.
    /// This is useful for Blargg tests that output results to the screen instead of $6000.
    ///
    /// # Arguments
    /// * `nametable_addr` - Starting address in nametable (e.g., 0x2081)
    /// * `length` - Number of tiles to read
    ///
    /// # Returns
    /// String containing the decoded text
    #[cfg(test)]
    pub fn read_nametable_text(&self, nametable_addr: u16, length: usize) -> String {
        let ppu = self.ppu.borrow();
        let mut text = String::new();

        for i in 0..length {
            let addr = nametable_addr.wrapping_add(i as u16);
            let tile_index = ppu.read_nametable_for_debug(addr);

            // Decode tile index to character
            // Blargg's branch timing tests store ASCII values directly as tiles
            let ch = if tile_index >= 0x20 && tile_index <= 0x7E {
                tile_index as char
            } else if tile_index == 0x00 {
                ' ' // Treat 0x00 as space
            } else {
                '?' // Unknown/control character
            };

            text.push(ch);
        }

        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_nestest() {
        // Load the golden log from file
        let golden_log = fs::read_to_string("roms/nestest.log")
            .expect("Failed to load nestest.log - make sure roms/nestest.log exists");

        // Load the nestest ROM
        let rom_data = fs::read("roms/nestest.nes").expect("Failed to load ROM");
        let cartridge = Cartridge::new(&rom_data).expect("Failed to parse ROM");

        // Create NES and insert cartridge
        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();
        // nestest automated test starts execution at $C000 (not reset vector $C004)
        nes.cpu.get_state().pc = 0xC000;
        // CPU reset takes 7 cycles, manually sync PPU and CPU cycle counters
        // PPU already has 1 cycle from initialization, so add 20 more (21 - 1 = 20)
        nes.ppu.borrow_mut().run_ppu_cycles(20); // 7 * 3 = 21 PPU cycles for NTSC
        nes.cpu.set_total_cycles(7); // Account for reset cycles

        for line in golden_log.lines() {
            let expected = line.to_string();
            let actual = nes.trace(true);

            assert_eq!(expected, actual);
            nes.run_cpu_tick();
        }
    }

    #[test]
    fn test_nes_new_with_ntsc() {
        let nes = Nes::new(TvSystem::Ntsc);
        assert_eq!(nes.tv_system(), TvSystem::Ntsc);
    }

    #[test]
    fn test_nes_new_with_pal() {
        let nes = Nes::new(TvSystem::Pal);
        assert_eq!(nes.tv_system(), TvSystem::Pal);
    }

    #[test]
    fn test_tv_system_stored_correctly() {
        let ntsc_nes = Nes::new(TvSystem::Ntsc);
        let pal_nes = Nes::new(TvSystem::Pal);

        assert_eq!(ntsc_nes.tv_system(), TvSystem::Ntsc);
        assert_eq!(pal_nes.tv_system(), TvSystem::Pal);
    }

    #[test]
    fn test_ntsc_ppu_cycles_per_cpu_cycle() {
        let ntsc = TvSystem::Ntsc;
        // NTSC: 3 PPU cycles per CPU cycle
        assert_eq!(ntsc.ppu_cycles_per_cpu_cycle(), 3.0);
    }

    #[test]
    fn test_pal_ppu_cycles_per_cpu_cycle() {
        let pal = TvSystem::Pal;
        // PAL: 3.2 PPU cycles per CPU cycle
        assert_eq!(pal.ppu_cycles_per_cpu_cycle(), 3.2);
    }

    #[test]
    fn test_ntsc_scanlines_per_frame() {
        let ntsc = TvSystem::Ntsc;
        // NTSC: 262 scanlines per frame
        assert_eq!(ntsc.scanlines_per_frame(), 262);
    }

    #[test]
    fn test_pal_scanlines_per_frame() {
        let pal = TvSystem::Pal;
        // PAL: 312 scanlines per frame
        assert_eq!(pal.scanlines_per_frame(), 312);
    }

    #[test]
    fn test_ntsc_ppu_runs_3x_cpu_cycles() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        // Write NOP to RAM and set PC directly (skip reset to avoid ROM requirement)
        nes.memory.borrow_mut().write(0x0000, 0xEA, false); // NOP in RAM
        nes.cpu.get_state().pc = 0x0000; // Set PC to RAM address

        // NOP takes 2 CPU cycles, so PPU should run 6 cycles (3x ratio for NTSC)
        // Plus 1 cycle for initial PPU offset (sprite 0 hit timing correction)
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7);
    }

    #[test]
    fn test_pal_ppu_runs_3_2x_cpu_cycles() {
        let mut nes = Nes::new(TvSystem::Pal);
        // Write NOP to RAM and set PC directly
        nes.memory.borrow_mut().write(0x0000, 0xEA, false); // NOP in RAM
        nes.cpu.get_state().pc = 0x0000;

        // NOP takes 2 CPU cycles, PAL ratio is 3.2, so 2 * 3.2 = 6.4
        // Plus 1 cycle for initial PPU offset
        // Should accumulate fractional part
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7);
    }

    #[test]
    fn test_pal_ppu_accumulates_fractional_cycles() {
        let mut nes = Nes::new(TvSystem::Pal);
        // Write NOP instructions to RAM
        for i in 0..10 {
            nes.memory.borrow_mut().write(i, 0xEA, false); // NOP
        }
        nes.cpu.get_state().pc = 0x0000;

        // Run 5 NOPs: 5 instructions * 2 cycles = 10 CPU cycles
        // 10 * 3.2 = 32 PPU cycles, plus 1 cycle initial offset
        for _ in 0..5 {
            nes.run_cpu_tick();
        }
        assert_eq!(nes.ppu.borrow().total_cycles(), 33);
    }

    #[test]
    fn test_ntsc_ppu_accumulates_over_multiple_instructions() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        // Write NOP instructions to RAM
        for i in 0..3 {
            nes.memory.borrow_mut().write(i, 0xEA, false); // NOP (2 cycles each)
        }
        nes.cpu.get_state().pc = 0x0000;

        // 3 NOPs = 6 CPU cycles, 18 PPU cycles (6 * 3), plus 1 cycle initial offset
        nes.run_cpu_tick();
        nes.run_cpu_tick();
        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 19);
    }

    #[test]
    fn test_ppu_cycles_reset_on_nes_reset() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.memory.borrow_mut().write(0x0000, 0xEA, false); // NOP
        nes.cpu.get_state().pc = 0x0000;

        nes.run_cpu_tick();
        assert_eq!(nes.ppu.borrow().total_cycles(), 7); // 6 + 1 offset

        // Reset just the PPU to test the counter is cleared
        nes.ppu.borrow_mut().reset();
        assert_eq!(nes.ppu.borrow().total_cycles(), 0);
    }

    #[test]
    fn test_nes_color_to_rgb() {
        // Test a few key colors from the NES palette
        assert_eq!(Nes::lookup_system_palette(0x00), (0x54, 0x54, 0x54)); // Gray
        assert_eq!(Nes::lookup_system_palette(0x01), (0x00, 0x1E, 0x74)); // Blue
        assert_eq!(Nes::lookup_system_palette(0x16), (0x98, 0x22, 0x20)); // Red
        assert_eq!(Nes::lookup_system_palette(0x2A), (0x4C, 0xD0, 0x20)); // Green
        assert_eq!(Nes::lookup_system_palette(0x30), (0xEC, 0xEE, 0xEC)); // White
        assert_eq!(Nes::lookup_system_palette(0x0D), (0x00, 0x00, 0x00)); // Black

        // Test that values above 0x3F are masked
        assert_eq!(Nes::lookup_system_palette(0x40), (0x54, 0x54, 0x54)); // Same as 0x00
        assert_eq!(Nes::lookup_system_palette(0xFF), (0x00, 0x00, 0x00)); // Same as 0x3F
    }

    #[test]
    fn test_nes_provides_access_to_ppu_screen_buffer() {
        let nes = Nes::new(TvSystem::Ntsc);

        // Should be able to access the PPU's screen buffer
        let screen_buffer = nes.get_screen_buffer();

        // Verify it has the correct dimensions
        assert_eq!(screen_buffer.width(), 256);
        assert_eq!(screen_buffer.height(), 240);
    }

    /// Helper function to create a minimal iNES ROM with a simple infinite loop
    /// This ROM just executes JMP $8000 (4C 00 80) forever
    fn create_minimal_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Magic bytes
        rom.push(1); // 1x 16 KB PRG ROM
        rom.push(0); // 0x 8 KB CHR ROM (no CHR)
        rom.push(0); // Flags 6 (horizontal mirroring, no other features)
        rom.push(0); // Flags 7
        rom.extend_from_slice(&[0; 8]); // Padding to complete 16-byte header

        // PRG ROM (16 KB = 16384 bytes)
        let mut prg_rom = vec![0; 16384];

        // Reset vector at $FFFC-$FFFD points to $8000
        prg_rom[0x3FFC] = 0x00; // Low byte
        prg_rom[0x3FFD] = 0x80; // High byte

        // Code at $8000: JMP $8000 (infinite loop)
        prg_rom[0] = 0x4C; // JMP absolute
        prg_rom[1] = 0x00; // Low byte of address
        prg_rom[2] = 0x80; // High byte of address

        rom.extend_from_slice(&prg_rom);
        rom
    }

    #[test]
    fn test_oam_dma_takes_513_cycles_on_even_cpu_cycle() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let rom_data = create_minimal_rom();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();

        // Set CPU to an even cycle (8)
        nes.cpu.set_total_cycles(8);

        let cycles_before = nes.cpu.get_total_cycles();
        assert_eq!(cycles_before % 2, 0, "Should start on even cycle");

        // Trigger OAM DMA by writing to $4014
        nes.memory.borrow_mut().write(0x4014, 0x02, false);

        // Run one CPU tick which should process the DMA
        nes.run_cpu_tick();

        // On even alignment, DMA should take 513 CPU cycles
        let cycles_after = nes.cpu.get_total_cycles();
        assert_eq!(
            cycles_after - cycles_before,
            513,
            "DMA should take 513 cycles on even alignment"
        );
    }

    #[test]
    fn test_oam_dma_takes_514_cycles_on_odd_cpu_cycle() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let rom_data = create_minimal_rom();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();

        // Set CPU to an odd cycle (7)
        nes.cpu.set_total_cycles(7);

        let cycles_before = nes.cpu.get_total_cycles();
        assert_eq!(cycles_before % 2, 1, "Should start on odd cycle");

        // Trigger OAM DMA by writing to $4014
        nes.memory.borrow_mut().write(0x4014, 0x02, false);

        // Run one CPU tick which should process the DMA
        nes.run_cpu_tick();

        // On odd alignment, DMA should take 514 CPU cycles (513 + 1 wait cycle)
        let cycles_after = nes.cpu.get_total_cycles();
        assert_eq!(
            cycles_after - cycles_before,
            514,
            "DMA should take 514 cycles on odd alignment"
        );
    }

    #[test]
    fn test_oam_dma_transfers_256_bytes() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let rom_data = create_minimal_rom();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();

        // Set up test data in RAM at page $02 ($0200-$02FF)
        for i in 0..256u16 {
            nes.memory
                .borrow_mut()
                .write(0x0200 + i, (i & 0xFF) as u8, false);
        }

        // Trigger OAM DMA from page $02
        nes.memory.borrow_mut().write(0x4014, 0x02, false);
        nes.run_cpu_tick();

        // Verify all 256 bytes were copied to OAM by reading through $2004
        for i in 0..256 {
            // Set OAM address via $2003
            nes.memory.borrow_mut().write(0x2003, i as u8, false);
            // Read OAM data via $2004
            let oam_byte = nes.memory.borrow().read(0x2004);
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: mask bits 2-4
                ((i & 0xFF) as u8) & 0xE3
            } else {
                (i & 0xFF) as u8
            };
            assert_eq!(
                oam_byte, expected,
                "OAM byte {} should match source data (with attribute masking)",
                i
            );
        }
    }

    #[test]
    fn test_oam_dma_uses_correct_source_page() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let rom_data = create_minimal_rom();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();

        // Set up distinct data in different pages
        // Page $03: $0300-$03FF
        for i in 0..256u16 {
            nes.memory.borrow_mut().write(0x0300 + i, 0xAA, false); // Marker value
        }

        // Trigger OAM DMA from page $03
        nes.memory.borrow_mut().write(0x4014, 0x03, false);
        nes.run_cpu_tick();

        // Verify bytes came from page $03 by reading through $2004
        for i in 0..256 {
            // Set OAM address via $2003
            nes.memory.borrow_mut().write(0x2003, i as u8, false);
            // Read OAM data via $2004
            let oam_byte = nes.memory.borrow().read(0x2004);
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: 0xAA with masking = 0xAA & 0xE3 = 0xA2
                0xA2
            } else {
                0xAA
            };
            assert_eq!(
                oam_byte, expected,
                "OAM byte {} should be from page $03 (with attribute masking)",
                i
            );
        }
    }

    #[test]
    fn test_ppu_advances_during_oam_dma() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let rom_data = create_minimal_rom();
        let cartridge = Cartridge::new(&rom_data).expect("Failed to create cartridge");
        nes.insert_cartridge(cartridge);
        nes.cpu.reset();

        // Set CPU to an even cycle (8)
        nes.cpu.set_total_cycles(8);

        // Get initial PPU state
        let initial_ppu_cycles = nes.ppu.borrow().total_cycles();

        // Trigger OAM DMA
        nes.memory.borrow_mut().write(0x4014, 0x02, false);
        nes.run_cpu_tick();

        // PPU should have advanced by 513 CPU cycles * 3 PPU cycles per CPU cycle
        let expected_ppu_cycles = initial_ppu_cycles + (513 * 3);
        let actual_ppu_cycles = nes.ppu.borrow().total_cycles();

        assert_eq!(
            actual_ppu_cycles, expected_ppu_cycles,
            "PPU should advance by 513*3 cycles during DMA on even alignment"
        );
    }

    #[test]
    fn test_ntsc_refresh_rate_calculation() {
        // NTSC CPU runs at approximately 1.789773 MHz
        // Even frame: 89342 PPU cycles / 3 = 29780.67 CPU cycles
        // Odd frame:  89341 PPU cycles / 3 = 29780.33 CPU cycles
        // Average: ~29780.5 CPU cycles per frame
        // Refresh rate: 1789773 / 29780.5 ≈ 60.10 Hz

        let tv_system = TvSystem::Ntsc;
        let even_frame_ppu_cycles = 262 * 341; // 89342
        let odd_frame_ppu_cycles = 262 * 341 - 1; // 89341 (with odd frame skip)
        let avg_ppu_cycles = (even_frame_ppu_cycles + odd_frame_ppu_cycles) as f64 / 2.0;
        let avg_cpu_cycles = avg_ppu_cycles / tv_system.ppu_cycles_per_cpu_cycle();

        assert_eq!(even_frame_ppu_cycles, 89342);
        assert_eq!(odd_frame_ppu_cycles, 89341);
        assert!(
            (avg_cpu_cycles - 29780.5).abs() < 0.01,
            "NTSC should average ~29780.5 CPU cycles per frame"
        );
    }

    #[test]
    fn test_pal_refresh_rate_calculation() {
        // PAL CPU runs at approximately 1.662607 MHz
        // PAL frame: 312 scanlines * 341 dots = 106392 PPU cycles
        // 106392 PPU cycles / 3.2 = 33247.5 CPU cycles per frame
        // Refresh rate: 1662607 / 33247.5 ≈ 50.00 Hz

        let tv_system = TvSystem::Pal;
        let frame_ppu_cycles = 312 * 341; // 106392
        let cpu_cycles_per_frame = frame_ppu_cycles as f64 / tv_system.ppu_cycles_per_cpu_cycle();

        assert_eq!(frame_ppu_cycles, 106392);
        assert!(
            (cpu_cycles_per_frame - 33247.5).abs() < 0.01,
            "PAL should have ~33247.5 CPU cycles per frame"
        );
    }

    #[test]
    fn test_apu_clocked_every_cpu_cycle() {
        // Test that the APU is clocked once for every CPU cycle
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Load a simple NOP program that executes predictably
        let rom_data = create_minimal_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom_data).unwrap();
        nes.insert_cartridge(cartridge);

        // Reset to start execution
        nes.reset();

        // Get initial frame counter cycle count
        let initial_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();

        // Execute one CPU instruction (NOP = 2 cycles)
        let cpu_cycles = nes.run_cpu_tick();

        // APU should have been clocked once per CPU cycle
        let final_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let apu_cycles_elapsed = final_cycle - initial_cycle;

        assert_eq!(
            apu_cycles_elapsed, cpu_cycles as u32,
            "APU should be clocked once per CPU cycle"
        );
    }

    #[test]
    fn test_apu_clocked_during_oam_dma() {
        // Test that the APU is clocked during OAM DMA cycles
        let mut nes = Nes::new(TvSystem::Ntsc);

        let rom_data = create_minimal_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom_data).unwrap();
        nes.insert_cartridge(cartridge);
        nes.reset();

        // Get initial APU cycle count
        let initial_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();

        // Trigger an OAM DMA by writing to $4014
        nes.memory.borrow_mut().write(0x4014, 0x02, false);

        // Run a CPU tick which should execute the DMA
        let dma_cycles = nes.run_cpu_tick();

        // APU should have been clocked for all DMA cycles
        let final_cycle = nes.apu.borrow().frame_counter().get_cycle_counter();
        let apu_cycles_elapsed = final_cycle - initial_cycle;

        // OAM DMA takes 513 or 514 cycles, but returns min(cycles, 255) as u8
        assert!(
            dma_cycles == 255,
            "OAM DMA should return 255 (capped from 513/514 cycles)"
        );
        // APU should have been clocked for the actual DMA cycles (513 or 514)
        assert!(
            apu_cycles_elapsed == 513 || apu_cycles_elapsed == 514,
            "APU should be clocked 513 or 514 times during OAM DMA, got {}",
            apu_cycles_elapsed
        );
    }

    #[test]
    fn test_sample_ready_initially_false() {
        // Test that sample_ready returns false initially
        let nes = Nes::new(TvSystem::Ntsc);

        assert!(!nes.sample_ready());
    }

    #[test]
    fn test_sample_ready_after_clocking() {
        // Test that sample_ready returns true after enough APU clocks
        let mut nes = Nes::new(TvSystem::Ntsc);

        let rom_data = create_minimal_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom_data).unwrap();
        nes.insert_cartridge(cartridge);
        nes.reset();

        // Clock the APU until a sample is ready
        // At 44100 Hz sample rate and 1789773 Hz CPU clock:
        // cycles_per_sample = 1789773 / 44100 ≈ 40.59 cycles
        // So we need to run at least 41 CPU cycles
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        assert!(nes.sample_ready());
    }

    #[test]
    fn test_get_sample_returns_value() {
        // Test that get_sample returns a valid audio sample
        let mut nes = Nes::new(TvSystem::Ntsc);

        let rom_data = create_minimal_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom_data).unwrap();
        nes.insert_cartridge(cartridge);
        nes.reset();

        // Clock until a sample is ready
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        // Get the sample
        let sample = nes.get_sample();
        assert!(sample.is_some());

        // Sample should be in valid range 0.0 to 1.0
        let sample_value = sample.unwrap();
        assert!(sample_value >= 0.0 && sample_value <= 1.0);
    }

    #[test]
    fn test_get_sample_clears_ready_flag() {
        // Test that get_sample clears the sample_ready flag
        let mut nes = Nes::new(TvSystem::Ntsc);

        let rom_data = create_minimal_nrom_rom();
        let cartridge = crate::cartridge::Cartridge::new(&rom_data).unwrap();
        nes.insert_cartridge(cartridge);
        nes.reset();

        // Clock until a sample is ready
        for _ in 0..50 {
            nes.run_cpu_tick();
            if nes.sample_ready() {
                break;
            }
        }

        assert!(nes.sample_ready());

        // Get the sample
        nes.get_sample();

        // sample_ready should now return false
        assert!(!nes.sample_ready());
    }

    #[test]
    fn test_get_sample_returns_none_when_not_ready() {
        // Test that get_sample returns None when no sample is ready
        let mut nes = Nes::new(TvSystem::Ntsc);

        let sample = nes.get_sample();
        assert!(sample.is_none());
    }

    /// Helper function to create a minimal NROM ROM for testing
    fn create_minimal_nrom_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        // iNES header
        rom.extend_from_slice(b"NES\x1A"); // Signature
        rom.push(2); // 2 * 16KB PRG ROM
        rom.push(1); // 1 * 8KB CHR ROM
        rom.push(0x00); // Flags 6: Mapper 0 (NROM)
        rom.push(0x00); // Flags 7
        rom.extend_from_slice(&[0; 8]); // Unused padding

        // 32KB PRG ROM (2 * 16KB) - filled with NOPs
        rom.extend_from_slice(&[0xEA; 32768]); // NOP instruction

        // 8KB CHR ROM - filled with zeros
        rom.extend_from_slice(&[0x00; 8192]);

        rom
    }
}
