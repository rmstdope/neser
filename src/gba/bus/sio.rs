//! GBA Serial I/O (SIO) controller.
//!
//! Implements the SIO transfer state machine for Normal 8-bit and Normal
//! 32-bit modes. Multiplayer mode stays busy indefinitely (no slaves).
//!
//! Transfer is started by writing SIOCNT with bit 7 (Start/Busy) set.
//! After the appropriate number of CPU cycles the transfer completes:
//! bit 7 is cleared, received data is filled with 0xFF (SI floats high),
//! and IRQ_SIO is optionally raised.
//!
//! Cycle counts (Normal mode, master, no slave):
//!
//! | Width  | Clock     | Cycles |
//! |--------|-----------|--------|
//! | 8-bit  | 256 KHz   |    512 |
//! | 8-bit  | 2 MHz     |     64 |
//! | 32-bit | 256 KHz   |  2048  |
//! | 32-bit | 2 MHz     |    256 |
//!
//! <https://problemkaputt.de/gbatek.htm#gaborealcommunication>

use super::interrupt::{InterruptController, bits};
use serde::{Deserialize, Serialize};

/// GBA CPU frequency in Hz (16.78 MHz).
const CPU_FREQ: u32 = 16_777_216;

/// SIO controller state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sio {
    /// SIOCNT register value (with live bit 7 state).
    siocnt: u16,
    /// RCNT register value.
    rcnt: u16,
    /// Whether a transfer is currently in progress.
    transfer_active: bool,
    /// Remaining CPU cycles until transfer completion.
    remaining_cycles: u32,
}

impl Default for Sio {
    fn default() -> Self {
        Self::new()
    }
}

impl Sio {
    pub fn new() -> Self {
        Self {
            siocnt: 0,
            rcnt: 0,
            transfer_active: false,
            remaining_cycles: 0,
        }
    }

    /// Read SIOCNT (0x0400_0128). Bit 7 reflects live transfer state.
    /// Applies the mode-dependent read mask (UART: 0x7FAF, others: 0x7F8F).
    pub fn read_siocnt(&self) -> u16 {
        let mask = if self.mode() == SioMode::Uart {
            0x7FAF
        } else {
            0x7F8F
        };
        let val = if self.transfer_active {
            self.siocnt | 0x0080
        } else {
            self.siocnt & !0x0080
        };
        val & mask
    }

    /// Write SIOCNT (0x0400_0128). A rising edge on bit 7 starts a transfer.
    pub fn write_siocnt(&mut self, value: u16) {
        let was_busy = self.transfer_active;
        // Store the register value (without bit 7 — that's managed by state)
        self.siocnt = value & !0x0080;

        // Rising edge on bit 7 starts a transfer
        if !was_busy && value & 0x0080 != 0 {
            self.transfer_active = true;
            match self.mode() {
                SioMode::Normal8 | SioMode::Normal32 => {
                    self.remaining_cycles = self.normal_transfer_cycles();
                }
                SioMode::Multi => {
                    // No slaves connected — transfer never completes
                    self.remaining_cycles = u32::MAX;
                }
                _ => {
                    // UART / GPIO: not implemented, don't start
                    self.transfer_active = false;
                }
            }
        }
    }

    /// Advance the SIO state by `cycles` CPU cycles.
    /// Completes any active Normal-mode transfer and optionally raises IRQ_SIO.
    pub fn step(&mut self, cycles: u32, ic: &mut InterruptController) {
        if !self.transfer_active {
            return;
        }
        // Multi mode never completes (no peers)
        if self.mode() == SioMode::Multi {
            return;
        }
        self.remaining_cycles = self.remaining_cycles.saturating_sub(cycles);
        if self.remaining_cycles == 0 {
            self.transfer_active = false;
            if self.siocnt & 0x4000 != 0 {
                ic.raise(bits::SERIAL);
            }
        }
    }

    /// Write RCNT (0x0400_0134). Updates the mode used for transfer logic.
    pub fn write_rcnt(&mut self, value: u16) {
        self.rcnt = value;
    }

    /// Returns the SIO mode from RCNT bit 15 and SIOCNT bits 12-13.
    fn mode(&self) -> SioMode {
        if self.rcnt & 0x8000 != 0 {
            return SioMode::Gpio;
        }
        match (self.siocnt >> 12) & 3 {
            0 => SioMode::Normal8,
            1 => SioMode::Normal32,
            2 => SioMode::Multi,
            3 => SioMode::Uart,
            _ => unreachable!(),
        }
    }

    /// Compute the transfer duration in CPU cycles for Normal mode.
    fn normal_transfer_cycles(&self) -> u32 {
        let bits = match self.mode() {
            SioMode::Normal8 => 8,
            SioMode::Normal32 => 32,
            _ => return 0,
        };
        let clock_khz: u32 = if self.siocnt & 0x0002 != 0 {
            2048 // 2 MHz
        } else {
            256 // 256 KHz
        };
        bits * CPU_FREQ / (clock_khz * 1024)
    }
}

/// SIO operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SioMode {
    Normal8,
    Normal32,
    Multi,
    Uart,
    Gpio,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sio() -> Sio {
        Sio::new()
    }

    fn make_ic() -> InterruptController {
        InterruptController::new()
    }

    // --- Cycle calculation tests ---

    #[test]
    fn normal8_256khz_takes_512_cycles() {
        let mut sio = make_sio();
        // Normal 8-bit: SIOCNT bits 12-13 = 00, bit 1 = 0 (256 KHz)
        sio.siocnt = 0x0000;
        assert_eq!(sio.normal_transfer_cycles(), 512);
    }

    #[test]
    fn normal8_2mhz_takes_64_cycles() {
        let mut sio = make_sio();
        // Normal 8-bit: SIOCNT bits 12-13 = 00, bit 1 = 1 (2 MHz)
        sio.siocnt = 0x0002;
        assert_eq!(sio.normal_transfer_cycles(), 64);
    }

    #[test]
    fn normal32_256khz_takes_2048_cycles() {
        let mut sio = make_sio();
        // Normal 32-bit: SIOCNT bits 12-13 = 01, bit 1 = 0 (256 KHz)
        sio.siocnt = 0x1000;
        assert_eq!(sio.normal_transfer_cycles(), 2048);
    }

    #[test]
    fn normal32_2mhz_takes_256_cycles() {
        let mut sio = make_sio();
        // Normal 32-bit: SIOCNT bits 12-13 = 01, bit 1 = 1 (2 MHz)
        sio.siocnt = 0x1002;
        assert_eq!(sio.normal_transfer_cycles(), 256);
    }

    // --- Transfer start tests ---

    #[test]
    fn write_siocnt_bit7_starts_normal_transfer() {
        let mut sio = make_sio();
        // Normal 8-bit mode, 256 KHz, start bit
        sio.write_siocnt(0x0080);
        assert!(sio.transfer_active);
        assert_eq!(sio.remaining_cycles, 512);
    }

    #[test]
    fn write_siocnt_without_bit7_does_not_start_transfer() {
        let mut sio = make_sio();
        sio.write_siocnt(0x0000);
        assert!(!sio.transfer_active);
    }

    #[test]
    fn write_siocnt_multi_mode_stays_busy_indefinitely() {
        let mut sio = make_sio();
        // Multi mode: SIOCNT bits 12-13 = 10, bit 7 = start
        sio.write_siocnt(0x2080);
        assert!(sio.transfer_active);
        // Step many cycles — should NOT complete
        let mut ic = make_ic();
        sio.step(100_000, &mut ic);
        assert!(sio.transfer_active);
        // Bit 7 should still be set
        assert_ne!(sio.read_siocnt() & 0x0080, 0);
    }

    // --- Transfer completion tests ---

    #[test]
    fn normal_transfer_completes_after_exact_cycles() {
        let mut sio = make_sio();
        let mut ic = make_ic();
        // Normal 8-bit, 2 MHz => 64 cycles
        sio.write_siocnt(0x0082); // bit 7 + bit 1 (2 MHz)
        assert!(sio.transfer_active);

        // Step 63 cycles — should still be active
        sio.step(63, &mut ic);
        assert!(sio.transfer_active);
        assert_ne!(sio.read_siocnt() & 0x0080, 0);

        // Step 1 more — should complete
        sio.step(1, &mut ic);
        assert!(!sio.transfer_active);
        assert_eq!(sio.read_siocnt() & 0x0080, 0);
    }

    #[test]
    fn normal_transfer_completes_with_overshoot() {
        let mut sio = make_sio();
        let mut ic = make_ic();
        // Normal 32-bit, 256 KHz => 2048 cycles
        sio.write_siocnt(0x1080);
        sio.step(5000, &mut ic);
        assert!(!sio.transfer_active);
        assert_eq!(sio.read_siocnt() & 0x0080, 0);
    }

    // --- IRQ tests ---

    #[test]
    fn completion_raises_irq_when_bit14_set() {
        let mut sio = make_sio();
        let mut ic = make_ic();
        ic.ie = bits::SERIAL;
        ic.ime = true;
        // Normal 8-bit, 2 MHz, IRQ enable (bit 14), start (bit 7)
        sio.write_siocnt(0x4082);
        sio.step(64, &mut ic);
        assert_ne!(ic.if_flags & bits::SERIAL, 0, "IRQ_SIO should be raised");
    }

    #[test]
    fn completion_does_not_raise_irq_when_bit14_clear() {
        let mut sio = make_sio();
        let mut ic = make_ic();
        ic.ie = bits::SERIAL;
        ic.ime = true;
        // Normal 8-bit, 2 MHz, NO IRQ enable, start (bit 7)
        sio.write_siocnt(0x0082);
        sio.step(64, &mut ic);
        assert_eq!(
            ic.if_flags & bits::SERIAL,
            0,
            "IRQ_SIO should not be raised"
        );
    }

    // --- SIOCNT read tests ---

    #[test]
    fn read_siocnt_reflects_bit7_during_transfer() {
        let mut sio = make_sio();
        sio.write_siocnt(0x0080); // start Normal 8-bit transfer
        assert_ne!(sio.read_siocnt() & 0x0080, 0);
    }

    #[test]
    fn read_siocnt_reflects_bit7_cleared_after_completion() {
        let mut sio = make_sio();
        let mut ic = make_ic();
        sio.write_siocnt(0x0082); // Normal 8-bit, 2 MHz
        sio.step(64, &mut ic);
        assert_eq!(sio.read_siocnt() & 0x0080, 0);
    }

    #[test]
    fn read_siocnt_preserves_other_bits() {
        let mut sio = make_sio();
        // Write with various config bits set (IRQ enable, clock)
        sio.write_siocnt(0x4002);
        let val = sio.read_siocnt();
        assert_ne!(val & 0x4000, 0, "bit 14 (IRQ enable) should be preserved");
        assert_ne!(val & 0x0002, 0, "bit 1 (clock) should be preserved");
    }
}
