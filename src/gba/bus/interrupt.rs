//! GBA interrupt controller — `IE`, `IF`, `IME`.
//!
//! Modeled per GBATek "Interrupts" section. The controller raises the IRQ
//! line whenever any bit is set in both [`InterruptController::ie`] and
//! [`InterruptController::if_flags`] *and* the master enable [`IME`][1] bit
//! is set. Software acknowledges an interrupt by writing a 1 to the
//! corresponding bit of `IF` (write-1-to-clear semantics).
//!
//! [1]: <https://problemkaputt.de/gbatek.htm#gbainterruptregisterimeimeie>

use serde::{Deserialize, Serialize};

/// `IF` / `IE` bit positions used elsewhere in the bus (timers/PPU/etc.).
pub mod bits {
    /// V-Blank interrupt source.
    pub const VBLANK: u16 = 1 << 0;
    /// H-Blank interrupt source.
    pub const HBLANK: u16 = 1 << 1;
    /// V-Counter match interrupt source.
    pub const VCOUNT: u16 = 1 << 2;
    /// Timer 0 overflow interrupt source.
    pub const TIMER0: u16 = 1 << 3;
    /// Timer 1 overflow interrupt source.
    pub const TIMER1: u16 = 1 << 4;
    /// Timer 2 overflow interrupt source.
    pub const TIMER2: u16 = 1 << 5;
    /// Timer 3 overflow interrupt source.
    pub const TIMER3: u16 = 1 << 6;
    /// Serial-communication interrupt source.
    pub const SERIAL: u16 = 1 << 7;
    /// DMA 0 interrupt source.
    pub const DMA0: u16 = 1 << 8;
    /// DMA 1 interrupt source.
    pub const DMA1: u16 = 1 << 9;
    /// DMA 2 interrupt source.
    pub const DMA2: u16 = 1 << 10;
    /// DMA 3 interrupt source.
    pub const DMA3: u16 = 1 << 11;
    /// Keypad interrupt source.
    pub const KEYPAD: u16 = 1 << 12;
    /// Game Pak (cartridge) interrupt source.
    pub const GAMEPAK: u16 = 1 << 13;
}

/// All bits actually used by the GBA interrupt controller (14 sources).
pub const IRQ_MASK: u16 = 0x3FFF;

/// Interrupt controller state — `IE`, `IF`, `IME` registers.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InterruptController {
    /// Interrupt Enable (`IE`, 0x04000200) — one bit per interrupt source.
    pub ie: u16,
    /// Interrupt Flag (`IF`, 0x04000202) — pending interrupt requests.
    pub if_flags: u16,
    /// Interrupt Master Enable (`IME`, 0x04000208) — global IRQ gate.
    pub ime: bool,
}

impl InterruptController {
    /// Create a new controller with all registers cleared (real hardware
    /// resets `IE`/`IF`/`IME` to zero).
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the IRQ line should currently be asserted to the CPU. The
    /// line goes high when any pending interrupt is both enabled (`IE`) and
    /// the master enable bit (`IME`) is set.
    pub fn irq_line(&self) -> bool {
        self.ime && (self.ie & self.if_flags & IRQ_MASK) != 0
    }

    /// Whether the HALT state should exit. On real GBA hardware the CPU
    /// exits HALT when any interrupt fires that is enabled in `IE`,
    /// regardless of `IME`. The `IME` flag only controls whether the CPU
    /// actually vectors to the interrupt handler.
    pub fn halt_exit_line(&self) -> bool {
        (self.ie & self.if_flags & IRQ_MASK) != 0
    }

    /// Raise an interrupt source. The bit is OR-ed into `IF`; whether the
    /// line actually asserts depends on `IE`/`IME`.
    pub fn raise(&mut self, sources: u16) {
        self.if_flags |= sources & IRQ_MASK;
    }

    /// Acknowledge interrupt sources — bits set in `value` are cleared in
    /// `IF` (write-1-to-clear semantics).
    pub fn acknowledge(&mut self, value: u16) {
        self.if_flags &= !(value & IRQ_MASK);
    }

    /// Write to `IE` (0x04000200).
    pub fn write_ie(&mut self, value: u16) {
        self.ie = value & IRQ_MASK;
    }

    /// Write to `IF` (0x04000202) — write-1-to-clear.
    pub fn write_if(&mut self, value: u16) {
        self.acknowledge(value);
    }

    /// Write to `IME` (0x04000208) — only bit 0 is significant.
    pub fn write_ime(&mut self, value: u16) {
        self.ime = (value & 1) != 0;
    }

    /// Read `IME` (0x04000208) — bit 0 reflects the master-enable.
    pub fn read_ime(&self) -> u16 {
        u16::from(self.ime)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_cleared_and_line_low() {
        let ic = InterruptController::new();
        assert_eq!(ic.ie, 0);
        assert_eq!(ic.if_flags, 0);
        assert!(!ic.ime);
        assert!(!ic.irq_line());
    }

    #[test]
    fn line_requires_ime_ie_and_pending() {
        let mut ic = InterruptController::new();
        ic.write_ie(bits::TIMER0);
        ic.raise(bits::TIMER0);
        assert!(!ic.irq_line(), "IME=0 must keep line low");
        ic.write_ime(1);
        assert!(ic.irq_line(), "IME=1 + IE & IF must assert line");
    }

    #[test]
    fn write_one_to_clear_if() {
        let mut ic = InterruptController::new();
        ic.write_ie(0xFFFF);
        ic.write_ime(1);
        ic.raise(bits::TIMER0 | bits::VBLANK);
        assert!(ic.irq_line());
        ic.write_if(bits::TIMER0);
        assert_eq!(ic.if_flags, bits::VBLANK);
        assert!(ic.irq_line(), "VBlank still pending");
        ic.write_if(bits::VBLANK);
        assert!(!ic.irq_line());
    }

    #[test]
    fn ie_and_if_mask_unused_bits() {
        let mut ic = InterruptController::new();
        ic.write_ie(0xFFFF);
        assert_eq!(ic.ie, IRQ_MASK);
        ic.raise(0xFFFF);
        assert_eq!(ic.if_flags, IRQ_MASK);
    }

    #[test]
    fn ime_only_bit_0_is_significant() {
        let mut ic = InterruptController::new();
        ic.write_ime(0x1234);
        assert!(!ic.ime, "bit 0 is 0 → disabled");
        assert_eq!(ic.read_ime(), 0);
        ic.write_ime(0x1235);
        assert!(ic.ime);
        assert_eq!(ic.read_ime(), 1);
    }
}
