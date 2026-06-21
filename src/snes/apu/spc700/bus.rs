//! SPC700 bus trait and a flat-RAM implementation for unit/vector testing.
//!
//! The SPC700 has a 16-bit address space backed by 64 KB ARAM, with I/O ports
//! at `$00F0–$00FF` and a selectable 64-byte boot ROM at `$FFC0–$FFFF`. Those
//! special regions are added in later sub-issues; this trait keeps the CPU core
//! bus-agnostic and testable, mirroring the `SnesBus` pattern used by the 65816.
//!
//! Every [`Spc700Bus::read`], [`Spc700Bus::write`], and [`Spc700Bus::idle`] call
//! advances SPC700 time by the corresponding bus-defined cycle cost from
//! [`Spc700Bus::read_cycles`], [`Spc700Bus::write_cycles`], or
//! [`Spc700Bus::idle_cycles`].

/// Bus interface for the SPC700 CPU core.
///
/// Reads take `&mut self` because several SPC700-visible registers (I/O ports,
/// timer counters) have read side effects in the full APU implementation.
pub trait Spc700Bus {
    /// Base cycle cost for reading from the given address.
    fn read_cycles(&self, _addr: u16) -> u8 {
        1
    }

    /// Read a byte from the 16-bit SPC700 address space.
    ///
    /// Timing is represented by [`Self::read_cycles`].
    fn read(&mut self, addr: u16) -> u8;

    /// Base cycle cost for writing to the given address.
    fn write_cycles(&self, _addr: u16) -> u8 {
        1
    }

    /// Write a byte to the 16-bit SPC700 address space.
    ///
    /// Timing is represented by [`Self::write_cycles`].
    fn write(&mut self, addr: u16, value: u8);

    /// Base cycle cost for an idle SPC700 cycle.
    fn idle_cycles(&self) -> u8 {
        1
    }

    /// Consume one internal SPC700 idle step with no memory access.
    ///
    /// Timing is represented by [`Self::idle_cycles`].
    fn idle(&mut self);
}

/// Flat 64 KB RAM bus used for unit tests and SingleStepTests vectors.
///
/// Records the number of cycles (reads + writes + idles) so timing can be
/// asserted independently of the memory contents.
pub struct FlatRamBus {
    ram: Box<[u8; 0x1_0000]>,
    cycles: u64,
}

impl FlatRamBus {
    /// Create a new flat-RAM bus with all memory zeroed.
    pub fn new() -> Self {
        Self {
            ram: Box::new([0u8; 0x1_0000]),
            cycles: 0,
        }
    }

    /// Number of cycles (reads + writes + idles) executed so far.
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Directly set a memory byte without consuming a cycle (test setup).
    pub fn set(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }

    /// Directly read a memory byte without consuming a cycle (test assertions).
    pub fn get(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    /// Load a contiguous slice starting at `addr`, wrapping within 64 KB.
    pub fn load(&mut self, addr: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.ram[addr.wrapping_add(i as u16) as usize] = byte;
        }
    }
}

impl Default for FlatRamBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Spc700Bus for FlatRamBus {
    fn read_cycles(&self, _addr: u16) -> u8 {
        1
    }

    fn read(&mut self, addr: u16) -> u8 {
        self.cycles += 1;
        self.ram[addr as usize]
    }

    fn write_cycles(&self, _addr: u16) -> u8 {
        1
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.cycles += 1;
        self.ram[addr as usize] = value;
    }

    fn idle_cycles(&self) -> u8 {
        1
    }

    fn idle(&mut self) {
        self.cycles += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_ram_new_is_zeroed() {
        let bus = FlatRamBus::new();
        assert_eq!(bus.get(0x0000), 0x00);
        assert_eq!(bus.get(0xFFFF), 0x00);
        assert_eq!(bus.cycles(), 0);
    }

    #[test]
    fn flat_ram_read_write_roundtrip_and_counts_cycles() {
        let mut bus = FlatRamBus::new();
        bus.write(0x1234, 0xAB);
        assert_eq!(bus.read(0x1234), 0xAB);
        assert_eq!(bus.cycles(), 2);
    }

    #[test]
    fn flat_ram_idle_counts_a_cycle() {
        let mut bus = FlatRamBus::new();
        bus.idle();
        assert_eq!(bus.cycles(), 1);
    }

    #[test]
    fn flat_ram_set_get_do_not_consume_cycles() {
        let mut bus = FlatRamBus::new();
        bus.set(0x0200, 0x42);
        assert_eq!(bus.get(0x0200), 0x42);
        assert_eq!(bus.cycles(), 0);
    }

    #[test]
    fn flat_ram_load_wraps_within_64k() {
        let mut bus = FlatRamBus::new();
        bus.load(0xFFFF, &[0x11, 0x22]);
        assert_eq!(bus.get(0xFFFF), 0x11);
        assert_eq!(bus.get(0x0000), 0x22);
    }
}
