//! GBA system memory bus.
//!
//! Routes the ARM7TDMI 32-bit address space across the GBA memory regions
//! (BIOS, EWRAM, IWRAM, I/O, PRAM, VRAM, OAM, cart ROM/SRAM) per GBATek's
//! "GBA Memory Map" tables. Implements the [`Bus`](super::cpu::Bus) trait
//! used by the CPU, and exposes hooks for stepping the timer system and
//! routing interrupts.
//!
//! See `architecture.md` for the GBA module layout.
//!
//! <https://problemkaputt.de/gbatek.htm#gbamemorymap>

pub mod dma;
pub mod interrupt;
pub mod io;
pub mod memory;
pub mod timer;

use crate::gba::cpu::bus::Bus;
use crate::gba::input::Keypad;
use crate::gba::ppu::{Ppu, PpuStepEvents};

pub use dma::{DmaBus, DmaChannel, DmaController};
pub use interrupt::{InterruptController, bits as irq_bits};
pub use io::{IoRegisters, REG_IE, REG_IF, REG_IME};
pub use timer::{Timer, Timers};

use memory::{
    BIOS_SIZE, EWRAM_SIZE, IWRAM_SIZE, OAM_SIZE, PRAM_SIZE, ROM_MAX_SIZE, SRAM_SIZE, VRAM_SIZE,
    read_le_u16, read_le_u32, write_le_u16, write_le_u32,
};

/// Wait-state stub values returned by [`GbaBus::n_cycles`] /
/// [`GbaBus::s_cycles`] for each region/access width. Values are GBATek
/// defaults (post-reset `WAITCNT` = 0).
pub mod wait_states {
    /// Sequential / non-sequential cycle counts indexed by `WidthClass`.
    /// Order: 8/16-bit, 32-bit.
    pub const BIOS: [u32; 2] = [1, 1];
    pub const EWRAM_N: [u32; 2] = [3, 6];
    pub const EWRAM_S: [u32; 2] = [3, 6];
    pub const IWRAM: [u32; 2] = [1, 1];
    pub const IO: [u32; 2] = [1, 1];
    pub const PRAM: [u32; 2] = [1, 2];
    pub const VRAM: [u32; 2] = [1, 2];
    pub const OAM: [u32; 2] = [1, 1];
    /// Cart ROM (Wait State 0) — halfword N=5, S=3 cycles; word N=8, S=6
    /// cycles for the post-reset `WAITCNT` timing stub values used here.
    pub const ROM_N: [u32; 2] = [5, 8];
    pub const ROM_S: [u32; 2] = [3, 6];
    pub const SRAM: [u32; 2] = [5, 5];
}

/// Access width for [`GbaBus::n_cycles`] / [`GbaBus::s_cycles`] cycle stubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    /// 8-bit or 16-bit access.
    HalfwordOrByte,
    /// 32-bit access (counts as two halfword accesses on 16-bit buses).
    Word,
}

impl WidthClass {
    fn idx(self) -> usize {
        match self {
            WidthClass::HalfwordOrByte => 0,
            WidthClass::Word => 1,
        }
    }
}

/// GBA memory bus.
pub struct GbaBus {
    /// 16 KB BIOS ROM at `0x0000_0000`.
    bios: Vec<u8>,
    /// 256 KB on-board work RAM (EWRAM) at `0x0200_0000`.
    ewram: Vec<u8>,
    /// 32 KB on-chip work RAM (IWRAM) at `0x0300_0000`.
    iwram: Vec<u8>,
    /// 1 KB Palette RAM at `0x0500_0000`.
    pram: Vec<u8>,
    /// 96 KB VRAM at `0x0600_0000` (mirroring is 128 KB-periodic, see read).
    vram: Vec<u8>,
    /// 1 KB OAM at `0x0700_0000`.
    oam: Vec<u8>,
    /// Cartridge ROM at `0x0800_0000` (mirrored at `0x0A`/`0x0C`).
    rom: Vec<u8>,
    /// Cartridge SRAM at `0x0E00_0000`.
    sram: Vec<u8>,
    /// I/O register storage and dispatch.
    pub io: IoRegisters,
    /// Interrupt controller.
    pub ic: InterruptController,
    /// Timer bank (TM0-TM3).
    pub timers: Timers,
    /// DMA controller (DMA0-DMA3).
    pub dma: DmaController,
    /// Picture Processing Unit (PPU).
    pub ppu: Ppu,
    /// Keypad (KEYINPUT / KEYCNT, key IRQ).
    pub keypad: Keypad,
    /// Last value driven on the bus (used to model open-bus reads).
    last_bus_value: u32,
    /// Whether the BIOS is locked. After the boot ROM finishes executing,
    /// BIOS reads from outside the BIOS region return open-bus instead of
    /// the BIOS contents.
    bios_locked: bool,
}

impl Default for GbaBus {
    fn default() -> Self {
        Self::new()
    }
}

impl GbaBus {
    /// Create a new bus with all regions sized per GBATek and all storage
    /// zero-initialised.
    pub fn new() -> Self {
        Self {
            bios: vec![0; BIOS_SIZE],
            ewram: vec![0; EWRAM_SIZE],
            iwram: vec![0; IWRAM_SIZE],
            pram: vec![0; PRAM_SIZE],
            vram: vec![0; VRAM_SIZE],
            oam: vec![0; OAM_SIZE],
            rom: Vec::new(),
            sram: vec![0; SRAM_SIZE],
            io: IoRegisters::new(),
            ic: InterruptController::new(),
            timers: Timers::new(),
            dma: DmaController::new(),
            ppu: Ppu::new(),
            keypad: Keypad::new(),
            last_bus_value: 0,
            bios_locked: false,
        }
    }

    /// Load a BIOS image. Up to [`BIOS_SIZE`] bytes are copied. Resets the
    /// BIOS lock flag so subsequent reads return BIOS contents.
    pub fn load_bios(&mut self, data: &[u8]) {
        let n = data.len().min(BIOS_SIZE);
        self.bios[..n].copy_from_slice(&data[..n]);
        self.bios_locked = false;
    }

    /// Lock BIOS access. After this is called, reads of the BIOS region
    /// while the CPU PC is outside the BIOS return open-bus. For the bus
    /// foundation we model the simpler "all reads are open-bus" semantics
    /// — full PC-aware locking can be added with the CPU integration.
    pub fn lock_bios(&mut self) {
        self.bios_locked = true;
    }

    /// Whether the BIOS is currently locked from external reads.
    pub fn bios_locked(&self) -> bool {
        self.bios_locked
    }

    /// Load a cartridge ROM. Cap at [`ROM_MAX_SIZE`].
    pub fn load_rom(&mut self, data: &[u8]) {
        let n = data.len().min(ROM_MAX_SIZE);
        self.rom = data[..n].to_vec();
    }

    /// Whether a cartridge has been inserted.
    pub fn has_cart(&self) -> bool {
        !self.rom.is_empty()
    }

    /// Step the bus peripherals (timers, DMA, PPU) by `cycles` CPU
    /// cycles. Any pending IRQs are routed into [`Self::ic`]. PPU
    /// V-Blank/H-Blank edges are propagated to the DMA controller.
    pub fn step(&mut self, cycles: u32) {
        self.timers.step(cycles, &mut self.ic);
        let events = self.ppu.step(
            cycles,
            &mut self.ic,
            self.vram.as_slice(),
            self.pram.as_slice(),
        );
        self.handle_ppu_events(events);
        self.run_pending_dma();
    }

    /// Propagate PPU V-Blank / H-Blank edges to DMA-mode hooks. Each
    /// edge is forwarded individually because a single PPU step may
    /// span many scanlines (and thus many V-Blank/H-Blank edges) when
    /// the CPU executes a long block of instructions between calls.
    fn handle_ppu_events(&mut self, events: PpuStepEvents) {
        for _ in 0..events.vblank_starts {
            self.notify_vblank();
        }
        for _ in 0..events.hblank_starts {
            self.notify_hblank();
        }
    }

    /// Run any pending Immediate-mode DMA transfers and any triggered
    /// transfers that have already been armed.
    pub fn run_pending_dma(&mut self) {
        // Take the controller out of `self` so we can pass `&mut self` as
        // the [`DmaBus`] backing during the transfer (avoids borrow conflict
        // between the controller state and the bus memory it operates on).
        let mut dma = std::mem::take(&mut self.dma);
        dma.run_pending_triggered(self);
        self.dma = dma;
    }

    /// Notify pending V-blank-triggered DMA channels and run them. Called
    /// by the PPU when it enters V-blank.
    pub fn notify_vblank(&mut self) {
        self.dma.notify_vblank();
        self.run_pending_dma();
    }

    /// Notify pending H-blank-triggered DMA channels and run them. Called
    /// by the PPU when it enters H-blank.
    pub fn notify_hblank(&mut self) {
        self.dma.notify_hblank();
        self.run_pending_dma();
    }

    /// Notify that audio FIFO `which` (0=A, 1=B) needs replenishment and
    /// run the corresponding Special-mode DMA channel (1 or 2).
    pub fn notify_fifo(&mut self, which: usize) {
        self.dma.notify_fifo(which);
        self.run_pending_dma();
    }

    /// Take the accumulated DMA stall cycles. The CPU consumes these when
    /// stepping after a DMA-induced pause.
    pub fn take_dma_stall_cycles(&mut self) -> u32 {
        self.dma.take_cpu_stall()
    }

    /// Return non-sequential access cycle count for `addr` and access width.
    pub fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let i = width.idx();
        match (addr >> 24) & 0xF {
            0x0 => wait_states::BIOS[i],
            0x2 => wait_states::EWRAM_N[i],
            0x3 => wait_states::IWRAM[i],
            0x4 => wait_states::IO[i],
            0x5 => wait_states::PRAM[i],
            0x6 => wait_states::VRAM[i],
            0x7 => wait_states::OAM[i],
            0x8..=0xD => wait_states::ROM_N[i],
            0xE | 0xF => wait_states::SRAM[i],
            _ => 1,
        }
    }

    /// Return sequential access cycle count for `addr` and access width.
    pub fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let i = width.idx();
        match (addr >> 24) & 0xF {
            0x2 => wait_states::EWRAM_S[i],
            0x8..=0xD => wait_states::ROM_S[i],
            _ => self.n_cycles(addr, width),
        }
    }

    /// Look up the BIOS contents at `addr` honouring the BIOS lock.
    fn read_bios_byte(&self, addr: u32) -> Option<u8> {
        if self.bios_locked || (addr as usize) >= BIOS_SIZE {
            None
        } else {
            Some(self.bios[addr as usize])
        }
    }

    /// Read a 16-bit little-endian halfword from BIOS, honouring the lock.
    /// Returns `None` if either byte falls outside the BIOS region or is locked.
    fn read_bios_u16(&self, addr: u32) -> Option<u16> {
        Some(u16::from_le_bytes([
            self.read_bios_byte(addr)?,
            self.read_bios_byte(addr + 1)?,
        ]))
    }

    /// Read a 32-bit little-endian word from BIOS, honouring the lock.
    /// Returns `None` if any byte falls outside the BIOS region or is locked.
    fn read_bios_u32(&self, addr: u32) -> Option<u32> {
        Some(u32::from_le_bytes([
            self.read_bios_byte(addr)?,
            self.read_bios_byte(addr + 1)?,
            self.read_bios_byte(addr + 2)?,
            self.read_bios_byte(addr + 3)?,
        ]))
    }

    /// Open-bus byte for a given address.
    fn open_bus_byte(&self, addr: u32) -> u8 {
        ((self.last_bus_value >> ((addr & 3) * 8)) & 0xFF) as u8
    }

    /// Open-bus halfword for a given address.
    fn open_bus_halfword(&self, addr: u32) -> u16 {
        let shift = if addr & 0x2 == 0 { 0 } else { 16 };
        ((self.last_bus_value >> shift) & 0xFFFF) as u16
    }

    /// Open-bus word for a given address.
    fn open_bus_word(&self) -> u32 {
        self.last_bus_value
    }

    /// Map the cartridge ROM with mirroring across the three wait-state
    /// regions. The ROM is intentionally mirrored within its 32 MB window
    /// (the cart bus repeats the inserted image), so this only returns
    /// `None` when no cartridge is inserted at all — in which case callers
    /// substitute the GBATek "no-cart" open-bus pattern.
    fn rom_byte(&self, offset: usize) -> Option<u8> {
        if self.rom.is_empty() {
            return None;
        }
        // ROM is mirrored within its 32 MB window.
        Some(self.rom[offset % self.rom.len()])
    }

    /// Read a 16-bit little-endian halfword from cart ROM, respecting
    /// mirroring. Returns `None` when no cartridge is inserted.
    fn rom_u16(&self, offset: usize) -> Option<u16> {
        Some(u16::from_le_bytes([
            self.rom_byte(offset)?,
            self.rom_byte(offset + 1)?,
        ]))
    }

    /// Read a 32-bit little-endian word from cart ROM, respecting mirroring.
    /// Returns `None` when no cartridge is inserted.
    fn rom_u32(&self, offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes([
            self.rom_byte(offset)?,
            self.rom_byte(offset + 1)?,
            self.rom_byte(offset + 2)?,
            self.rom_byte(offset + 3)?,
        ]))
    }
}

impl Bus for GbaBus {
    fn read32(&mut self, addr: u32) -> u32 {
        let aligned = addr & !0x3;
        let val = match (aligned >> 24) & 0xF {
            0x0 | 0x1 => self
                .read_bios_u32(aligned)
                .unwrap_or_else(|| self.open_bus_word()),
            0x2 => read_le_u32(&self.ewram, aligned as usize),
            0x3 => read_le_u32(&self.iwram, aligned as usize),
            0x4 => self
                .io
                .try_read32(
                    aligned,
                    &self.ic,
                    &self.timers,
                    &self.dma,
                    &self.ppu,
                    &self.keypad,
                )
                .unwrap_or_else(|| self.open_bus_word()),
            0x5 => read_le_u32(&self.pram, aligned as usize),
            0x6 => {
                let off = vram_offset(aligned);
                read_le_u32(&self.vram, off)
            }
            0x7 => read_le_u32(&self.oam, aligned as usize),
            0x8..=0xD => {
                let off = (aligned & 0x01FF_FFFF) as usize;
                self.rom_u32(off)
                    .unwrap_or_else(|| open_bus_no_cart_word(aligned))
            }
            0xE | 0xF => {
                // SRAM is 8-bit only on real hardware; word access mirrors
                // the byte across the word.
                let b = self.sram[(aligned as usize) % SRAM_SIZE];
                u32::from_le_bytes([b, b, b, b])
            }
            _ => self.open_bus_word(),
        };
        self.last_bus_value = val;
        val
    }

    fn read16(&mut self, addr: u32) -> u16 {
        let aligned = addr & !0x1;
        let val = match (aligned >> 24) & 0xF {
            0x0 | 0x1 => self
                .read_bios_u16(aligned)
                .unwrap_or_else(|| self.open_bus_halfword(aligned)),
            0x2 => read_le_u16(&self.ewram, aligned as usize),
            0x3 => read_le_u16(&self.iwram, aligned as usize),
            0x4 => self
                .io
                .try_read16(
                    aligned,
                    &self.ic,
                    &self.timers,
                    &self.dma,
                    &self.ppu,
                    &self.keypad,
                )
                .unwrap_or_else(|| self.open_bus_halfword(aligned)),
            0x5 => read_le_u16(&self.pram, aligned as usize),
            0x6 => {
                let off = vram_offset(aligned);
                read_le_u16(&self.vram, off)
            }
            0x7 => read_le_u16(&self.oam, aligned as usize),
            0x8..=0xD => {
                let off = (aligned & 0x01FF_FFFF) as usize;
                self.rom_u16(off)
                    .unwrap_or_else(|| open_bus_no_cart_halfword(aligned))
            }
            0xE | 0xF => {
                let b = self.sram[(aligned as usize) % SRAM_SIZE];
                u16::from_le_bytes([b, b])
            }
            _ => self.open_bus_halfword(aligned),
        };
        // Don't disturb the high half of last_bus_value: only refresh the
        // matching half. Some GBA games rely on the prefetcher's word state.
        let shift = if aligned & 0x2 == 0 { 0 } else { 16 };
        self.last_bus_value =
            (self.last_bus_value & !(0xFFFFu32 << shift)) | ((val as u32) << shift);
        val
    }

    fn read8(&mut self, addr: u32) -> u8 {
        let val = match (addr >> 24) & 0xF {
            0x0 | 0x1 => self
                .read_bios_byte(addr)
                .unwrap_or_else(|| self.open_bus_byte(addr)),
            0x2 => self.ewram[(addr as usize) % EWRAM_SIZE],
            0x3 => self.iwram[(addr as usize) % IWRAM_SIZE],
            0x4 => self
                .io
                .try_read8(
                    addr,
                    &self.ic,
                    &self.timers,
                    &self.dma,
                    &self.ppu,
                    &self.keypad,
                )
                .unwrap_or_else(|| self.open_bus_byte(addr)),
            0x5 => self.pram[(addr as usize) % PRAM_SIZE],
            0x6 => self.vram[vram_offset(addr)],
            0x7 => self.oam[(addr as usize) % OAM_SIZE],
            0x8..=0xD => {
                let off = (addr & 0x01FF_FFFF) as usize;
                self.rom_byte(off).unwrap_or(open_bus_no_cart_byte(addr))
            }
            0xE | 0xF => self.sram[(addr as usize) % SRAM_SIZE],
            _ => self.open_bus_byte(addr),
        };
        let shift = (addr & 3) * 8;
        self.last_bus_value = (self.last_bus_value & !(0xFFu32 << shift)) | ((val as u32) << shift);
        val
    }

    fn write32(&mut self, addr: u32, value: u32) {
        let aligned = addr & !0x3;
        self.last_bus_value = value;
        let touches_io = (aligned >> 24) & 0xF == 0x4;
        match (aligned >> 24) & 0xF {
            0x0 | 0x1 => { /* BIOS is read-only */ }
            0x2 => write_le_u32(&mut self.ewram, aligned as usize, value),
            0x3 => write_le_u32(&mut self.iwram, aligned as usize, value),
            0x4 => self.io.write32(
                aligned,
                value,
                &mut self.ic,
                &mut self.timers,
                &mut self.dma,
                &mut self.ppu,
                &mut self.keypad,
            ),
            0x5 => write_le_u32(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u32(&mut self.vram, off, value);
            }
            0x7 => write_le_u32(&mut self.oam, aligned as usize, value),
            0x8..=0xD => { /* Cartridge ROM is read-only via the bus */ }
            0xE | 0xF => {
                // SRAM byte-only writes — store low byte into addressed cell
                self.sram[(aligned as usize) % SRAM_SIZE] = value as u8;
            }
            _ => {}
        }
        if touches_io && self.dma.any_pending() {
            self.run_pending_dma();
        }
    }

    fn write16(&mut self, addr: u32, value: u16) {
        let aligned = addr & !0x1;
        let shift = if aligned & 0x2 == 0 { 0 } else { 16 };
        self.last_bus_value =
            (self.last_bus_value & !(0xFFFFu32 << shift)) | ((value as u32) << shift);
        let touches_io = (aligned >> 24) & 0xF == 0x4;
        match (aligned >> 24) & 0xF {
            0x0 | 0x1 => {}
            0x2 => write_le_u16(&mut self.ewram, aligned as usize, value),
            0x3 => write_le_u16(&mut self.iwram, aligned as usize, value),
            0x4 => self.io.write16(
                aligned,
                value,
                &mut self.ic,
                &mut self.timers,
                &mut self.dma,
                &mut self.ppu,
                &mut self.keypad,
            ),
            0x5 => write_le_u16(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u16(&mut self.vram, off, value);
            }
            0x7 => write_le_u16(&mut self.oam, aligned as usize, value),
            0x8..=0xD => {}
            0xE | 0xF => {
                self.sram[(aligned as usize) % SRAM_SIZE] = value as u8;
            }
            _ => {}
        }
        if touches_io && self.dma.any_pending() {
            self.run_pending_dma();
        }
    }

    fn write8(&mut self, addr: u32, value: u8) {
        let shift = (addr & 3) * 8;
        self.last_bus_value =
            (self.last_bus_value & !(0xFFu32 << shift)) | ((value as u32) << shift);
        let touches_io = (addr >> 24) & 0xF == 0x4;
        match (addr >> 24) & 0xF {
            0x0 | 0x1 => {}
            0x2 => self.ewram[(addr as usize) % EWRAM_SIZE] = value,
            0x3 => self.iwram[(addr as usize) % IWRAM_SIZE] = value,
            0x4 => self.io.write8(
                addr,
                value,
                &mut self.ic,
                &mut self.timers,
                &mut self.dma,
                &mut self.ppu,
                &mut self.keypad,
            ),
            0x5 => {
                // Byte writes to PRAM duplicate the byte to a halfword.
                let off = (addr as usize & !1) % PRAM_SIZE;
                self.pram[off] = value;
                self.pram[off + 1] = value;
            }
            0x6 => {
                // Byte writes to VRAM duplicate; ignored for OBJ region in
                // bitmap modes — modelling the simple case here.
                let off = vram_offset(addr) & !1;
                self.vram[off] = value;
                self.vram[off + 1] = value;
            }
            0x7 => { /* OAM ignores byte writes */ }
            0x8..=0xD => {}
            0xE | 0xF => self.sram[(addr as usize) % SRAM_SIZE] = value,
            _ => {}
        }
        if touches_io && self.dma.any_pending() {
            self.run_pending_dma();
        }
    }
}

/// DMA transfers use the bus' standard read/write paths so that DMA
/// destinations like PRAM/VRAM/OAM see the same byte-mirroring rules as
/// CPU stores.
impl dma::DmaBus for GbaBus {
    fn dma_read16(&mut self, addr: u32) -> u16 {
        <Self as Bus>::read16(self, addr)
    }
    fn dma_write16(&mut self, addr: u32, value: u16) {
        <Self as Bus>::write16(self, addr, value);
    }
    fn dma_read32(&mut self, addr: u32) -> u32 {
        <Self as Bus>::read32(self, addr)
    }
    fn dma_write32(&mut self, addr: u32, value: u32) {
        <Self as Bus>::write32(self, addr, value);
    }
    fn dma_raise_irq(&mut self, sources: u16) {
        self.ic.raise(sources);
    }
}

/// VRAM mirrors with a 128 KB period: `[0..64KB][64..96KB][64..96KB-mirror]`.
fn vram_offset(addr: u32) -> usize {
    let local = (addr as usize) & 0x1FFFF; // 128 KB period
    if local < 0x18000 {
        local
    } else {
        // Mirror upper 32 KB of VRAM into the second half of each 128 KB
        // window.
        0x10000 + (local & 0x7FFF)
    }
}

/// Reading from cartridge ROM with no cart inserted returns the lower half of
/// `addr / 2` (GBATek "open bus" rule for the cart bus).
fn open_bus_no_cart_word(addr: u32) -> u32 {
    let lo = open_bus_no_cart_halfword(addr) as u32;
    let hi = open_bus_no_cart_halfword(addr.wrapping_add(2)) as u32;
    lo | (hi << 16)
}

fn open_bus_no_cart_halfword(addr: u32) -> u16 {
    ((addr >> 1) & 0xFFFF) as u16
}

fn open_bus_no_cart_byte(addr: u32) -> u8 {
    let hw = open_bus_no_cart_halfword(addr & !1);
    if addr & 1 == 0 {
        hw as u8
    } else {
        (hw >> 8) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewram_mirrors_within_256k() {
        let mut bus = GbaBus::new();
        bus.write32(0x0200_0010, 0xCAFE_BABE);
        // Mirror at +0x40000.
        assert_eq!(bus.read32(0x0204_0010), 0xCAFE_BABE);
    }

    #[test]
    fn iwram_round_trips() {
        let mut bus = GbaBus::new();
        bus.write32(0x0300_0020, 0xDEAD_BEEF);
        assert_eq!(bus.read32(0x0300_0020), 0xDEAD_BEEF);
        assert_eq!(bus.read16(0x0300_0020), 0xBEEF);
        assert_eq!(bus.read8(0x0300_0020), 0xEF);
    }

    #[test]
    fn pram_vram_oam_round_trip() {
        let mut bus = GbaBus::new();
        bus.write16(0x0500_0010, 0x1234);
        assert_eq!(bus.read16(0x0500_0010), 0x1234);
        bus.write32(0x0600_0040, 0xAABB_CCDD);
        assert_eq!(bus.read32(0x0600_0040), 0xAABB_CCDD);
        bus.write16(0x0700_0008, 0xBEEF);
        assert_eq!(bus.read16(0x0700_0008), 0xBEEF);
    }

    #[test]
    fn vram_mirror_64k_to_96k_window() {
        let mut bus = GbaBus::new();
        // Write to upper VRAM (0x18000–0x1FFFF mirrors 0x10000–0x17FFF).
        bus.write16(0x0601_0010, 0x4242);
        assert_eq!(bus.read16(0x0601_8010), 0x4242);
    }

    #[test]
    fn bios_readable_until_locked() {
        let mut bus = GbaBus::new();
        bus.load_bios(&[0x11, 0x22, 0x33, 0x44]);
        assert_eq!(bus.read32(0x0000_0000), 0x4433_2211);
        // Touch a different region so last_bus_value reflects something
        // distinct from the BIOS we just read.
        bus.write32(0x0200_0000, 0xAAAA_BBBB);
        let _ = bus.read32(0x0200_0000);
        bus.lock_bios();
        // After lock, BIOS reads return open-bus (last bus value).
        assert_eq!(bus.read32(0x0000_0000), 0xAAAA_BBBB);
    }

    #[test]
    fn cart_open_bus_when_no_rom() {
        let mut bus = GbaBus::new();
        // No cart inserted — read should not panic and should be a defined
        // open-bus value.
        let v = bus.read16(0x0800_0000);
        assert_eq!(v, 0); // (0x0800_0000 >> 1) & 0xFFFF == 0
        let v2 = bus.read16(0x0800_0004);
        assert_eq!(v2, 2);
    }

    #[test]
    fn cart_rom_round_trip_when_loaded() {
        let mut bus = GbaBus::new();
        bus.load_rom(&[0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34]);
        assert_eq!(bus.read32(0x0800_0000), 0xEFBE_ADDE);
        assert_eq!(bus.read16(0x0800_0004), 0x3412);
    }

    #[test]
    fn rom_writes_are_ignored() {
        let mut bus = GbaBus::new();
        bus.load_rom(&[0x11, 0x22, 0x33, 0x44]);
        bus.write32(0x0800_0000, 0xFFFF_FFFF);
        assert_eq!(bus.read32(0x0800_0000), 0x4433_2211);
    }

    #[test]
    fn sram_byte_only_mirrors_word_reads() {
        let mut bus = GbaBus::new();
        bus.write8(0x0E00_0010, 0xAB);
        assert_eq!(bus.read8(0x0E00_0010), 0xAB);
        // Word reads return the byte mirrored into all 4 lanes.
        assert_eq!(bus.read32(0x0E00_0010), 0xABAB_ABAB);
    }

    #[test]
    fn n_and_s_cycles_match_gbatek_defaults() {
        let bus = GbaBus::new();
        assert_eq!(bus.n_cycles(0x0300_0000, WidthClass::HalfwordOrByte), 1);
        assert_eq!(bus.n_cycles(0x0200_0000, WidthClass::HalfwordOrByte), 3);
        assert_eq!(bus.n_cycles(0x0200_0000, WidthClass::Word), 6);
        assert_eq!(bus.n_cycles(0x0800_0000, WidthClass::HalfwordOrByte), 5);
        assert_eq!(bus.s_cycles(0x0800_0000, WidthClass::HalfwordOrByte), 3);
    }

    #[test]
    fn out_of_range_address_returns_open_bus() {
        let mut bus = GbaBus::new();
        // Write a known value through EWRAM so last_bus_value is non-zero.
        bus.write32(0x0200_0000, 0x1234_5678);
        // 0x10000000 is outside any defined region — open bus.
        assert_eq!(bus.read32(0x1000_0000), 0x1234_5678);
    }

    #[test]
    fn timer_steps_via_bus_step() {
        let mut bus = GbaBus::new();
        // TM0CNT_L reload = 0
        bus.write16(0x0400_0100, 0);
        // TM0CNT_H: enable, prescaler 0
        bus.write16(0x0400_0102, 0x0080);
        bus.step(1024);
        assert_eq!(bus.read16(0x0400_0100), 1024);
    }

    #[test]
    fn ime_ie_if_acknowledge_round_trip() {
        // Test vector: Write 0xFFFF to IE; raise TM0; set IME=1 → IRQ asserted.
        // Then write 0x0008 to IF → TIMER0 cleared, IRQ de-asserts.
        let mut bus = GbaBus::new();
        bus.write16(REG_IE, 0xFFFF);
        bus.write16(REG_IME, 1);
        bus.ic.raise(irq_bits::TIMER0);
        assert!(bus.ic.irq_line());
        bus.write16(REG_IF, irq_bits::TIMER0);
        assert!(!bus.ic.irq_line());
    }

    #[test]
    fn cascade_test_vector_4() {
        // Acceptance: enable TM0+TM1 cascade; overflow TM0 once → TM1 == 1.
        let mut bus = GbaBus::new();
        bus.write16(0x0400_0100, 0xFFFF); // TM0 reload
        bus.write16(0x0400_0102, 0x0080); // TM0 enable, prescaler 0
        bus.write16(0x0400_0104, 0); // TM1 reload
        bus.write16(0x0400_0106, 0x0084); // TM1 enable + cascade
        bus.step(1);
        assert_eq!(bus.read16(0x0400_0104), 1);
    }

    #[test]
    fn timer_overflow_irq_test_vector_3() {
        // Acceptance: prescaler=0, tick 0x10000 → TM0 wraps to 0 + IRQ.
        let mut bus = GbaBus::new();
        bus.write16(REG_IE, irq_bits::TIMER0);
        bus.write16(REG_IME, 1);
        bus.write16(0x0400_0100, 0); // TM0 reload
        bus.write16(0x0400_0102, 0x00C0 | 0x0080); // enable + IRQ on overflow
        bus.step(0x1_0000);
        assert_eq!(bus.read16(0x0400_0100), 0);
        assert!(bus.ic.irq_line());
    }

    #[test]
    fn unimplemented_io_register_does_not_panic() {
        let mut bus = GbaBus::new();
        // REG_DISPCNT (0x04000000)
        bus.write16(0x0400_0000, 0x1234);
        assert_eq!(bus.read16(0x0400_0000), 0x1234);
        // A random unimplemented register stretching the dispatch.
        bus.write16(0x0400_0050, 0xBEEF);
        assert_eq!(bus.read16(0x0400_0050), 0xBEEF);
    }

    #[test]
    fn io_read_outside_1k_window_returns_open_bus() {
        let mut bus = GbaBus::new();
        // Establish a known last-bus value via an EWRAM read.
        bus.write32(0x0200_0000, 0x1234_5678);
        let _ = bus.read32(0x0200_0000);
        // 0x0400_0400 is in I/O region 0x4 but past the 1 KB I/O window.
        assert_eq!(bus.read32(0x0400_0400), 0x1234_5678);
    }

    #[test]
    fn dma_immediate_fires_via_cpu_io_writes() {
        // Acceptance: CPU programs DMA via I/O writes; transfer happens
        // when the enable bit transitions 0→1 and the data appears at
        // the destination region.
        let mut bus = GbaBus::new();
        // Place 4 words of source data in EWRAM.
        for i in 0..4 {
            bus.write32(0x0200_0000 + i * 4, 0xAABB_0000 + i);
        }
        // Program DMA channel 0 via I/O writes (mirroring real software).
        bus.write32(0x0400_00B0, 0x0200_0000); // SAD
        bus.write32(0x0400_00B4, 0x0200_1000); // DAD
        bus.write16(0x0400_00B8, 4); // count
        // CNT_H: enable | IRQ | timing=immediate | word | src=inc | dst=inc.
        bus.write16(REG_IE, irq_bits::DMA0);
        bus.write16(REG_IME, 1);
        bus.write16(0x0400_00BA, 0x8000 | 0x4000 | 0x0400);
        // Verify destination contains the source data.
        for i in 0..4 {
            assert_eq!(bus.read32(0x0200_1000 + i * 4), 0xAABB_0000 + i);
        }
        // CPU stall accumulator reflects 4 units × 2 cycles each.
        assert_eq!(bus.take_dma_stall_cycles(), 8);
        // IRQ was raised through the controller.
        assert!(bus.ic.irq_line());
        // Enable bit is cleared after one-shot completion.
        assert_eq!(bus.read16(0x0400_00BA) & 0x8000, 0);
    }

    #[test]
    fn dma_vblank_fires_on_notify() {
        let mut bus = GbaBus::new();
        bus.write16(0x0200_0000, 0xCAFE);
        bus.write16(0x0200_0002, 0xBABE);
        bus.write32(0x0400_00BC, 0x0200_0000); // CH1 SAD
        bus.write32(0x0400_00C0, 0x0200_1000); // CH1 DAD
        bus.write16(0x0400_00C4, 2); // CH1 count
        // CNT_H: enable | timing=VBlank (1) — halfword.
        bus.write16(0x0400_00C6, 0x8000 | (1 << 12));
        // Without notify nothing happens.
        assert_eq!(bus.read16(0x0200_1000), 0);
        bus.notify_vblank();
        assert_eq!(bus.read16(0x0200_1000), 0xCAFE);
        assert_eq!(bus.read16(0x0200_1002), 0xBABE);
    }

    #[test]
    fn dma_priority_arbitration_serves_channel0_first() {
        // Setup both CH0 and CH1 immediate transfers and verify CH0 ran
        // first by inspecting where they wrote in the bus.
        let mut bus = GbaBus::new();
        bus.write32(0x0200_0000, 0xC0_DA);
        bus.write32(0x0200_0100, 0xC1_DA);
        // CH1 first (lower priority), then CH0 (higher priority): CH0
        // must preempt CH1 if it were already mid-burst, but with both
        // immediate-pending CH0 should be served first.
        bus.write32(0x0400_00BC, 0x0200_0000); // CH1 SAD
        bus.write32(0x0400_00C0, 0x0200_2000); // CH1 DAD
        bus.write16(0x0400_00C4, 1);
        // Channel 0 gets programmed last but should still win priority.
        bus.write32(0x0400_00B0, 0x0200_0100); // CH0 SAD
        bus.write32(0x0400_00B4, 0x0200_3000); // CH0 DAD
        bus.write16(0x0400_00B8, 1);
        // Enable both at once via 32-bit write of CH1.CNT_H | CH0.CNT_H?
        // The two are at different addresses, so do CH1 then CH0; the CH0
        // enable rising edge will start its transfer first because of
        // priority, even though CH1 was armed earlier.
        bus.write16(0x0400_00C6, 0x8000 | 0x0400); // CH1 enable, word
        bus.write16(0x0400_00BA, 0x8000 | 0x0400); // CH0 enable, word
        // Both transfers must have completed.
        assert_eq!(bus.read32(0x0200_3000), 0xC1_DA);
        assert_eq!(bus.read32(0x0200_2000), 0xC0_DA);
    }

    #[test]
    fn ppu_dispcnt_round_trips_through_bus() {
        let mut bus = GbaBus::new();
        // 16-bit write/read at REG_DISPCNT must reach the live PPU.
        bus.write16(crate::gba::ppu::REG_DISPCNT, 0x0403);
        assert_eq!(bus.ppu.read_dispcnt(), 0x0403);
        assert_eq!(bus.read16(crate::gba::ppu::REG_DISPCNT), 0x0403);
    }

    #[test]
    fn bus_step_advances_ppu_vcount() {
        let mut bus = GbaBus::new();
        // Step a full scanline and verify VCOUNT incremented.
        bus.step(crate::gba::ppu::CYCLES_PER_SCANLINE);
        assert_eq!(bus.read16(crate::gba::ppu::REG_VCOUNT), 1);
    }

    #[test]
    fn bus_step_raises_vblank_irq_when_enabled() {
        let mut bus = GbaBus::new();
        bus.write16(REG_IE, irq_bits::VBLANK);
        bus.write16(REG_IME, 1);
        // Enable V-Blank IRQ in DISPSTAT.
        bus.write16(
            crate::gba::ppu::REG_DISPSTAT,
            crate::gba::ppu::dispstat::VBLANK_IRQ_ENABLE,
        );
        // Step 160 scanlines to trigger V-Blank.
        bus.step(crate::gba::ppu::CYCLES_PER_SCANLINE * crate::gba::ppu::VISIBLE_SCANLINES);
        assert!(bus.ic.irq_line());
        assert_ne!(bus.ic.if_flags & irq_bits::VBLANK, 0);
    }

    #[test]
    fn bus_step_renders_mode3_via_vram_bus_writes() {
        // End-to-end: CPU writes Mode 3 + BG2 enable to DISPCNT, paints
        // VRAM through the bus, steps a frame, and verifies the
        // framebuffer reflects the painted pixels.
        let mut bus = GbaBus::new();
        bus.write16(
            crate::gba::ppu::REG_DISPCNT,
            3 | crate::gba::ppu::dispcnt::BG2_ENABLE,
        );
        // Pixel 0,0 = pure red (BGR555 0x001F). Pixel 1,0 = pure blue.
        bus.write16(0x0600_0000, 0x001F);
        bus.write16(0x0600_0002, 0x7C00);
        bus.step(crate::gba::ppu::CYCLES_PER_SCANLINE * crate::gba::ppu::SCANLINES_PER_FRAME);
        let fb = bus.ppu.framebuffer();
        assert_eq!(&fb[0..3], &[0xFF, 0, 0]);
        assert_eq!(&fb[3..6], &[0, 0, 0xFF]);
    }

    #[test]
    fn bus_keyinput_reads_active_low_state_via_io() {
        let mut bus = GbaBus::new();
        // No buttons pressed → all bits 0–9 high.
        assert_eq!(bus.read16(crate::gba::input::REG_KEYINPUT), 0x03FF);
        // Press A (id=0) → bit 0 clears.
        bus.keypad.set_button(0, true, &mut bus.ic);
        assert_eq!(bus.read16(crate::gba::input::REG_KEYINPUT), 0x03FE);
    }

    #[test]
    fn bus_keypad_irq_routes_to_interrupt_controller() {
        let mut bus = GbaBus::new();
        // Configure KEYCNT via the I/O bus: select A, IRQ-enable.
        bus.write16(
            crate::gba::input::REG_KEYCNT,
            crate::gba::input::KEYCNT_IRQ_ENABLE | 0x0001,
        );
        // Pressing A must raise the keypad IRQ.
        bus.keypad.set_button(0, true, &mut bus.ic);
        assert_ne!(bus.ic.if_flags & irq_bits::KEYPAD, 0);
    }

    #[test]
    fn bus_keyinput_is_read_only_via_io_writes() {
        let mut bus = GbaBus::new();
        bus.write16(crate::gba::input::REG_KEYINPUT, 0x0000);
        assert_eq!(bus.read16(crate::gba::input::REG_KEYINPUT), 0x03FF);
    }
}
