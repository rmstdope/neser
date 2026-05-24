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
use crate::gba::input::{Keypad, REG_KEYCNT, REG_KEYINPUT};
use crate::gba::ppu::{self, Ppu};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        (off + 1 < IO_SIZE).then_some(off)
    }

    /// Read a raw halfword from the backing store (no masking).
    pub fn backing_u16(&self, addr: u32) -> u16 {
        Self::idx(addr)
            .map(|i| u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]))
            .unwrap_or(0)
    }

    /// Returns `true` when the current SIO mode is UART (RCNT bit 15 = 0
    /// AND SIOCNT bits 12-13 = 11).
    fn is_uart_mode(&self) -> bool {
        self.backing_u16(0x0400_0134) & 0x8000 == 0
            && (self.backing_u16(0x0400_0128) >> 12) & 3 == 3
    }

    /// Returns `true` when the current SIO mode is Normal 32-bit
    /// (RCNT bit 15 = 0 AND SIOCNT bits 12-13 = 01).
    fn is_normal32_mode(&self) -> bool {
        self.backing_u16(0x0400_0134) & 0x8000 == 0
            && (self.backing_u16(0x0400_0128) >> 12) & 3 == 1
    }

    /// Compute the RCNT read mask based on the current SIO mode.
    ///
    /// All modes clear bits 9-13. Pin-state bits 0-3 vary:
    /// - Normal 8/32: bits 1,3 forced low (SD, SO idle low).
    /// - JOY Bus: bits 0,1 forced low (SC, SD idle low).
    /// - Multi / UART / GPIO: all pin bits pass through.
    fn rcnt_read_mask(&self) -> u16 {
        let rcnt = self.backing_u16(0x0400_0134);
        let base_mask: u16 = 0xC1FF; // bits 14-15 + bits 0-8; clears 9-13
        let pin_mask: u16 = if rcnt & 0x8000 != 0 {
            // GPIO (bit 14=0) or JOY Bus (bit 14=1).
            if rcnt & 0x4000 != 0 {
                !0x0003 // JOY Bus: clear bits 0-1
            } else {
                0xFFFF // GPIO: all pins pass through
            }
        } else {
            let siocnt = self.backing_u16(0x0400_0128);
            match (siocnt >> 12) & 3 {
                0 | 1 => !0x000A, // Normal 8/32: clear bits 1,3
                _ => 0xFFFF,      // Multi / UART: all pins pass through
            }
        };
        base_mask & pin_mask
    }

    /// Try to read a halfword from the I/O register space.
    ///
    /// Returns `None` for write-only registers and addresses outside the
    /// 1 KB I/O window so the bus can supply the correct open-bus value.
    /// Readable registers with unused bits apply read masks per GBATek.
    pub fn try_read16(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> Option<u16> {
        match addr {
            REG_IE => Some(ic.ie),
            REG_IF => Some(ic.if_flags),
            REG_IME => Some(ic.read_ime()),
            // PPU display registers (readable).
            ppu::REG_DISPCNT => Some(ppu.read_dispcnt()),
            ppu::REG_GREEN_SWAP => Some(ppu.read_green_swap()),
            ppu::REG_BG0CNT => Some(ppu.read_bg_cnt(0)),
            ppu::REG_BG1CNT => Some(ppu.read_bg_cnt(1)),
            ppu::REG_BG2CNT => Some(ppu.read_bg_cnt(2)),
            ppu::REG_BG3CNT => Some(ppu.read_bg_cnt(3)),
            ppu::REG_DISPSTAT => Some(ppu.read_dispstat()),
            ppu::REG_VCOUNT => Some(ppu.read_vcount()),
            // PPU write-only registers → open-bus.
            0x0400_0010..=0x0400_001E => None, // BG scroll offsets
            0x0400_0020..=0x0400_003E => None, // BG affine params
            0x0400_0040..=0x0400_0046 => None, // Window H/V coords
            ppu::REG_MOSAIC => None,           // MOSAIC
            0x0400_0054 => None,               // BLDY
            // PPU readable registers with masks.
            0x0400_0048 => Some(ppu.read_winin()),    // WININ
            0x0400_004A => Some(ppu.read_winout()),   // WINOUT
            0x0400_0050 => Some(ppu.read_bldcnt()),   // BLDCNT
            0x0400_0052 => Some(ppu.read_bldalpha()), // BLDALPHA
            // Invalid addresses in PPU/blend range → open-bus.
            0x0400_004E | 0x0400_0056..=0x0400_005E => None,
            // Invalid addresses after sound FIFO (not in APU intercept range).
            0x0400_00A8..=0x0400_00AF => None,
            // Keypad.
            REG_KEYINPUT => Some(keypad.read_keyinput()),
            REG_KEYCNT => Some(keypad.read_keycnt()),
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
            // DMA registers.
            0x0400_00B0..=0x0400_00DF => dma.try_read16(addr),
            // Invalid addresses post-DMA → open-bus.
            0x0400_00E0..=0x0400_00FF => None,
            // --- SIO / JOY Bus registers ---
            // SIODATA32_L/H (SIOMULTI0/1): R/W in Normal-32 mode, 0 otherwise.
            0x0400_0120 | 0x0400_0122 => {
                if self.is_normal32_mode() {
                    self.read_backing(addr, 0xFFFF)
                } else {
                    Some(0)
                }
            }
            // SIOMULTI2/3: receive-only buffers, always 0 (disconnected).
            0x0400_0124 | 0x0400_0126 => Some(0),
            // SIOCNT: read is intercepted at bus level via Sio::read_siocnt().
            // This path is only reached from io.rs unit tests (not bus reads).
            0x0400_0128 => {
                let mask = if self.is_uart_mode() { 0x7FAF } else { 0x7F8F };
                self.read_backing(addr, mask)
            }
            // SIOMLT_SEND / SIODATA8: backing in most modes, 0 in UART.
            0x0400_012A => {
                if self.is_uart_mode() {
                    Some(0)
                } else {
                    self.read_backing(addr, 0xFFFF)
                }
            }
            // RCNT: mode-dependent read mask (bits 9-13 always clear,
            // pin bits 0-3 vary by mode).
            0x0400_0134 => self.read_backing(addr, self.rcnt_read_mask()),
            // Invalid system-area addresses → read as zero (not open-bus).
            0x0400_0136 | 0x0400_0142 | 0x0400_015A | 0x0400_0206 | 0x0400_020A | 0x0400_0302 => {
                Some(0)
            }
            // JOYCNT: R/W with mask 0x0047.
            0x0400_0140 => self.read_backing(addr, 0x0047),
            // JOY_RECV / JOY_TRANS: always 0 (disconnected, no JOY Bus peer).
            0x0400_0150..=0x0400_0156 => Some(0),
            // JOYSTAT: always 0 (disconnected).
            0x0400_0158 => Some(0),
            _ => Self::idx(addr).map(|i| u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]])),
        }
    }

    /// Read a halfword from the backing store with a mask applied.
    fn read_backing(&self, addr: u32, mask: u16) -> Option<u16> {
        Self::idx(addr).map(|i| u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]) & mask)
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
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> Option<u32> {
        let lo = self.try_read16(addr, ic, timers, dma, ppu, keypad)? as u32;
        let hi = self.try_read16(addr.wrapping_add(2), ic, timers, dma, ppu, keypad)? as u32;
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
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> Option<u8> {
        let hw = self.try_read16(addr & !1, ic, timers, dma, ppu, keypad)?;
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
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> u16 {
        self.try_read16(addr, ic, timers, dma, ppu, keypad)
            .unwrap_or(0)
    }

    /// Read a word from the I/O register space, returning 0 for addresses
    /// outside the 1 KB I/O window.
    pub fn read32(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> u32 {
        self.try_read32(addr, ic, timers, dma, ppu, keypad)
            .unwrap_or(0)
    }

    /// Read a byte from the I/O register space, returning 0 for addresses
    /// outside the 1 KB I/O window.
    pub fn read8(
        &self,
        addr: u32,
        ic: &InterruptController,
        timers: &Timers,
        dma: &DmaController,
        ppu: &Ppu,
        keypad: &Keypad,
    ) -> u8 {
        self.try_read8(addr, ic, timers, dma, ppu, keypad)
            .unwrap_or(0)
    }

    /// Write a halfword to the I/O register space.
    #[allow(clippy::too_many_arguments)]
    pub fn write16(
        &mut self,
        addr: u32,
        value: u16,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
        ppu: &mut Ppu,
        keypad: &mut Keypad,
    ) {
        match addr {
            REG_IE => ic.write_ie(value),
            REG_IF => ic.write_if(value),
            REG_IME => ic.write_ime(value),
            // PPU display registers.
            ppu::REG_DISPCNT => ppu.write_dispcnt(value),
            ppu::REG_GREEN_SWAP => ppu.write_green_swap(value),
            ppu::REG_BG0CNT => ppu.write_bg_cnt(0, value),
            ppu::REG_BG1CNT => ppu.write_bg_cnt(1, value),
            ppu::REG_BG2CNT => ppu.write_bg_cnt(2, value),
            ppu::REG_BG3CNT => ppu.write_bg_cnt(3, value),
            ppu::REG_DISPSTAT => ppu.write_dispstat(value, ic),
            ppu::REG_VCOUNT => { /* VCOUNT is read-only */ }
            ppu::REG_BG0HOFS => ppu.write_bg_hofs(0, value),
            ppu::REG_BG0VOFS => ppu.write_bg_vofs(0, value),
            ppu::REG_BG1HOFS => ppu.write_bg_hofs(1, value),
            ppu::REG_BG1VOFS => ppu.write_bg_vofs(1, value),
            ppu::REG_BG2HOFS => ppu.write_bg_hofs(2, value),
            ppu::REG_BG2VOFS => ppu.write_bg_vofs(2, value),
            ppu::REG_BG3HOFS => ppu.write_bg_hofs(3, value),
            ppu::REG_BG3VOFS => ppu.write_bg_vofs(3, value),
            // PPU affine BG2/BG3 registers (write-only, reads fall
            // through to the I/O backing store / open-bus).
            0x0400_0020..=0x0400_003E => {
                ppu.write_affine(addr, value);
            }
            // PPU window registers.
            ppu::REG_WIN0H => ppu.write_win_h(0, value),
            ppu::REG_WIN1H => ppu.write_win_h(1, value),
            ppu::REG_WIN0V => ppu.write_win_v(0, value),
            ppu::REG_WIN1V => ppu.write_win_v(1, value),
            ppu::REG_WININ => ppu.write_winin(value),
            ppu::REG_WINOUT => ppu.write_winout(value),
            ppu::REG_MOSAIC => ppu.write_mosaic(value),
            // PPU color special effect registers.
            ppu::REG_BLDCNT => ppu.write_bldcnt(value),
            ppu::REG_BLDALPHA => ppu.write_bldalpha(value),
            ppu::REG_BLDY => ppu.write_bldy(value),
            // Keypad.
            REG_KEYINPUT => { /* KEYINPUT is read-only */ }
            REG_KEYCNT => keypad.write_keycnt(value, ic),
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
            // JOYCNT: bits 0-2 are write-1-to-clear flags, bit 6 is R/W.
            0x0400_0140 => {
                if let Some(i) = Self::idx(addr) {
                    let cur = u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]);
                    let new_val = (cur & !(value & 0x07)) | (value & 0x40);
                    let b = new_val.to_le_bytes();
                    self.bytes[i] = b[0];
                    self.bytes[i + 1] = b[1];
                }
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
    #[allow(clippy::too_many_arguments)]
    pub fn write32(
        &mut self,
        addr: u32,
        value: u32,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
        ppu: &mut Ppu,
        keypad: &mut Keypad,
    ) {
        self.write16(addr, value as u16, ic, timers, dma, ppu, keypad);
        self.write16(
            addr.wrapping_add(2),
            (value >> 16) as u16,
            ic,
            timers,
            dma,
            ppu,
            keypad,
        );
    }

    /// Write a byte to the I/O register space — many GBA I/O registers
    /// don't accept 8-bit writes; we model the simple "byte-merge into the
    /// containing halfword" semantics which is correct for the registers
    /// covered by this foundation.
    #[allow(clippy::too_many_arguments)]
    pub fn write8(
        &mut self,
        addr: u32,
        value: u8,
        ic: &mut InterruptController,
        timers: &mut Timers,
        dma: &mut DmaController,
        ppu: &mut Ppu,
        keypad: &mut Keypad,
    ) {
        // DMA registers (0x0400_00B0..=0x0400_00DF) need a dedicated
        // byte path because SAD/DAD/CNT_L are write-only — the generic
        // read-modify-write below would zero the untouched byte.
        if (0x0400_00B0..=0x0400_00DF).contains(&addr) {
            dma.write8(addr, value);
            return;
        }
        // PPU affine BG registers (0x0400_0020..=0x0400_003E) are also
        // write-only on hardware. Reads fall through to the I/O backing
        // store (zero), so the generic read-modify-write would clobber
        // the untouched byte. Merge against the PPU's live affine state
        // instead.
        if (0x0400_0020..=0x0400_003E).contains(&addr) {
            let aligned = addr & !1;
            let current = ppu.read_affine(aligned).unwrap_or(0);
            let merged = if addr & 1 == 0 {
                (current & 0xFF00) | value as u16
            } else {
                (current & 0x00FF) | ((value as u16) << 8)
            };
            ppu.write_affine(aligned, merged);
            return;
        }
        // BG scroll registers (0x10..0x1E) are write-only. Merge byte
        // writes against the PPU's live scroll state instead of io.read16()
        // (which returns open-bus zero for these addresses).
        if (ppu::REG_BG0HOFS..=ppu::REG_BG3VOFS + 1).contains(&addr) {
            let aligned = addr & !1;
            let bg = ((aligned - ppu::REG_BG0HOFS) / 4) as usize;
            let is_vofs = (aligned - ppu::REG_BG0HOFS) % 4 == 2;
            let current = if is_vofs {
                ppu.read_bg_vofs(bg)
            } else {
                ppu.read_bg_hofs(bg)
            };
            let merged = if addr & 1 == 0 {
                (current & 0xFF00) | value as u16
            } else {
                (current & 0x00FF) | ((value as u16) << 8)
            };
            if is_vofs {
                ppu.write_bg_vofs(bg, merged);
            } else {
                ppu.write_bg_hofs(bg, merged);
            }
            return;
        }
        // Window H/V registers (0x40..0x46) are write-only. Merge byte
        // writes against the PPU's live window state.
        // Layout: WIN0H=0x40, WIN1H=0x42, WIN0V=0x44, WIN1V=0x46
        if (ppu::REG_WIN0H..=ppu::REG_WIN1V + 1).contains(&addr) {
            let aligned = addr & !1;
            let is_v = aligned >= ppu::REG_WIN0V;
            let idx = ((aligned >> 1) & 1) as usize; // 0 or 1
            let current = if is_v {
                ppu.read_win_v(idx)
            } else {
                ppu.read_win_h(idx)
            };
            let merged = if addr & 1 == 0 {
                (current & 0xFF00) | value as u16
            } else {
                (current & 0x00FF) | ((value as u16) << 8)
            };
            if is_v {
                ppu.write_win_v(idx, merged);
            } else {
                ppu.write_win_h(idx, merged);
            }
            return;
        }
        // MOSAIC (0x0400_004C..=0x0400_004D) is write-only. Merge byte writes
        // against the PPU's live value.
        if (ppu::REG_MOSAIC..=ppu::REG_MOSAIC + 1).contains(&addr) {
            let current = ppu.read_mosaic();
            let merged = if addr & 1 == 0 {
                (current & 0xFF00) | value as u16
            } else {
                (current & 0x00FF) | ((value as u16) << 8)
            };
            ppu.write_mosaic(merged);
            return;
        }
        // BLDY (0x0400_0054..=0x0400_0055) is write-only. The high byte is
        // unused (EVY lives only in bits 0-4 of the low byte). Merge byte
        // writes against the PPU's live bldy value so a high-byte write does
        // not clobber the EVY coefficient.
        if (ppu::REG_BLDY..=ppu::REG_BLDY + 1).contains(&addr) {
            let current = ppu.read_bldy();
            let merged = if addr & 1 == 0 {
                (current & 0xFF00) | value as u16
            } else {
                (current & 0x00FF) | ((value as u16) << 8)
            };
            ppu.write_bldy(merged);
            return;
        }
        let aligned = addr & !1;
        let current = self.read16(aligned, ic, timers, dma, ppu, keypad);
        let merged = if addr & 1 == 0 {
            (current & 0xFF00) | value as u16
        } else {
            (current & 0x00FF) | ((value as u16) << 8)
        };
        self.write16(aligned, merged, ic, timers, dma, ppu, keypad);
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
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        // WAITCNT (0x204) is not specially handled — should just
        // round-trip through the backing store.
        io.write16(0x0400_0204, 0xBEEF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(io.read16(0x0400_0204, &ic, &t, &d, &p, &k), 0xBEEF);
    }

    #[test]
    fn affine_bg_writes_route_to_ppu_and_reads_are_open_bus_zero() {
        // BG2/BG3 affine registers (0x20..=0x3E) are write-only on
        // hardware: writes must reach the PPU, reads must return the
        // I/O backing-store value (zero by default), not the PPU state.
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // BG2PA = identity scale.
        io.write16(
            ppu::REG_BG2PA,
            0x0100,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        // BG3Y high halfword with bit 27 set → sign-extends to negative.
        io.write16(
            ppu::REG_BG3Y_H,
            0x0FFF,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write16(
            ppu::REG_BG3Y_L,
            0xFFFF,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        // PPU side has the live values.
        assert_eq!(p.bg_affine(0).expect("BG2 affine").pa, 0x0100);
        assert_eq!(p.bg_affine(1).expect("BG3 affine").y, -1);

        // Bus-side reads return open-bus / backing-store zero, NOT the
        // latched PPU value (write-only register).
        assert_eq!(io.read16(ppu::REG_BG2PA, &ic, &t, &d, &p, &k), 0);
        assert_eq!(io.read16(ppu::REG_BG3Y_L, &ic, &t, &d, &p, &k), 0);
    }

    #[test]
    fn affine_bg_byte_writes_merge_into_live_ppu_state() {
        // Affine BG registers are write-only: io.read16 returns 0. A
        // generic read-modify-write byte path would therefore clobber
        // the previously-written byte. Verify that two byte writes to
        // BG2PA preserve each other.
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Low byte first, then high byte — the high byte must not
        // wipe out the previously-written low byte.
        io.write8(
            ppu::REG_BG2PA,
            0x34,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_BG2PA + 1,
            0x12,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.bg_affine(0).expect("BG2 affine").pa as u16, 0x1234);

        // Same for an X reference-point halfword (low halfword).
        io.write8(
            ppu::REG_BG2X_L,
            0xCD,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_BG2X_L + 1,
            0xAB,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(
            (p.bg_affine(0).expect("BG2 affine").x as u32) & 0xFFFF,
            0xABCD
        );

        // Bus-side reads remain open-bus / backing-store zero.
        assert_eq!(io.read16(ppu::REG_BG2PA, &ic, &t, &d, &p, &k), 0);
        assert_eq!(io.read16(ppu::REG_BG2X_L, &ic, &t, &d, &p, &k), 0);
    }

    #[test]
    fn try_read_returns_none_outside_window() {
        let io = IoRegisters::new();
        let ic = InterruptController::new();
        let t = Timers::new();
        let d = DmaController::new();
        let p = Ppu::new();
        let k = Keypad::new();
        // 0x0400_0400 is one past the documented I/O window.
        assert_eq!(io.try_read16(0x0400_0400, &ic, &t, &d, &p, &k), None);
        // Halfword starting at the last byte must also be rejected to
        // avoid an out-of-bounds backing-store access.
        assert_eq!(io.try_read16(0x0400_03FF, &ic, &t, &d, &p, &k), None);
        // An address below the I/O base must not underflow.
        assert_eq!(io.try_read16(0x0300_0000, &ic, &t, &d, &p, &k), None);
    }

    #[test]
    fn try_read_returns_some_inside_window() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(0x0400_03FE, 0x1234, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_03FE, &ic, &t, &d, &p, &k),
            Some(0x1234)
        );
    }

    #[test]
    fn ie_dispatches_to_interrupt_controller() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(REG_IE, 0x1234, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(ic.ie, 0x1234 & super::super::interrupt::IRQ_MASK);
        assert_eq!(io.read16(REG_IE, &ic, &t, &d, &p, &k), ic.ie);
    }

    #[test]
    fn timer0_writes_dispatch_to_timer_bank() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(
            REG_TM0CNT_L,
            0xABCD,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write16(
            REG_TM0CNT_H,
            0x0080,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        // After enable rising edge, counter == reload.
        assert_eq!(io.read16(REG_TM0CNT_L, &ic, &t, &d, &p, &k), 0xABCD);
    }

    #[test]
    fn byte_writes_merge_into_halfword_register() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(REG_DISPCNT, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(REG_DISPCNT, 0x12, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(io.read16(REG_DISPCNT, &ic, &t, &d, &p, &k), 0xFF12);
        io.write8(
            REG_DISPCNT + 1,
            0x34,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(io.read16(REG_DISPCNT, &ic, &t, &d, &p, &k), 0x3412);
    }

    #[test]
    fn dma_cnt_h_dispatches_to_dma_controller() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        // Write CNT_H of channel 0 — enable bit set.
        io.write16(0x0400_00BA, 0x8000, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert!(d.channels[0].enabled());
        assert!(d.any_pending());
        // SAD/DAD/CNT_L are write-only (read 0).
        assert_eq!(io.read16(0x0400_00B0, &ic, &t, &d, &p, &k), 0);
    }

    #[test]
    fn dispcnt_dispatches_to_ppu() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(
            ppu::REG_DISPCNT,
            0x0403,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_dispcnt(), 0x0403);
        assert_eq!(io.read16(ppu::REG_DISPCNT, &ic, &t, &d, &p, &k), 0x0403);
    }

    #[test]
    fn vcount_is_read_only_via_io() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(
            ppu::REG_VCOUNT,
            0x00AB,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_vcount(), 0);
        assert_eq!(io.read16(ppu::REG_VCOUNT, &ic, &t, &d, &p, &k), 0);
    }

    #[test]
    fn mode0_bg0_hofs_scroll_changes_leftmost_pixel() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 2 has pixel (0,0) = color index 1.
        vram[64] = 0x01;

        // Map (0,0) = tile 1 (empty), (1,0) = tile 2 (red at x=0).
        vram[0] = 0x01;
        vram[1] = 0x00;
        vram[2] = 0x02;
        vram[3] = 0x00;

        // BG0HOFS = 8 pixels (0x0400_0010).
        io.write16(0x0400_0010, 8, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_vofs_scroll_changes_top_row_pixel() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 3 has pixel (0,0) = color index 1.
        vram[96] = 0x01;

        // Map (0,0) = tile 1 (empty), (0,1) = tile 3 (red at y=8).
        vram[0] = 0x01;
        vram[1] = 0x00;
        vram[64] = 0x03;
        vram[65] = 0x00;

        // BG0VOFS = 8 pixels (0x0400_0012).
        io.write16(0x0400_0012, 8, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_64x32_uses_second_screenblock_for_x_over_255() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        // BG0CNT: screen size = 64x32 (size=1 in bits 14..15).
        io.write16(0x0400_0008, 0x4000, &mut ic, &mut t, &mut d, &mut p, &mut k);

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 3 has pixel (0,0) = color index 1.
        vram[96] = 0x01;

        // Screenblock 0 entry (0,0) = tile 1 (empty).
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;
        // Screenblock 1 entry (0,0) = tile 3 (red).
        vram[0x0800] = 0x03;
        vram[0x0801] = 0x00;

        // Scroll x to 256 so leftmost pixel samples from second screenblock.
        io.write16(0x0400_0010, 256, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_32x64_uses_lower_screenblock_for_y_over_255() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        // BG0CNT: screen size = 32x64 (size=2 in bits 14..15).
        io.write16(0x0400_0008, 0x8000, &mut ic, &mut t, &mut d, &mut p, &mut k);

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 3 has pixel (0,0) = color index 1.
        vram[96] = 0x01;

        // Screenblock 0 entry (0,0) = tile 1 (empty).
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;
        // Screenblock 1 entry (0,0) = tile 3 (red).
        vram[0x0800] = 0x03;
        vram[0x0801] = 0x00;

        // Scroll y to 256 so top pixel samples from second screenblock row.
        io.write16(0x0400_0012, 256, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_64x64_uses_bottom_right_screenblock_for_xy_over_255() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        // BG0CNT: screen size = 64x64 (size=3 in bits 14..15).
        io.write16(0x0400_0008, 0xC000, &mut ic, &mut t, &mut d, &mut p, &mut k);

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 3 has pixel (0,0) = color index 1.
        vram[96] = 0x01;

        // Screenblock 0 entry (0,0) = tile 1 (empty).
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;
        // Screenblock 3 entry (0,0) = tile 3 (red).
        vram[0x1800] = 0x03;
        vram[0x1801] = 0x00;

        // Scroll to x=256, y=256 so sample lands in bottom-right block.
        io.write16(0x0400_0010, 256, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write16(0x0400_0012, 256, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_hofs_byte_writes_preserve_low_byte() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 2 has pixel (0,0) = color index 1.
        vram[64] = 0x01;

        // Map (0,0) = tile 1 (empty), (1,0) = tile 2 (red at x=8).
        vram[0] = 0x01;
        vram[1] = 0x00;
        vram[2] = 0x02;
        vram[3] = 0x00;

        // BG0HOFS byte writes: low=8 then high=0.
        io.write8(0x0400_0010, 0x08, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_0011, 0x00, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_vofs_byte_writes_preserve_low_byte() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let mut vram = vec![0u8; 96 * 1024];
        let mut pram = vec![0u8; 1024];

        // Mode 0 + BG0 enabled.
        io.write16(
            ppu::REG_DISPCNT,
            ppu::dispcnt::BG0_ENABLE,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 3 has pixel (0,0) = color index 1.
        vram[96] = 0x01;

        // Map (0,0) = tile 1 (empty), (0,1) = tile 3 (red at y=8).
        vram[0] = 0x01;
        vram[1] = 0x00;
        vram[64] = 0x03;
        vram[65] = 0x00;

        // BG0VOFS byte writes: low=8 then high=0.
        io.write8(0x0400_0012, 0x08, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_0013, 0x00, &mut ic, &mut t, &mut d, &mut p, &mut k);

        p.step(
            ppu::CYCLES_PER_SCANLINE * ppu::SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &[0u8; 1024],
        );

        assert_eq!(&p.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    /// Byte writes to BG1-BG3 HOFS/VOFS must merge correctly against the
    /// PPU's live scroll state (not the I/O backing store which is zero
    /// for these write-only registers). Regression test for #2383.
    #[test]
    fn bg1_hofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Write low byte = 0xAB, then high byte = 0x01 to BG1HOFS.
        io.write8(0x0400_0014, 0xAB, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_0015, 0x01, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_hofs(1), 0x01AB);
    }

    #[test]
    fn bg1_vofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(0x0400_0016, 0xCD, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_0017, 0x01, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_vofs(1), 0x01CD);
    }

    #[test]
    fn bg2_hofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(0x0400_0018, 0x42, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_0019, 0x01, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_hofs(2), 0x0142);
    }

    #[test]
    fn bg2_vofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(0x0400_001A, 0x77, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_001B, 0x00, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_vofs(2), 0x0077);
    }

    #[test]
    fn bg3_hofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(0x0400_001C, 0xFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_001D, 0x01, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_hofs(3), 0x01FF);
    }

    #[test]
    fn bg3_vofs_byte_writes_merge_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(0x0400_001E, 0x10, &mut ic, &mut t, &mut d, &mut p, &mut k);
        io.write8(0x0400_001F, 0x01, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bg_vofs(3), 0x0110);
    }

    // ---------------------------------------------------------------
    // I/O read-back tests (per GBATek I/O map & mgba-emu/suite io-read)
    // ---------------------------------------------------------------

    /// Write-only PPU registers (BG scroll offsets, affine params, window
    /// coords, MOSAIC, BLDY) must return `None` from `try_read16` so the
    /// bus can substitute the open-bus value.
    #[test]
    fn write_only_ppu_registers_return_none() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let write_only_addrs: &[(u32, &str)] = &[
            // BG scroll offsets (0x10..0x1E)
            (0x0400_0010, "BG0HOFS"),
            (0x0400_0012, "BG0VOFS"),
            (0x0400_0014, "BG1HOFS"),
            (0x0400_0016, "BG1VOFS"),
            (0x0400_0018, "BG2HOFS"),
            (0x0400_001A, "BG2VOFS"),
            (0x0400_001C, "BG3HOFS"),
            (0x0400_001E, "BG3VOFS"),
            // BG affine params (0x20..0x3E)
            (ppu::REG_BG2PA, "BG2PA"),
            (ppu::REG_BG2PB, "BG2PB"),
            (ppu::REG_BG2X_L, "BG2X_LO"),
            (ppu::REG_BG2Y_H, "BG2Y_HI"),
            (ppu::REG_BG3PA, "BG3PA"),
            (ppu::REG_BG3Y_L, "BG3Y_LO"),
            // Window coordinates (0x40..0x46)
            (0x0400_0040, "WIN0H"),
            (0x0400_0042, "WIN1H"),
            (0x0400_0044, "WIN0V"),
            (0x0400_0046, "WIN1V"),
            // MOSAIC
            (0x0400_004C, "MOSAIC"),
            // BLDY (write-only)
            (0x0400_0054, "BLDY"),
        ];

        for &(addr, name) in write_only_addrs {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                None,
                "{name} at {addr:#010X} should return None (write-only)"
            );
        }
    }

    /// Readable PPU registers with masks: WININ, WINOUT, BLDCNT, BLDALPHA.
    /// After writing 0xFFFF, reads must return the value with unused bits
    /// masked out.
    #[test]
    fn readable_ppu_registers_apply_read_masks() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let masked_regs: &[(u32, u16, &str)] = &[
            (0x0400_0048, 0x3F3F, "WININ"),
            (0x0400_004A, 0x3F3F, "WINOUT"),
            (0x0400_0050, 0x3FFF, "BLDCNT"),
            (0x0400_0052, 0x1F1F, "BLDALPHA"),
        ];

        for &(addr, expected, name) in masked_regs {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                Some(expected),
                "{name} at {addr:#010X} should read back {expected:#06X}"
            );
        }
    }

    /// Invalid addresses in the PPU range (0x4E, 0x56-0x5E) and post-DMA
    /// range (0xA8-0xAE, 0xE0-0xFE) must return `None` (open-bus).
    #[test]
    fn invalid_io_addresses_return_none() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let invalid_addrs: &[(u32, &str)] = &[
            (0x0400_004E, "INVALID (4E)"),
            (0x0400_0056, "INVALID (56)"),
            (0x0400_0058, "INVALID (58)"),
            (0x0400_005A, "INVALID (5A)"),
            (0x0400_005C, "INVALID (5C)"),
            (0x0400_005E, "INVALID (5E)"),
            (0x0400_00A8, "INVALID (A8)"),
            (0x0400_00AA, "INVALID (AA)"),
            (0x0400_00AC, "INVALID (AC)"),
            (0x0400_00AE, "INVALID (AE)"),
            (0x0400_00E0, "INVALID (E0)"),
            (0x0400_00F0, "INVALID (F0)"),
            (0x0400_00FE, "INVALID (FE)"),
        ];

        for &(addr, name) in invalid_addrs {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                None,
                "{name} at {addr:#010X} should return None (open-bus)"
            );
        }
    }

    /// Invalid system-area addresses (0x136, 0x142, 0x15A, 0x206, 0x20A,
    /// 0x302) read as zero on real hardware — not open-bus.
    #[test]
    fn invalid_system_addresses_return_zero() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        let system_addrs: &[(u32, &str)] = &[
            (0x0400_0136, "INVALID (136)"),
            (0x0400_0142, "INVALID (142)"),
            (0x0400_015A, "INVALID (15A)"),
            (0x0400_0206, "INVALID (206)"),
            (0x0400_020A, "INVALID (20A)"),
            (0x0400_0302, "INVALID (302)"),
        ];

        for &(addr, name) in system_addrs {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                Some(0),
                "{name} at {addr:#010X} should return Some(0)"
            );
        }
    }

    /// Helper to set SIO mode via RCNT + SIOCNT writes.
    #[allow(clippy::too_many_arguments)]
    fn set_sio_mode(
        io: &mut IoRegisters,
        ic: &mut InterruptController,
        t: &mut Timers,
        d: &mut DmaController,
        p: &mut Ppu,
        k: &mut Keypad,
        siocnt: u16,
        rcnt: u16,
    ) {
        io.write16(0x0400_0134, rcnt, ic, t, d, p, k);
        io.write16(0x0400_0128, siocnt, ic, t, d, p, k);
    }

    /// SIOMULTI2 (0x124) and SIOMULTI3 (0x126) are receive-only buffers
    /// for multi-player mode. They always read as 0 when disconnected,
    /// regardless of what was written to them.
    #[test]
    fn siomulti2_3_always_read_zero() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        for addr in [0x0400_0124, 0x0400_0126] {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                Some(0),
                "SIOMULTI at {addr:#010X} should always read 0"
            );
        }
    }

    /// SIODATA32_L/H (0x120/0x122) are R/W only in Normal 32-bit mode.
    /// In all other modes, they behave as receive buffers returning 0.
    #[test]
    fn siodata32_mode_dependent_read() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Normal 32-bit mode: SIODATA32 is R/W.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x1000, 0);
        io.write16(0x0400_0120, 0xBEEF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0120, &ic, &t, &d, &p, &k),
            Some(0xBEEF),
            "SIODATA32_L in N32 mode should read back written value"
        );

        // Multi-player mode: reads as 0.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x2000, 0);
        io.write16(0x0400_0120, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0120, &ic, &t, &d, &p, &k),
            Some(0),
            "SIODATA32_L in Multi mode should read 0"
        );
    }

    /// SIOCNT (0x128) read mask: non-UART modes mask = 0x7F8F,
    /// UART mode mask = 0x7FAF.
    #[test]
    fn siocnt_read_mask_mode_dependent() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Multi-player mode: write 0xEFFF, expect 0x6F8F.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x2000, 0);
        io.write16(0x0400_0128, 0xEFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0128, &ic, &t, &d, &p, &k),
            Some(0x6F8F),
            "SIOCNT in Multi mode should apply mask 0x7F8F"
        );

        // UART mode: write 0xFFFF, expect 0x7FAF.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x3000, 0);
        io.write16(0x0400_0128, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0128, &ic, &t, &d, &p, &k),
            Some(0x7FAF),
            "SIOCNT in UART mode should apply mask 0x7FAF"
        );
    }

    /// SIOMLT_SEND / SIODATA8 (0x12A) reads back the written value in
    /// most modes, but returns 0 in UART mode.
    #[test]
    fn siomlt_send_uart_reads_zero() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Multi mode: read back written value.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x2000, 0);
        io.write16(0x0400_012A, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_012A, &ic, &t, &d, &p, &k),
            Some(0xFFFF),
            "SIOMLT_SEND in Multi mode should read back written value"
        );

        // UART mode: reads as 0.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x3000, 0);
        io.write16(0x0400_012A, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_012A, &ic, &t, &d, &p, &k),
            Some(0),
            "SIODATA8 in UART mode should read 0"
        );
    }

    /// RCNT (0x134) read mask varies by SIO mode. All modes clear bits
    /// 9-13. Pin state bits 0-3 differ: Normal modes force bits 1,3 = 0;
    /// JOY Bus forces bits 0,1 = 0.
    #[test]
    fn rcnt_mode_dependent_read_mask() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Multi-player: write 0x3FFF, expect 0x01FF.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x2000, 0);
        io.write16(0x0400_0134, 0x3FFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0134, &ic, &t, &d, &p, &k),
            Some(0x01FF),
            "RCNT in Multi mode"
        );

        // Normal 8-bit: write 0x3FFF, expect 0x01F5.
        set_sio_mode(&mut io, &mut ic, &mut t, &mut d, &mut p, &mut k, 0x0000, 0);
        io.write16(0x0400_0134, 0x3FFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0134, &ic, &t, &d, &p, &k),
            Some(0x01F5),
            "RCNT in Normal 8-bit mode"
        );

        // JOY Bus: write 0xFFFF, expect 0xC1FC.
        io.write16(0x0400_0134, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        // RCNT=0xFFFF has bits 15-14 = 11 → JOY Bus mode.
        assert_eq!(
            io.try_read16(0x0400_0134, &ic, &t, &d, &p, &k),
            Some(0xC1FC),
            "RCNT in JOY Bus mode"
        );
    }

    /// JOYCNT (0x140) is R/W with mask 0x0047. Bits 0-2 are
    /// write-1-to-clear flags, bit 6 is a normal R/W bit.
    /// Writing 0xFFFF to a zeroed JOYCNT results in 0x0040
    /// (bits 0-2 cleared by W1C, bit 6 set).
    #[test]
    fn joycnt_read_mask_and_w1c() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write16(0x0400_0140, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(
            io.try_read16(0x0400_0140, &ic, &t, &d, &p, &k),
            Some(0x0040),
            "JOYCNT after writing 0xFFFF should be 0x0040"
        );
    }

    /// JOY_RECV (0x150-0x152) and JOY_TRANS (0x154-0x156) always read
    /// as 0 when disconnected.
    #[test]
    fn joy_recv_trans_always_read_zero() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        for addr in [0x0400_0150, 0x0400_0152, 0x0400_0154, 0x0400_0156] {
            io.write16(addr, 0xFFFF, &mut ic, &mut t, &mut d, &mut p, &mut k);
            assert_eq!(
                io.try_read16(addr, &ic, &t, &d, &p, &k),
                Some(0),
                "JOY register at {addr:#010X} should read 0"
            );
        }
    }

    #[test]
    fn winin_winout_routed_to_ppu() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Write WININ with all bits set — only 0x3F3F should stick.
        io.write16(
            ppu::REG_WININ,
            0xFFFF,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_winin(), 0x3F3F);
        assert_eq!(
            io.try_read16(ppu::REG_WININ, &ic, &t, &d, &p, &k),
            Some(0x3F3F)
        );

        // Write WINOUT.
        io.write16(
            ppu::REG_WINOUT,
            0x3F27,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_winout(), 0x3F27);
        assert_eq!(
            io.try_read16(ppu::REG_WINOUT, &ic, &t, &d, &p, &k),
            Some(0x3F27)
        );

        // Write WIN0H (write-only, routed to PPU).
        io.write16(
            ppu::REG_WIN0H,
            0x1080,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_win_h(0), 0x1080);
    }

    #[test]
    fn window_byte_writes_route_correctly() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Byte write to WIN0H (0x0400_0040): low byte then high byte
        io.write8(
            ppu::REG_WIN0H,
            0xAB,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_WIN0H + 1,
            0xCD,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_win_h(0), 0xCDAB);

        // Byte write to WIN1H (0x0400_0042)
        io.write8(
            ppu::REG_WIN1H,
            0x10,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_WIN1H + 1,
            0x80,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_win_h(1), 0x8010);

        // Byte write to WIN0V (0x0400_0044)
        io.write8(
            ppu::REG_WIN0V,
            0x20,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_WIN0V + 1,
            0xA0,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_win_v(0), 0xA020);

        // Byte write to WIN1V (0x0400_0046)
        io.write8(
            ppu::REG_WIN1V,
            0x00,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_WIN1V + 1,
            0xA0,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_win_v(1), 0xA000);
    }

    #[test]
    fn green_swap_dispatches_to_ppu() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();
        io.write16(
            ppu::REG_GREEN_SWAP,
            1,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(p.read_green_swap(), 1);
        assert_eq!(io.read16(ppu::REG_GREEN_SWAP, &ic, &t, &d, &p, &k), 1);
    }

    #[test]
    fn bldy_high_byte_write_via_io_preserves_evy() {
        // A byte write to BLDY+1 (0x0400_0055) must not clobber the EVY
        // coefficient previously written to the low byte (0x0400_0054).
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        // Write EVY = 12 via the normal halfword path.
        io.write16(ppu::REG_BLDY, 12, &mut ic, &mut t, &mut d, &mut p, &mut k);
        assert_eq!(p.read_bldy(), 12, "EVY should be 12 after halfword write");

        // Now do a high-byte byte write (addr+1 = 0x0400_0055).
        // On hardware the high byte is unused; EVY must remain 12.
        io.write8(
            ppu::REG_BLDY + 1,
            0xFF,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        assert_eq!(
            p.read_bldy(),
            12,
            "EVY must not be cleared by a high-byte write to BLDY+1"
        );
    }

    #[test]
    fn mosaic_halfword_write_routes_to_live_ppu_state() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write16(
            ppu::REG_MOSAIC,
            0xABCD,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        assert_eq!(p.read_mosaic(), 0xABCD);
        assert_eq!(io.try_read16(ppu::REG_MOSAIC, &ic, &t, &d, &p, &k), None);
    }

    #[test]
    fn mosaic_byte_writes_merge_into_live_ppu_state() {
        let mut io = IoRegisters::new();
        let mut ic = InterruptController::new();
        let mut t = Timers::new();
        let mut d = DmaController::new();
        let mut p = Ppu::new();
        let mut k = Keypad::new();

        io.write8(
            ppu::REG_MOSAIC,
            0x34,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );
        io.write8(
            ppu::REG_MOSAIC + 1,
            0x12,
            &mut ic,
            &mut t,
            &mut d,
            &mut p,
            &mut k,
        );

        assert_eq!(p.read_mosaic(), 0x1234);
    }
}
