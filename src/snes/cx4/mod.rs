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

use serde::{Deserialize, Serialize};

/// Bytes of CX4 data RAM (fullsnes: "6000h..6BFFh R/W CX4RAM (3Kbytes)").
pub(crate) const DATA_RAM_SIZE: usize = 0xC00;

/// Every piece of CX4 state, saved and restored as one value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cx4State {
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
    /// CX4 cycles owed to the master clock, in units of 1 / master-clock-rate.
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
}

impl Cx4 {
    pub(crate) fn new() -> Self {
        Self {
            state: Cx4State::default(),
        }
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
        Cx4::new()
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
}
