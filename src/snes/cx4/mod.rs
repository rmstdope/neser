//! Capcom CX4 enhancement chip (a Hitachi HG51B169 RISC CPU), used by Mega Man X2 and X3.
//!
//! The SNES sees the chip at `$00-$3F/$80-$BF:$6000-$7FFF`: 3 KB of data RAM, the DMA/program
//! control ports at `$7F40-$7F5F`, 32 vector bytes at `$7F60-$7F7F` and the sixteen 24-bit
//! registers R0-R15 at `$7F80-$7FAF` (fullsnes "SNES Cart Capcom CX4 - I/O Ports"). Writing the
//! program counter to `$7F4F` loads a 256-word page of the game's own CX4 program from cartridge
//! ROM into the chip's two-page cache and runs it at 20 MHz until a `stop` opcode.
//!
//! fullsnes gives the port map, the opcode table and the data ROM, but marks most opcode
//! details, all timings and the cache/DMA behaviour as unknown. Where it is silent this follows
//! Mesen2 (`Core/SNES/Coprocessors/CX4/Cx4.cpp`, `Cx4.Instructions.cpp`), the project's
//! implementation reference for the SNES, behaviour for behaviour.

mod data_rom;
mod instructions;

use crate::snes::ppu::SnesVideoRegion;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// The CX4's clock (fullsnes component lists: "X1 2pin 20MHz").
const CX4_CLOCK_HZ: u64 = 20_000_000;

/// The SNES master clock rate per region, as Mesen2 `SnesConsole::GetMasterClockRate` gives it
/// (the same denominators the APU uses, see `src/snes/apu/mod.rs`).
const fn master_clock_hz(region: SnesVideoRegion) -> u64 {
    match region {
        SnesVideoRegion::Ntsc => 21_477_270,
        SnesVideoRegion::Pal => 21_281_370,
    }
}

/// Bytes of CX4 data RAM (fullsnes: "6000h..6BFFh R/W CX4RAM (3Kbytes)").
pub(crate) const DATA_RAM_SIZE: usize = 0xC00;

/// Every piece of CX4 state, saved and restored as one value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Cx4State {
    /// The chip's 3 KB data RAM.
    pub data_ram: Vec<u8>,
    /// R0-R15, 24 bits each, shared with the SNES at `$7F80-$7FAF`.
    pub regs: [u32; 16],
    /// `$7F60-$7F7F`, readable and writable by the SNES.
    pub vectors: [u8; 0x20],

    /// `$7F40-$7F42` DMA source, `$7F43-$7F44` length, `$7F45-$7F47` destination.
    pub dma_source: u32,
    pub dma_length: u16,
    pub dma_dest: u32,
    pub dma_pos: u32,
    pub dma_enabled: bool,

    /// `$7F48` program cache page, `$7F49-$7F4B` program ROM base, `$7F4C` page locks,
    /// `$7F4D-$7F4E` program bank (page), `$7F4F` program counter.
    pub cache_page: u8,
    pub cache_base: u32,
    pub cache_lock: [bool; 2],
    pub program_bank: u16,
    pub program_counter: u8,
    /// A cache fill in progress (`cache_enabled`), whether it was a `$7F48` preload, how many
    /// words of it are loaded, and the ROM address each page holds (`None` for never loaded).
    pub cache_enabled: bool,
    pub cache_preload: bool,
    pub cache_pos: u16,
    pub cache_address: [Option<u32>; 2],
    /// The two 256-word program cache pages.
    pub program_ram: [Vec<u16>; 2],

    /// `$7F50` bits 0-2 and 4-6: extra cycles per external RAM / ROM access.
    pub ram_access_delay: u8,
    pub rom_access_delay: u8,
    /// `$7F51` bit 0: the chip does not raise the SNES IRQ when it stops.
    pub irq_disabled: bool,
    /// `$7F52` bit 0.
    pub single_rom: bool,
    /// The chip's own IRQ flag, readable at `$7F5E` bit 1.
    pub irq_flag: bool,
    /// The SNES IRQ line the chip drives. It rises on `stop` and only a `$7F51` write with bit 0
    /// set lowers it (Mesen2: `$7F5E` clears the flag "but keeps IRQ signal high").
    pub irq_line: bool,
    /// `$7F55-$7F5C` suspend: remaining cycles (0 = until `$7F5D`).
    pub suspend_enabled: bool,
    pub suspend_duration: u32,

    /// Execution state.
    pub stopped: bool,
    /// Set by an invalid DMA; only `$7F53` releases it.
    pub locked: bool,
    pub cycle_count: u64,
    /// The CX4 cycle the master clock has reached; execution catches up to it.
    pub target_cycle: u64,
    /// The fractional CX4 cycle carried between master clocks, in units of 1 / master rate.
    pub clock_budget: u64,
    pub pb: u16,
    pub pc: u8,
    pub a: u32,
    pub p: u16,
    pub sp: u8,
    pub stack: [u32; 8],
    pub mult: u64,
    pub rom_buffer: u32,
    pub ram_buffer: [u8; 3],
    pub memory_data_reg: u32,
    pub memory_address_reg: u32,
    pub data_pointer_reg: u32,
    pub negative: bool,
    pub zero: bool,
    pub carry: bool,
    pub overflow: bool,

    /// A pending external bus access started by source/destination `$2E`/`$2F`.
    pub bus_enabled: bool,
    pub bus_reading: bool,
    pub bus_writing: bool,
    pub bus_delay_cycles: u8,
    pub bus_address: u32,
}

impl Default for Cx4State {
    /// Power-on: stopped, single ROM, three extra cycles per external access (Mesen2 `Reset`).
    fn default() -> Self {
        Self {
            data_ram: vec![0; DATA_RAM_SIZE],
            regs: [0; 16],
            vectors: [0; 0x20],
            dma_source: 0,
            dma_length: 0,
            dma_dest: 0,
            dma_pos: 0,
            dma_enabled: false,
            cache_page: 0,
            cache_base: 0,
            cache_lock: [false; 2],
            program_bank: 0,
            program_counter: 0,
            cache_enabled: false,
            cache_preload: false,
            cache_pos: 0,
            cache_address: [None; 2],
            program_ram: [vec![0; 256], vec![0; 256]],
            ram_access_delay: 3,
            rom_access_delay: 3,
            irq_disabled: false,
            single_rom: true,
            irq_flag: false,
            irq_line: false,
            suspend_enabled: false,
            suspend_duration: 0,
            stopped: true,
            locked: false,
            cycle_count: 0,
            target_cycle: 0,
            clock_budget: 0,
            pb: 0,
            pc: 0,
            a: 0,
            p: 0,
            sp: 0,
            stack: [0; 8],
            mult: 0,
            rom_buffer: 0,
            ram_buffer: [0; 3],
            memory_data_reg: 0,
            memory_address_reg: 0,
            data_pointer_reg: 0,
            negative: false,
            zero: false,
            carry: false,
            overflow: false,
            bus_enabled: false,
            bus_reading: false,
            bus_writing: false,
            bus_delay_cycles: 0,
            bus_address: 0,
        }
    }
}

/// The CX4 as the SNES bus owns it.
pub(crate) struct Cx4 {
    state: Cx4State,
    /// Cartridge ROM, read by program-cache fills, DMA and the external bus.
    rom: Rc<Vec<u8>>,
    master_clock_hz: u64,
}

/// What the CX4's own bus finds at an address (Mesen2's `_mappings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusTarget {
    /// LoROM cartridge ROM, `$00-$7F/$80-$FF:$8000-$FFFF`, at this ROM offset.
    Rom(usize),
    /// The chip's own RAM and ports, `$00-$3F/$80-$BF:$6000-$7FFF`, at this offset.
    Cx4(u16),
}

impl Cx4 {
    pub(crate) fn new(rom: Rc<Vec<u8>>, region: SnesVideoRegion) -> Self {
        Self {
            state: Cx4State::default(),
            rom,
            master_clock_hz: master_clock_hz(region),
        }
    }

    /// Advances the chip by one SNES master clock: it runs until its own cycle count has
    /// caught up with 20 MHz of elapsed master-clock time (Mesen2 `Cx4::Run`).
    pub(crate) fn tick_master_clock(&mut self) {
        // 20 MHz is below the master clock, so each master clock adds at most one CX4 cycle.
        self.state.clock_budget += CX4_CLOCK_HZ;
        if self.state.clock_budget >= self.master_clock_hz {
            self.state.clock_budget -= self.master_clock_hz;
            self.state.target_cycle += 1;
        }
        self.run();
    }

    fn run(&mut self) {
        while self.state.cycle_count < self.state.target_cycle {
            if self.state.locked {
                self.step(1);
            } else if self.state.suspend_enabled {
                self.step(1);
                if self.state.suspend_duration != 0 {
                    self.state.suspend_duration -= 1;
                    if self.state.suspend_duration == 0 {
                        self.state.suspend_enabled = false;
                    }
                }
            } else if self.state.cache_enabled {
                self.process_cache();
            } else if self.state.dma_enabled {
                self.process_dma();
            } else if self.state.stopped {
                self.step(self.state.target_cycle - self.state.cycle_count);
            } else if !self.process_cache() {
                if !self.state.cache_enabled {
                    // A cache fill is needed but both pages are locked.
                    self.stop();
                }
            } else {
                let page = usize::from(self.state.cache_page);
                let opcode = self.state.program_ram[page][usize::from(self.state.pc)];
                self.state.pc = self.state.pc.wrapping_add(1);
                if self.state.pc == 0 {
                    // Reaching the end of the page loads the next one; done before the opcode
                    // runs so a jump to address 0 does not trigger it (Mesen2).
                    self.switch_cache_page();
                }
                self.exec(opcode);
            }
        }
    }

    /// Spends `cycles` CX4 cycles, completing a pending external bus access when its delay
    /// runs out.
    fn step(&mut self, cycles: u64) {
        if self.state.bus_enabled {
            if u64::from(self.state.bus_delay_cycles) > cycles {
                self.state.bus_delay_cycles -= cycles as u8;
            } else {
                self.state.bus_enabled = false;
                self.state.bus_delay_cycles = 0;
                let address = self.state.bus_address;
                if self.state.bus_reading {
                    self.state.bus_reading = false;
                    self.state.memory_data_reg = u32::from(self.bus_read(address));
                }
                if self.state.bus_writing {
                    self.state.bus_writing = false;
                    self.bus_write(address, self.state.memory_data_reg as u8);
                }
            }
        }
        self.state.cycle_count += cycles;
    }

    /// Stops the program, raising the SNES IRQ unless `$7F51` disabled it.
    fn stop(&mut self) {
        self.state.stopped = true;
        if !self.state.irq_disabled {
            self.state.irq_flag = true;
            self.state.irq_line = true;
        }
    }

    /// Execution ran off the end of a cache page: continue in page 1 with the program bank in
    /// the page register, or stop when already there or page 1 is locked (Mesen2).
    fn switch_cache_page(&mut self) {
        if self.state.cache_page == 1 {
            self.stop();
            return;
        }
        self.state.cache_page = 1;
        if self.state.cache_lock[1] {
            self.stop();
            return;
        }
        self.state.pb = self.state.p;
        if !self.process_cache() && !self.state.cache_enabled {
            self.stop();
        }
    }

    /// Makes the program bank `pb` available in a cache page, filling one from ROM when
    /// neither holds it. Returns whether the page is ready; a fill runs until the target cycle
    /// and continues on the next call.
    fn process_cache(&mut self) -> bool {
        let address = (self.state.cache_base + (u32::from(self.state.pb) << 9)) & 0xFF_FFFF;
        if self.state.cache_pos == 0 {
            if !self.state.cache_preload {
                let page = usize::from(self.state.cache_page);
                if self.state.cache_address[page] == Some(address) {
                    self.state.cache_enabled = false;
                    return true;
                }
                self.state.cache_page ^= 1;
                let page = usize::from(self.state.cache_page);
                if self.state.cache_address[page] == Some(address) {
                    self.state.cache_enabled = false;
                    return true;
                }
                if self.state.cache_lock[page] {
                    self.state.cache_page ^= 1;
                }
                if self.state.cache_lock[usize::from(self.state.cache_page)] {
                    self.state.cache_enabled = false;
                    return false;
                }
            }
            self.state.cache_enabled = true;
        }

        while self.state.cache_pos < 256 {
            let at = address + u32::from(self.state.cache_pos) * 2;
            let lsb = self.bus_read(at);
            self.step(self.access_delay(at));
            let msb = self.bus_read(at + 1);
            self.step(self.access_delay(at + 1));
            let page = usize::from(self.state.cache_page);
            let pos = usize::from(self.state.cache_pos);
            self.state.program_ram[page][pos] = u16::from_le_bytes([lsb, msb]);
            self.state.cache_pos += 1;
            if self.state.cycle_count >= self.state.target_cycle {
                break;
            }
        }

        if self.state.cache_pos < 256 {
            return false;
        }
        let page = usize::from(self.state.cache_page);
        self.state.cache_address[page] = Some(address);
        self.state.cache_pos = 0;
        self.state.cache_enabled = false;
        self.state.cache_preload = false;
        true
    }

    /// Copies `dma_length` bytes from `dma_source` to `dma_dest` over the chip's bus, until the
    /// target cycle. A copy to ROM, to unmapped space or within one kind of memory locks the
    /// chip until `$7F53` (Mesen2).
    fn process_dma(&mut self) {
        while self.state.dma_pos < u32::from(self.state.dma_length) {
            let src = (self.state.dma_source + self.state.dma_pos) & 0xFF_FFFF;
            let dest = (self.state.dma_dest + self.state.dma_pos) & 0xFF_FFFF;
            let valid = matches!(
                (Self::bus_target(src), Self::bus_target(dest)),
                (Some(BusTarget::Rom(_)), Some(BusTarget::Cx4(_)))
            );
            if !valid {
                self.state.locked = true;
                self.state.dma_pos = 0;
                self.state.dma_enabled = false;
                return;
            }
            self.step(self.access_delay(src));
            let value = self.bus_read(src);
            self.step(self.access_delay(dest));
            self.bus_write(dest, value);
            self.state.dma_pos += 1;
            if self.state.cycle_count >= self.state.target_cycle {
                break;
            }
        }
        if self.state.dma_pos >= u32::from(self.state.dma_length) {
            self.state.dma_pos = 0;
            self.state.dma_enabled = false;
        }
    }

    /// Decodes an address on the chip's own bus. CX4 boards have no SRAM (fullsnes: "SRAM
    /// 70-77:0000-7FFF (not installed)"), so only ROM and the chip itself answer.
    fn bus_target(address: u32) -> Option<BusTarget> {
        let bank = (address >> 16) as u8;
        let offset = address as u16;
        if offset >= 0x8000 {
            let index = usize::from(bank & 0x7F) * 0x8000 + usize::from(offset - 0x8000);
            return Some(BusTarget::Rom(index));
        }
        if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset >= 0x6000 {
            return Some(BusTarget::Cx4(offset));
        }
        None
    }

    fn bus_read(&self, address: u32) -> u8 {
        match Self::bus_target(address) {
            Some(BusTarget::Rom(index)) => self.rom.get(index).copied().unwrap_or(0),
            Some(BusTarget::Cx4(offset)) => self.read(offset),
            None => 0,
        }
    }

    fn bus_write(&mut self, address: u32, value: u8) {
        if let Some(BusTarget::Cx4(offset)) = Self::bus_target(address) {
            self.write(offset, value);
        }
    }

    /// Cycles one external access costs: ROM pays the `$7F50` ROM wait states on top of one
    /// cycle; the chip's own RAM costs one (Mesen2 `GetAccessDelay`).
    fn access_delay(&self, address: u32) -> u64 {
        match Self::bus_target(address) {
            Some(BusTarget::Rom(_)) => 1 + u64::from(self.state.rom_access_delay),
            _ => 1,
        }
    }

    /// The SNES IRQ line the chip drives.
    pub(crate) fn irq_line(&self) -> bool {
        self.state.irq_line
    }

    /// The /RES line: every register and the execution state return to power-on, while data
    /// RAM keeps its contents (Mesen2 `Cx4::Reset` clears `_state` but not `_dataRam`).
    pub(crate) fn reset(&mut self) {
        let data_ram = std::mem::take(&mut self.state.data_ram);
        self.state = Cx4State {
            data_ram,
            ..Cx4State::default()
        };
    }

    /// The data RAM, for the configured power-on fill.
    pub(crate) fn data_ram_mut(&mut self) -> &mut [u8] {
        &mut self.state.data_ram
    }

    pub(crate) fn capture_state(&self) -> Cx4State {
        self.state.clone()
    }

    /// Restores a captured state. A state whose RAM or cache is the wrong size (a corrupt
    /// file) is rejected rather than allowed to panic later.
    pub(crate) fn restore_state(&mut self, state: &Cx4State) -> Result<(), String> {
        if state.data_ram.len() != DATA_RAM_SIZE
            || state.program_ram.iter().any(|page| page.len() != 256)
        {
            return Err("CX4 state size mismatch".to_string());
        }
        self.state = state.clone();
        Ok(())
    }

    /// Maps an offset in `$6000-$7FFF` onto the chip's 4 KB register window; `$6xxx` mirrors
    /// `$7xxx` (Mesen2 `Read`/`Write`: `addr = 0x7000 | (addr & 0xFFF)`).
    fn window(offset: u16) -> u16 {
        0x7000 | (offset & 0x0FFF)
    }

    /// Index of R0-R15 byte for `$7F80-$7FAF` and its mirror `$7FC0-$7FEF`.
    fn register_byte(addr: u16) -> Option<(usize, u32)> {
        if (0x7F80..=0x7FAF).contains(&addr) || (0x7FC0..=0x7FEF).contains(&addr) {
            let index = (addr & 0x3F) as usize;
            Some((index / 3, (index % 3) as u32 * 8))
        } else {
            None
        }
    }

    fn is_busy(&self) -> bool {
        self.state.cache_enabled || self.state.dma_enabled || self.state.bus_delay_cycles > 0
    }

    fn is_running(&self) -> bool {
        self.is_busy() || !self.state.stopped
    }

    /// A SNES read of `$6000-$7FFF` (bank `$00-$3F/$80-$BF`). Has no side effects.
    pub(crate) fn read(&self, offset: u16) -> u8 {
        let addr = Self::window(offset);
        let state = &self.state;
        if addr <= 0x7BFF {
            return state.data_ram[(addr & 0x0FFF) as usize];
        }
        if (0x7F60..=0x7F7F).contains(&addr) {
            return state.vectors[(addr & 0x1F) as usize];
        }
        if let Some((reg, shift)) = Self::register_byte(addr) {
            return (state.regs[reg] >> shift) as u8;
        }
        if (0x7F53..=0x7F5F).contains(&addr) {
            // Status (fullsnes: `$7F5E` bit 6 busy); Mesen2 answers the whole range with
            // suspend (bit 0), IRQ flag (bit 1), running (bit 6) and busy (bit 7).
            return u8::from(state.suspend_enabled)
                | u8::from(state.irq_flag) << 1
                | u8::from(self.is_running()) << 6
                | u8::from(self.is_busy()) << 7;
        }
        match addr {
            0x7F40 => state.dma_source as u8,
            0x7F41 => (state.dma_source >> 8) as u8,
            0x7F42 => (state.dma_source >> 16) as u8,
            0x7F43 => state.dma_length as u8,
            0x7F44 => (state.dma_length >> 8) as u8,
            0x7F45 => state.dma_dest as u8,
            0x7F46 => (state.dma_dest >> 8) as u8,
            0x7F47 => (state.dma_dest >> 16) as u8,
            0x7F48 => state.cache_page,
            0x7F49 => state.cache_base as u8,
            0x7F4A => (state.cache_base >> 8) as u8,
            0x7F4B => (state.cache_base >> 16) as u8,
            0x7F4C => u8::from(state.cache_lock[0]) | u8::from(state.cache_lock[1]) << 1,
            0x7F4D => state.program_bank as u8,
            0x7F4E => (state.program_bank >> 8) as u8,
            0x7F4F => state.program_counter,
            0x7F50 => state.ram_access_delay | state.rom_access_delay << 4,
            0x7F51 => u8::from(state.irq_disabled),
            0x7F52 => u8::from(state.single_rom),
            _ => 0,
        }
    }

    /// A SNES write to `$6000-$7FFF` (bank `$00-$3F/$80-$BF`).
    pub(crate) fn write(&mut self, offset: u16, value: u8) {
        let addr = Self::window(offset);
        let state = &mut self.state;
        if addr <= 0x7BFF {
            state.data_ram[(addr & 0x0FFF) as usize] = value;
            return;
        }
        if (0x7F60..=0x7F7F).contains(&addr) {
            state.vectors[(addr & 0x1F) as usize] = value;
            return;
        }
        if let Some((reg, shift)) = Self::register_byte(addr) {
            state.regs[reg] = (state.regs[reg] & !(0xFF << shift)) | u32::from(value) << shift;
            return;
        }
        if (0x7F55..=0x7F5C).contains(&addr) {
            state.suspend_enabled = true;
            state.suspend_duration = u32::from(addr - 0x7F55) * 32;
            return;
        }
        let set_byte =
            |word: u32, shift: u32| (word & !(0xFF << shift)) | u32::from(value) << shift;
        match addr {
            0x7F40 => state.dma_source = set_byte(state.dma_source, 0),
            0x7F41 => state.dma_source = set_byte(state.dma_source, 8),
            0x7F42 => state.dma_source = set_byte(state.dma_source, 16),
            0x7F43 => state.dma_length = (state.dma_length & 0xFF00) | u16::from(value),
            0x7F44 => state.dma_length = (state.dma_length & 0x00FF) | u16::from(value) << 8,
            0x7F45 => state.dma_dest = set_byte(state.dma_dest, 0),
            0x7F46 => state.dma_dest = set_byte(state.dma_dest, 8),
            0x7F47 => {
                // fullsnes: "DMA start"; the chip only starts it while stopped (Mesen2).
                state.dma_dest = set_byte(state.dma_dest, 16);
                if state.stopped {
                    state.dma_enabled = true;
                }
            }
            0x7F48 => {
                // Preload a cache page from the program bank while stopped (Mesen2).
                state.cache_page = value & 0x01;
                if state.stopped {
                    state.pb = state.program_bank;
                    state.cache_preload = true;
                    state.cache_enabled = true;
                }
            }
            0x7F49 => state.cache_base = set_byte(state.cache_base, 0),
            0x7F4A => state.cache_base = set_byte(state.cache_base, 8),
            0x7F4B => state.cache_base = set_byte(state.cache_base, 16),
            0x7F4C => state.cache_lock = [value & 0x01 != 0, value & 0x02 != 0],
            0x7F4D => state.program_bank = (state.program_bank & 0xFF00) | u16::from(value),
            0x7F4E => {
                state.program_bank = (state.program_bank & 0x00FF) | u16::from(value & 0x7F) << 8;
            }
            0x7F4F => {
                // fullsnes: "Program ROM Instruction Pointer (PC/2), starts execution".
                state.program_counter = value;
                if state.stopped {
                    state.stopped = false;
                    state.pb = state.program_bank;
                    state.pc = value;
                }
            }
            0x7F50 => {
                state.ram_access_delay = value & 0x07;
                state.rom_access_delay = (value >> 4) & 0x07;
            }
            0x7F51 => {
                state.irq_disabled = value & 0x01 != 0;
                if state.irq_disabled {
                    state.irq_flag = false;
                    state.irq_line = false;
                }
            }
            0x7F52 => state.single_rom = value & 0x01 != 0,
            0x7F53 => {
                state.locked = false;
                state.stopped = true;
            }
            0x7F5D => state.suspend_enabled = false,
            // Clears the chip's IRQ flag but leaves the SNES IRQ line high (Mesen2).
            0x7F5E => state.irq_flag = false,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx4() -> Cx4 {
        Cx4::new(Rc::new(Vec::new()), SnesVideoRegion::Ntsc)
    }

    /// Writes `value` and reads it back from `read_at`.
    fn round_trip(cx4: &mut Cx4, write_at: u16, read_at: u16, value: u8) -> u8 {
        cx4.write(write_at, value);
        cx4.read(read_at)
    }

    #[test]
    fn data_ram_is_3k_at_6000_and_mirrored_at_7000() {
        let mut cx4 = cx4();
        assert_eq!(round_trip(&mut cx4, 0x6000, 0x7000, 0x5A), 0x5A);
        assert_eq!(round_trip(&mut cx4, 0x7BFF, 0x6BFF, 0xA5), 0xA5);
        assert_eq!(cx4.state.data_ram[0x000], 0x5A);
        assert_eq!(cx4.state.data_ram[0xBFF], 0xA5);
    }

    #[test]
    fn registers_r0_to_r15_are_24_bit_little_endian_at_7f80() {
        let mut cx4 = cx4();
        cx4.write(0x7F83, 0x33);
        cx4.write(0x7F84, 0x22);
        cx4.write(0x7F85, 0x11);
        assert_eq!(cx4.state.regs[1], 0x11_2233);
        cx4.write(0x7FAF, 0x77);
        assert_eq!(cx4.state.regs[15], 0x77_0000);
        assert_eq!(cx4.read(0x7F84), 0x22);
    }

    /// Overload's `cx4test` "$7F80-$7FFF": R0-R15 mirror at `$7FC0-$7FEF`, and the gaps
    /// `$7FB0-$7FBF`/`$7FF0-$7FFF` read 0 after writing `$FF`.
    #[test]
    fn register_file_mirrors_at_7fc0_and_its_gaps_read_zero() {
        let mut cx4 = cx4();
        assert_eq!(round_trip(&mut cx4, 0x7F80, 0x7FC0, 0x81), 0x81);
        assert_eq!(round_trip(&mut cx4, 0x7FEF, 0x7FAF, 0x42), 0x42);
        assert_eq!(round_trip(&mut cx4, 0x7FB0, 0x7FB0, 0xFF), 0x00);
        assert_eq!(round_trip(&mut cx4, 0x7FFF, 0x7FFF, 0xFF), 0x00);
    }

    /// Overload's `cx4test` "$7F40-$7F4F" and "$7F50-$7F5F": the bits each port keeps.
    #[test]
    fn control_ports_keep_only_their_implemented_bits() {
        let mut cx4 = cx4();
        for (port, kept) in [
            (0x7F48, 0x01),
            (0x7F4C, 0x03),
            (0x7F4E, 0x7F),
            (0x7F50, 0x77),
            (0x7F51, 0x01),
            (0x7F52, 0x01),
        ] {
            assert_eq!(
                round_trip(&mut cx4, port, port, 0xFF),
                kept,
                "port {port:04X}"
            );
            assert_eq!(
                round_trip(&mut cx4, port, port, 0x00),
                0x00,
                "port {port:04X}"
            );
        }
    }

    #[test]
    fn dma_and_program_address_ports_read_back() {
        let mut cx4 = cx4();
        for (port, value) in [
            (0x7F40, 0x12),
            (0x7F41, 0x34),
            (0x7F42, 0x56),
            (0x7F43, 0x78),
            (0x7F44, 0x9A),
            (0x7F45, 0xBC),
            (0x7F46, 0xDE),
            (0x7F49, 0x00),
            (0x7F4A, 0x80),
            (0x7F4B, 0x02),
            (0x7F4D, 0x0E),
        ] {
            assert_eq!(
                round_trip(&mut cx4, port, port, value),
                value,
                "port {port:04X}"
            );
        }
    }

    /// Overload's `cx4test` "$7F60-$7F7F": every bit of the vector bytes is kept.
    #[test]
    fn vector_bytes_7f60_to_7f7f_are_read_write() {
        let mut cx4 = cx4();
        for port in 0x7F60..=0x7F7F {
            assert_eq!(
                round_trip(&mut cx4, port, port, port as u8 ^ 0xA5),
                port as u8 ^ 0xA5
            );
        }
    }

    #[test]
    fn ram_and_rom_access_delays_power_on_as_three_cycles() {
        assert_eq!(cx4().read(0x7F50), 0x33);
        assert_eq!(cx4().read(0x7F52), 0x01);
    }

    #[test]
    fn status_reads_idle_when_stopped() {
        assert_eq!(cx4().read(0x7F5E), 0x00);
    }

    #[test]
    fn unused_register_space_reads_zero() {
        let mut cx4 = cx4();
        assert_eq!(round_trip(&mut cx4, 0x7C00, 0x7C00, 0xFF), 0x00);
        assert_eq!(round_trip(&mut cx4, 0x7F3F, 0x7F3F, 0xFF), 0x00);
    }

    // --- execution: program cache, DMA, clock --------------------------------------------

    /// ROM offset of LoROM `$02:8000`, where Mega Man keeps its CX4 program (fullsnes
    /// "CX4 Functions": `BASE=028000`).
    const PROGRAM_BASE: usize = 0x1_0000;

    /// A 256 KB LoROM image with `pages[n]` as the CX4 program page `n` at `$02:8000`.
    fn rom_with_program(pages: &[&[u16]]) -> Rc<Vec<u8>> {
        let mut rom = vec![0u8; 0x4_0000];
        for (page, words) in pages.iter().enumerate() {
            for (i, word) in words.iter().enumerate() {
                let at = PROGRAM_BASE + page * 0x200 + i * 2;
                rom[at..at + 2].copy_from_slice(&word.to_le_bytes());
            }
        }
        for (i, byte) in rom[0x1_8000..0x1_8010].iter_mut().enumerate() {
            *byte = 0xA0 + i as u8; // DMA source data at $03:8000
        }
        Rc::new(rom)
    }

    fn running_cx4(pages: &[&[u16]]) -> Cx4 {
        let mut cx4 = Cx4::new(rom_with_program(pages), SnesVideoRegion::Ntsc);
        for (port, value) in [(0x7F49, 0x00), (0x7F4A, 0x80), (0x7F4B, 0x02)] {
            cx4.write(port, value);
        }
        cx4
    }

    /// Ticks master clocks until the chip is idle again, returning how many it took.
    fn run_until_idle(cx4: &mut Cx4) -> u32 {
        for clocks in 1..=200_000 {
            cx4.tick_master_clock();
            if cx4.read(0x7F5E) & 0x40 == 0 {
                return clocks;
            }
        }
        panic!("CX4 still running after 200000 master clocks");
    }

    #[test]
    fn writing_the_program_counter_runs_the_program_from_rom() {
        // mov A,2Ah / mov R0,A / stop
        let mut cx4 = running_cx4(&[&[0x642A, 0xE060, 0xFC00]]);
        cx4.write(0x7F4D, 0x00);
        cx4.write(0x7F4E, 0x00);
        cx4.write(0x7F4F, 0x00);
        assert_eq!(
            cx4.read(0x7F5E) & 0x40,
            0x40,
            "busy as soon as $7F4F is written"
        );
        let clocks = run_until_idle(&mut cx4);
        assert_eq!(cx4.read(0x7F80), 0x2A);
        // Filling one 256-word page costs 512 ROM reads of 1 + 3 cycles each.
        assert!(clocks > 2048, "cache fill took {clocks} master clocks");
        assert!(cx4.irq_line());
    }

    #[test]
    fn a_far_jump_loads_the_program_page_into_the_second_cache_page() {
        // page 0: mov page.lsb,01h / jmp far 00h;  page 1: mov A,09h / mov R1,A / stop
        let mut cx4 = running_cx4(&[&[0x7C01, 0x0A00], &[0x6409, 0xE061, 0xFC00]]);
        cx4.write(0x7F4F, 0x00);
        run_until_idle(&mut cx4);
        assert_eq!(cx4.state.regs[1], 9);
        // A fill goes to the page that is not current, so the first program page landed in
        // cache page 1 and the far page in cache page 0 (Mesen2 `ProcessCache`).
        assert_eq!(cx4.state.cache_address, [Some(0x02_8200), Some(0x02_8000)]);
    }

    /// The first fill lands in cache page 1, and running off the end of cache page 1 stops.
    #[test]
    fn running_off_the_end_of_cache_page_1_stops_the_chip() {
        let mut cx4 = running_cx4(&[&[0x0000]]); // 256 nops
        cx4.write(0x7F4F, 0x00);
        run_until_idle(&mut cx4);
        assert!(cx4.state.stopped);
        assert_eq!(cx4.state.cache_page, 1);
    }

    #[test]
    fn a_program_can_call_back_into_a_cached_page_without_reloading_it() {
        // page 0: mov page.lsb,01h / call far 00h / mov R2,A / stop;  page 1: mov A,07h / ret
        let mut cx4 = running_cx4(&[&[0x7C01, 0x2A00, 0xE062, 0xFC00], &[0x6407, 0x3C00]]);
        cx4.write(0x7F4F, 0x00);
        run_until_idle(&mut cx4);
        assert_eq!(cx4.state.regs[2], 7);
    }

    #[test]
    fn dma_copies_rom_into_data_ram() {
        let mut cx4 = running_cx4(&[]);
        for (port, value) in [
            (0x7F40, 0x00),
            (0x7F41, 0x80),
            (0x7F42, 0x03),
            (0x7F43, 0x10),
            (0x7F44, 0x00),
            (0x7F45, 0x00),
            (0x7F46, 0x61),
            (0x7F47, 0x00),
        ] {
            cx4.write(port, value);
        }
        assert_eq!(cx4.read(0x7F5E) & 0xC0, 0xC0, "busy while the DMA runs");
        run_until_idle(&mut cx4);
        let expected: Vec<u8> = (0..16).map(|i| 0xA0 + i).collect();
        assert_eq!(cx4.state.data_ram[0x100..0x110], expected[..]);
        assert!(!cx4.irq_line(), "DMA does not raise the IRQ");
    }

    #[test]
    fn dma_into_rom_locks_the_chip_until_7f53() {
        let mut cx4 = running_cx4(&[]);
        for (port, value) in [
            (0x7F40, 0x00),
            (0x7F41, 0x61),
            (0x7F42, 0x00),
            (0x7F43, 0x01),
            (0x7F45, 0x00),
            (0x7F46, 0x80),
            (0x7F47, 0x00),
        ] {
            cx4.write(port, value);
        }
        for _ in 0..100 {
            cx4.tick_master_clock();
        }
        assert!(cx4.state.locked);
        cx4.write(0x7F53, 0x00);
        assert!(!cx4.state.locked && cx4.state.stopped);
    }

    #[test]
    fn the_program_reads_snes_rom_through_ext_ptr() {
        // mov ext_ptr,R0 / movb ext_dta,[ext_ptr] / wait / mov R1,ext_dta / stop
        let mut cx4 = running_cx4(&[&[0x6260, 0x612E, 0x1C00, 0xE161, 0xFC00]]);
        cx4.state.regs[0] = 0x03_8005;
        cx4.write(0x7F4F, 0x00);
        run_until_idle(&mut cx4);
        assert_eq!(cx4.state.regs[1], 0xA5);
    }

    #[test]
    fn the_chip_runs_20_million_cycles_per_second_of_master_clock() {
        for (region, master_hz) in [
            (SnesVideoRegion::Ntsc, 21_477_270_u64),
            (SnesVideoRegion::Pal, 21_281_370),
        ] {
            let mut cx4 = Cx4::new(Rc::new(Vec::new()), region);
            let clocks = master_hz / 100;
            for _ in 0..clocks {
                cx4.tick_master_clock();
            }
            // A hundredth of a second: 200,000 CX4 cycles, less the fraction still owed.
            assert_eq!(
                cx4.state.cycle_count,
                clocks * 20_000_000 / master_hz,
                "{region:?}"
            );
            assert_eq!(cx4.state.cycle_count, 199_999, "{region:?}");
        }
    }

    #[test]
    fn a_suspend_write_pauses_execution_for_its_cycle_count() {
        let mut cx4 = running_cx4(&[&[0xFC00]]);
        cx4.write(0x7F56, 0x00); // 32 cycles
        assert_eq!(cx4.read(0x7F5E) & 0x01, 0x01);
        for _ in 0..40 {
            cx4.tick_master_clock();
        }
        assert_eq!(cx4.read(0x7F5E) & 0x01, 0x00);
    }
}
