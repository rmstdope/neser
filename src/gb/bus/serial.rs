//! The Game Boy serial port (`$FF01` SB, `$FF02` SC), shared by the DMG and CGB buses.
//!
//! Only internal-clock transfers are emulated, against an absent peer: each
//! completed transfer shifts SB out (captured in [`Serial::output`]), shifts in
//! `$FF`, clears SC bit 7 and requests the serial interrupt (IF bit 3).
//!
//! The serial master clock toggles on every falling edge of one bit of the
//! 16-bit DIV counter, and a bit is shifted each time it falls to `false`
//! while SC is `$81`, so a byte takes 16 edges. Normal speed clocks off bit 7
//! (8192 Hz); the CGB fast clock (SC bit 1, CGB mode only) off bit 2
//! (262144 Hz). Double speed doubles both, since the counter counts CPU
//! clocks. (Pan Docs "Serial Data Transfer"; SameBoy `GB_serial_master_edge`.)

use crate::gb::console::save_state::BusState;

/// DIV-counter bit whose falling edge clocks the normal-speed serial master clock.
const NORMAL_CLOCK_BIT: u16 = 0x80;
/// DIV-counter bit whose falling edge clocks the CGB fast serial master clock.
const FAST_CLOCK_BIT: u16 = 0x04;

/// SB, SC and the internal-clock transfer in flight.
#[derive(Clone, Debug)]
pub(super) struct Serial {
    /// `$FF01` Serial Data (SB).
    pub sb: u8,
    /// `$FF02` Serial Control (SC), stored as written; [`Serial::read_sc`] masks it.
    pub sc: u8,
    /// Bytes shifted out by completed internal-clock transfers.
    output: Vec<u8>,
    /// Bits still to shift in the current internal-clock transfer.
    bits_remaining: u8,
    /// Free-running serial master clock, toggled on every falling edge of the
    /// selected DIV bit whether or not a transfer is active.
    master_clock: bool,
}

impl Serial {
    /// Power-on state: SB `$00`, SC `$7E` (SameBoy's power-on value on both models).
    pub fn new() -> Self {
        Self {
            sb: 0x00,
            sc: 0x7E,
            output: Vec::new(),
            bits_remaining: 0,
            master_clock: false,
        }
    }

    /// Read SC. Bits 6–2 always read as 1; bit 1 (fast clock) is readable
    /// only in CGB mode and reads as 1 on DMG and in DMG compatibility mode.
    pub fn read_sc(&self, cgb_mode: bool) -> u8 {
        self.sc | if cgb_mode { 0x7C } else { 0x7E }
    }

    /// Write SC. Setting bits 7 and 0 starts (or restarts) an 8-bit
    /// internal-clock transfer; an external-clock start is stored but never
    /// completes, since no peer drives the clock.
    pub fn write_sc(&mut self, val: u8) {
        let internal_start = val & 0x81 == 0x81;
        // Clock alignment: starting an internal-clock transfer while the master
        // clock is high forces it low first, so the first bit is always timed at
        // the correct phase. Restricted to internal starts so that writes that
        // merely inspect or clear SC never shift the clock phase.
        if internal_start {
            self.master_clock = false;
            self.bits_remaining = 8;
        }
        self.sc = val;
    }

    /// Advance the serial port across one M-cycle of DIV: `pre` and `post` are
    /// the DIV counter before and after it. Returns `true` when a transfer
    /// completed on this M-cycle, for the caller to raise IF bit 3.
    pub fn clock(&mut self, pre: u16, post: u16, cgb_mode: bool) -> bool {
        let bit = if cgb_mode && self.sc & 0x02 != 0 {
            FAST_CLOCK_BIT
        } else {
            NORMAL_CLOCK_BIT
        };
        if pre & bit == 0 || post & bit != 0 {
            return false;
        }
        self.master_clock ^= true;
        if self.master_clock || self.bits_remaining == 0 || self.sc & 0x81 != 0x81 {
            return false;
        }
        self.bits_remaining -= 1;
        if self.bits_remaining > 0 {
            return false;
        }
        self.output.push(self.sb);
        self.sb = 0xFF;
        self.sc &= 0x7F;
        true
    }

    /// Bytes shifted out by completed transfers, oldest first.
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// Write the serial fields of a save state.
    pub fn capture_into(&self, state: &mut BusState) {
        state.sb = Some(self.sb);
        state.sc = Some(self.sc);
        state.serial_buf = Some(self.output.clone());
        state.serial_bits_remaining = Some(self.bits_remaining);
        state.serial_master_clock = Some(self.master_clock);
    }

    /// Restore from a save state; a field the state lacks keeps its current value.
    pub fn restore_from(&mut self, state: &BusState) {
        if let Some(sb) = state.sb {
            self.sb = sb;
        }
        if let Some(sc) = state.sc {
            self.sc = sc;
        }
        if let Some(ref buf) = state.serial_buf {
            self.output = buf.clone();
        }
        if let Some(bits) = state.serial_bits_remaining {
            self.bits_remaining = bits;
        }
        if let Some(clock) = state.serial_master_clock {
            self.master_clock = clock;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive `serial` with a DIV counter that advances 4 per M-cycle from
    /// `start`, for `m_cycles` M-cycles; returns the M-cycle (1-based) on which
    /// a transfer completed, if any.
    fn run(serial: &mut Serial, start: u16, m_cycles: u32, cgb_mode: bool) -> Option<u32> {
        let mut counter = start;
        let mut done = None;
        for m in 1..=m_cycles {
            let next = counter.wrapping_add(4);
            if serial.clock(counter, next, cgb_mode) && done.is_none() {
                done = Some(m);
            }
            counter = next;
        }
        done
    }

    fn started(sb: u8, sc: u8) -> Serial {
        let mut serial = Serial::new();
        serial.sb = sb;
        serial.write_sc(sc);
        serial
    }

    #[test]
    fn internal_clock_transfer_completes_after_sixteen_bit7_falls() {
        // Counter 0: bit 7 falls every 256 counts = 64 M-cycles, first at M 64.
        // Master clock starts low, so bits shift on even falls: the 16th fall ends it.
        let mut serial = started(0x41, 0x81);
        assert_eq!(run(&mut serial, 0, 16 * 64, false), Some(16 * 64));
    }

    #[test]
    fn completion_sets_sb_to_ff_clears_sc_bit7_and_reports_interrupt() {
        let mut serial = started(0x41, 0x81);
        assert!(run(&mut serial, 0, 1024, false).is_some());
        assert_eq!(serial.output(), &[0x41]);
        assert_eq!(serial.sb, 0xFF);
        assert_eq!(serial.sc & 0x80, 0);
    }

    #[test]
    fn external_clock_never_completes() {
        let mut serial = started(0x55, 0x80);
        assert_eq!(run(&mut serial, 0, 4096, true), None);
        assert_eq!(serial.sc & 0x80, 0x80);
        assert!(serial.output().is_empty());
    }

    #[test]
    fn fast_bit_clocks_off_div_bit2_in_cgb_mode() {
        // Bit 2 falls every 8 counts = 2 M-cycles: 16 falls = 32 M-cycles.
        let mut serial = started(0x99, 0x83);
        assert_eq!(run(&mut serial, 0, 64, true), Some(32));
        assert_eq!(serial.output(), &[0x99]);
    }

    #[test]
    fn fast_bit_is_ignored_outside_cgb_mode() {
        let mut serial = started(0x99, 0x83);
        assert_eq!(run(&mut serial, 0, 1024, false), Some(1024));
    }

    #[test]
    fn sc_reads_bit1_only_in_cgb_mode() {
        let mut serial = Serial::new();
        assert_eq!(serial.read_sc(true), 0x7E, "power-on SC");
        serial.write_sc(0x81);
        assert_eq!(serial.read_sc(true), 0xFD);
        assert_eq!(serial.read_sc(false), 0xFF);
        serial.write_sc(0x02);
        assert_eq!(serial.read_sc(true), 0x7E);
        serial.write_sc(0x00);
        assert_eq!(serial.read_sc(true), 0x7C);
        assert_eq!(serial.read_sc(false), 0x7E);
    }

    #[test]
    fn restart_rearms_eight_bits() {
        let mut serial = started(0x12, 0x81);
        run(&mut serial, 0, 512, false);
        serial.write_sc(0x81);
        assert_eq!(run(&mut serial, 512 * 4, 1023, false), None);
    }
}
