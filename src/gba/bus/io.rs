//! GBA I/O register dispatch (`0x0400_0000`–`0x0400_03FE`).
//!
//! The GBA has roughly 300 named I/O registers covering PPU, APU, DMA,
//! timers, serial, keypad and interrupt control. This module provides:
//!
//! * A flat 1 KB backing store covering the full I/O window so that any
//!   read/write to a documented register location does not panic.
//! * Special-case dispatch for the registers that are owned by other
//!   subsystems modelled in this crate — the [`InterruptController`] and the
//!   [`Timers`] bank — so writes affect their internal state and reads
//!   return their live state.
//!
//! Subsequent sub-issues (PPU, APU, DMA) will hook additional registers
//! into this dispatch table as the corresponding subsystems come online.
//!
//! Modeled per GBATek "I/O Map".
//!
//! <https://problemkaputt.de/gbatek.htm#gbaiomap>

use super::dma::DmaController;
use super::interrupt::InterruptController;
use super::timer::Timers;

/// Size of the I/O window backing store. Registers above this limit
/// (`0x0400_0400`+) read as open-bus on real hardware.
pub const IO_SIZE: usize = 0x400;

/// Address of `REG_IE`.
pub const REG_IE: u32 = 0x0400_0200;
/// Address of `REG_IF`.
pub const REG_IF: u32 = 0x0400_0202;
/// Address of `REG_IME`.
pub const REG_IME: u32 = 0x0400_0208;

/// Address of timer 0 `CNT_L`. Other timers follow at +4/+8/+12.
pub const REG_TM0CNT_L: u32 = 0x0400_0100;
/// Address of timer 0 `CNT_H`.
pub const REG_TM0CNT_H: u32 = 0x0400_0102;

/// Address of `REG_DISPCNT` (PPU display control).
pub const REG_DISPCNT: u32 = 0x0400_0000;

/// I/O register backing store.
///
/// Most registers are not yet wired to live subsystems — for those, reads
/// and writes simply touch the backing buffer so the CPU sees consistent
/// values without panicking. Specific addresses (interrupt controller,
/// timers) are intercepted in [`Self::write16`] / [`Self::read16`] and
/// dispatched to the live state.
#[derive(Debug, Clone)]
pub struct IoRegisters {
    /// Flat backing store for unimplemented registers.
    bytes: Vec<u8>,
}

impl Default for IoRegisters {
    fn default() -> Self {
        Self::new()
    }
}

impl IoRegisters {
    /// Create a new I/O register block with all storage zero-initialised.
    pub fn new() -> Self {
        Self {
            bytes: vec![0; IO_SIZE],
        }
    }

    /// Compute the backing-store index for a halfword access at `addr`.
    ///
    /// Returns `None` when `addr` is below the I/O base (underflow) or when
    /// the access (1 byte at `i` or 2 bytes at `i`/`i+1`) would extend past
    /// the end of the 1 KB window. Callers can substitute the bus
    /// open-bus value when this returns `None`.
    fn idx(addr: u32) -> Option<usize> {
        let off = addr.checked_sub(0x0400_0000)? as usize;
        // Need both bytes of the halfword to lie within the window.
        if off + 1 < IO_SIZE { Some(off) } else { None }
    }

    /// Try to read a halfword from the I/O register space.
    ///
    /// Returns `None` for addresses outside the 1 KB I/O window so the bus
    /// can supply the correct open-bus value instead of treating the read
    /// as a handled register access returning zero.
    pub fn try_read16(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> Option<u16> {
        match addr {
            REG_IE => Some(ic.ie),
            REG_IF => Some(ic.if_flags),
            REG_IME => Some(ic.read_ime()),
            // Timers: TM{0..3}CNT_L = 0x100, 0x104, 0x108, 0x10C
            // Timers: TM{0..3}CNT_H = 0x102, 0x106, 0x10A, 0x10E
            0x0400_0100 => Some(timers.read_cnt_l(0)),
            0x0400_0102 => Some(timers.read_cnt_h(0)),
            0x0400_0104 => Some(timers.read_cnt_l(1)),
            0x0400_0106 => Some(timers.read_cnt_h(1)),
            0x0400_0108 => Some(timers.read_cnt_l(2)),
            0x0400_010A => Some(timers.read_cnt_h(2)),
            0x0400_010C => Some(timers.read_cnt_l(3)),
            0x0400_010E => Some(timers.read_cnt_h(3)),
            // DMA: 0x0400_00B0..=0x0400_00DF — write-only regs read 0.
            0x0400_00B0..=0x0400_00DF => dma.try_read16(addr),
            _ => Self::idx(addr).map(|i| u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]])),
        }
    }

    /// Try to read a word from the I/O register space (two halfwords).
    ///
    /// Returns `None` when either halfword falls outside the 1 KB I/O
    /// window.
    pub fn try_read32(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> Option<u32> {
        let lo = self.try_read16(addr, ic, timers, dma)? as u32;
        let hi = self.try_read16(addr.wrapping_add(2), ic, timers, dma)? as u32;
        Some(lo | (hi << 16))
    }

    /// Try to read a byte from the I/O register space.
    ///
    /// Returns `None` when the containing halfword lies outside the 1 KB
    /// I/O window.
    pub fn try_read8(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> Option<u8> {
        let hw = self.try_read16(addr & !1, ic, timers, dma)?;
        Some(if addr & 1 == 0 {
            hw as u8
        } else {
            (hw >> 8) as u8
        })
    }

    /// Read a halfword from the I/O register space, returning 0 for
    /// addresses outside the 1 KB I/O window. Prefer [`Self::try_read16`]
    /// from the bus so the correct open-bus value can be substituted.
    pub fn read16(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> u16 {
        self.try_read16(addr, ic, timers, dma).unwrap_or(0)
    }

    /// Read a word from the I/O register space, returning 0 for addresses
    /// outside the 1 KB I/O window.
    pub fn read32(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> u32 {
        self.try_read32(addr, ic, timers, dma).unwrap_or(0)
    }

    /// Read a byte from the I/O register space, returning 0 for addresses
    /// outside the 1 KB I/O window.
    pub fn read8(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
    ) -> u8 {
        self.try_read8(addr, ic, timers, dma).unwrap_or(0)
    }

    /// Write a halfword to the I/O register space.
    pub fn write16(
        &mut self,
        addr: u32,
        value: u16,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
    ) {
        match addr {
            REG_IE => ic.write_ie(value),
            REG_IF => ic.write_if(value),
            REG_IME => ic.write_ime(value),
            0x0400_0100 => timers.write_cnt_l(0, value),
            0x0400_0102 => timers.write_cnt_h(0, value),
            0x0400_0104 => timers.write_cnt_l(1, value),
            0x0400_0106 => timers.write_cnt_h(1, value),
            0x0400_0108 => timers.write_cnt_l(2, value),
            0x0400_010A => timers.write_cnt_h(2, value),
            0x0400_010C => timers.write_cnt_l(3, value),
            0x0400_010E => timers.write_cnt_h(3, value),
            0x0400_00B0..=0x0400_00DF => {
                dma.write16(addr, value);
            }
            _ => {
                if let Some(i) = Self::idx(addr) {
                    let b = value.to_le_bytes();
                    self.bytes[i] = b[0];
                    self.bytes[i + 1] = b[1];
                }
            }
        }
    }

    /// Write a word to the I/O register space (two halfwords).
    pub fn write32(
        &mut self,
        addr: u32,
        value: u32,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
    ) {
        self.write16(addr, value as u16, ic, timers, dma);
        self.write16(addr.wrapping_add(2), (value >> 16) as u16, ic, timers, dma);
    }

    /// Write a byte to the I/O register space — many GBA I/O registers
    /// don't accept 8-bit writes; we model the simple "byte-merge into the
    /// containing halfword" semantics which is correct for the registers
    /// covered by this foundation.
    pub fn write8(
        &mut self,
        addr: u32,
        value: u8,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
    ) {
        let aligned = addr & !1;
        let current = self.read16(aligned, ic, timers, dma);
        let merged = if addr & 1 == 0 {
            (current & 0xFF00) | value as u16
        } else {
            (current & 0x00FF) | ((value as u16) << 8)
        };
        self.write16(aligned, merged, ic, timers, dma);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmapped_register_round_trips_via_storage() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        // 0x40 is REG_SOUNDCNT_H; not specially handled here — should just
        // round-trip through the backing store.
        io.write16(0x0400_0040, 0xBEEF, &mut ic, &mut t, &mut d);
        assert_eq!(io.read16(0x0400_0040, &ic, &t, &d), 0xBEEF);
    }

    #[test]
    fn try_read_returns_none_outside_window() {
        let io = IoRegisters::new();
        let ic = InterruptController::new();
        let t = Timers::new();
        let d = DmaController::new();
        // 0x0400_0400 is one past the documented I/O window.
        assert_eq!(io.try_read16(0x0400_0400, &ic, &t, &d), None);
        // Halfword starting at the last byte must also be rejected to
        // avoid an out-of-bounds backing-store access.
        assert_eq!(io.try_read16(0x0400_03FF, &ic, &t, &d), None);
        // An address below the I/O base must not underflow.
        assert_eq!(io.try_read16(0x0300_0000, &ic, &t, &d), None);
    }

    #[test]
    fn try_read_returns_some_inside_window() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        io.write16(0x0400_03FE, 0x1234, &mut ic, &mut t, &mut d);
        assert_eq!(io.try_read16(0x0400_03FE, &ic, &t, &d), Some(0x1234));
    }

    #[test]
    fn ie_dispatches_to_interrupt_controller() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        io.write16(REG_IE, 0x1234, &mut ic, &mut t, &mut d);
        assert_eq!(ic.ie, 0x1234 & super::super::interrupt::IRQ_MASK);
        assert_eq!(io.read16(REG_IE, &ic, &t, &d), ic.ie);
    }

    #[test]
    fn timer0_writes_dispatch_to_timer_bank() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        io.write16(REG_TM0CNT_L, 0xABCD, &mut ic, &mut t, &mut d);
        io.write16(REG_TM0CNT_H, 0x0080, &mut ic, &mut t, &mut d);
        // After enable rising edge, counter == reload.
        assert_eq!(io.read16(REG_TM0CNT_L, &ic, &t, &d), 0xABCD);
    }

    #[test]
    fn byte_writes_merge_into_halfword_register() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        io.write16(REG_DISPCNT, 0xFFFF, &mut ic, &mut t, &mut d);
        io.write8(REG_DISPCNT, 0x12, &mut ic, &mut t, &mut d);
        assert_eq!(io.read16(REG_DISPCNT, &ic, &t, &d), 0xFF12);
        io.write8(REG_DISPCNT + 1, 0x34, &mut ic, &mut t, &mut d);
        assert_eq!(io.read16(REG_DISPCNT, &ic, &t, &d), 0x3412);
    }

    #[test]
    fn dma_cnt_h_dispatches_to_dma_controller() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        // Write CNT_H of channel 0 — enable bit set.
        io.write16(0x0400_00BA, 0x8000, &mut ic, &mut t, &mut d);
        assert!(d.channels[0].enabled());
        assert!(d.any_pending());
        // SAD/DAD/CNT_L are write-only (read 0).
        assert_eq!(io.read16(0x0400_00B0, &ic, &t, &d), 0);
    }
}
