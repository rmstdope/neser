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
pub mod sio;
pub mod timer;

use crate::gba::apu::Apu;
use crate::gba::cartridge::SaveBackend;
use crate::gba::console::config::GbaTraceConfig;
use crate::gba::cpu::bus::Bus;
use crate::gba::input::Keypad;
use crate::gba::ppu::{Ppu, PpuStepEvents};

#[allow(unused_imports)]
pub use dma::{DmaBus, DmaChannel, DmaController};
#[allow(unused_imports)]
pub use interrupt::{InterruptController, bits as irq_bits};
#[allow(unused_imports)]
pub use io::{IoRegisters, REG_IE, REG_IF, REG_IME};
#[allow(unused_imports)]
pub use timer::{Timer, Timers};

use memory::{
    BIOS_SIZE, EWRAM_SIZE, IWRAM_SIZE, OAM_SIZE, PRAM_SIZE, ROM_MAX_SIZE, SRAM_SIZE, VRAM_SIZE,
    read_le_u16, read_le_u32, write_le_u16, write_le_u32,
};

#[cfg(test)]
thread_local! {
    static GBA_BUS_TRACE_LINES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Pre-computed wait-state cycle counts for each memory region.
///
/// Each region stores `[N16, N32, S16, S32]` — non-sequential and sequential
/// access times for 16-bit and 32-bit widths. Updated when WAITCNT is written.
///
/// Values match mGBA/GBATek conventions (no +1 adjustment).
#[derive(Debug, Clone)]
pub struct Waitstates {
    /// Per-region cycle lookup: indexed by `(addr >> 24) & 0xF`, then by
    /// `[N16, N32, S16, S32]`.
    regions: [[u32; 4]; 16],
    /// Raw WAITCNT register value (writable bits 0-14).
    pub waitcnt: u16,
    /// Whether the prefetch buffer is enabled (bit 14 of WAITCNT).
    pub prefetch_enabled: bool,
}

/// Indices into the per-region `[N16, N32, S16, S32]` array.
const N16: usize = 0;
const N32: usize = 1;
const S16: usize = 2;
const S32: usize = 3;

/// LUT for SRAM and ROM non-sequential wait states (2-bit index → cycles).
const WAIT_N_LUT: [u32; 4] = [4, 3, 2, 8];

/// LUT for ROM WS0 sequential access (1-bit index → cycles).
const WAIT_S0_LUT: [u32; 2] = [2, 1];
/// LUT for ROM WS1 sequential access (1-bit index → cycles).
const WAIT_S1_LUT: [u32; 2] = [4, 1];
/// LUT for ROM WS2 sequential access (1-bit index → cycles).
const WAIT_S2_LUT: [u32; 2] = [8, 1];

const MGBA_DEBUG_STRING: u32 = 0x04FF_F600;
const MGBA_DEBUG_FLAGS: u32 = 0x04FF_F700;
const MGBA_DEBUG_ENABLE: u32 = 0x04FF_F780;
const MGBA_DEBUG_STRING_LEN: usize = 0x100;
const MGBA_DEBUG_ENABLE_VALUE: u16 = 0xC0DE;
const MGBA_DEBUG_OPEN_VALUE: u16 = 0x1DEA;
const MGBA_DEBUG_SEND_FLAG: u16 = 0x0100;

impl Default for Waitstates {
    fn default() -> Self {
        Self::new()
    }
}

impl Waitstates {
    /// Create with power-on defaults (WAITCNT = 0x0000).
    pub fn new() -> Self {
        let mut ws = Self {
            regions: [[1; 4]; 16],
            waitcnt: 0,
            prefetch_enabled: false,
        };
        ws.recalculate(0);
        ws
    }

    /// Recalculate all region timings from a new WAITCNT value.
    pub fn recalculate(&mut self, waitcnt: u16) {
        self.waitcnt = waitcnt & 0x5FFF; // mask unused bits 13, 15
        self.prefetch_enabled = waitcnt & (1 << 14) != 0;

        // Fixed regions (not affected by WAITCNT)
        let bios = [1, 1, 1, 1];
        let ewram = [3, 6, 3, 6];
        let iwram = [1, 1, 1, 1];
        let io = [1, 1, 1, 1];
        let pram = [1, 2, 1, 2];
        let vram = [1, 2, 1, 2];
        let oam = [1, 1, 1, 1];

        self.regions[0x0] = bios;
        self.regions[0x1] = bios; // mirror / unused
        self.regions[0x2] = ewram;
        self.regions[0x3] = iwram;
        self.regions[0x4] = io;
        self.regions[0x5] = pram;
        self.regions[0x6] = vram;
        self.regions[0x7] = oam;

        // SRAM wait (bits 0-1)
        let sram_n16 = WAIT_N_LUT[(waitcnt & 0x3) as usize];
        // SRAM is always non-sequential (8-bit bus), 32-bit = 2×N16 + 1
        let sram_n32 = 2 * sram_n16 + 1;
        let sram = [sram_n16, sram_n32, sram_n16, sram_n32];
        self.regions[0xE] = sram;
        self.regions[0xF] = sram;

        // WS0 (bits 2-4): regions 0x8, 0x9
        let ws0_n16 = WAIT_N_LUT[((waitcnt >> 2) & 0x3) as usize];
        let ws0_s16 = WAIT_S0_LUT[((waitcnt >> 4) & 0x1) as usize];
        let ws0_n32 = ws0_n16 + 1 + ws0_s16;
        let ws0_s32 = 2 * ws0_s16 + 1;
        let ws0 = [ws0_n16, ws0_n32, ws0_s16, ws0_s32];
        self.regions[0x8] = ws0;
        self.regions[0x9] = ws0;

        // WS1 (bits 5-7): regions 0xA, 0xB
        let ws1_n16 = WAIT_N_LUT[((waitcnt >> 5) & 0x3) as usize];
        let ws1_s16 = WAIT_S1_LUT[((waitcnt >> 7) & 0x1) as usize];
        let ws1_n32 = ws1_n16 + 1 + ws1_s16;
        let ws1_s32 = 2 * ws1_s16 + 1;
        let ws1 = [ws1_n16, ws1_n32, ws1_s16, ws1_s32];
        self.regions[0xA] = ws1;
        self.regions[0xB] = ws1;

        // WS2 (bits 8-10): regions 0xC, 0xD
        let ws2_n16 = WAIT_N_LUT[((waitcnt >> 8) & 0x3) as usize];
        let ws2_s16 = WAIT_S2_LUT[((waitcnt >> 10) & 0x1) as usize];
        let ws2_n32 = ws2_n16 + 1 + ws2_s16;
        let ws2_s32 = 2 * ws2_s16 + 1;
        let ws2 = [ws2_n16, ws2_n32, ws2_s16, ws2_s32];
        self.regions[0xC] = ws2;
        self.regions[0xD] = ws2;
    }

    /// Non-sequential cycle count for a given address and width.
    #[inline]
    pub fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let region = ((addr >> 24) & 0xF) as usize;
        match width {
            WidthClass::HalfwordOrByte => self.regions[region][N16],
            WidthClass::Word => self.regions[region][N32],
        }
    }

    /// Sequential cycle count for a given address and width.
    #[inline]
    pub fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        let region = ((addr >> 24) & 0xF) as usize;
        match width {
            WidthClass::HalfwordOrByte => self.regions[region][S16],
            WidthClass::Word => self.regions[region][S32],
        }
    }
}

/// Access width for [`GbaBus::n_cycles`] / [`GbaBus::s_cycles`] cycle stubs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    /// 8-bit or 16-bit access.
    HalfwordOrByte,
    /// 32-bit access (counts as two halfword accesses on 16-bit buses).
    Word,
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
    /// Active cartridge save backend mapped at `0x0E00_0000`.
    cart_save: SaveBackend,
    /// Legacy byte mirror of the cart-RAM window used by save-state memory capture.
    sram: Vec<u8>,
    /// I/O register storage and dispatch.
    pub io: IoRegisters,
    /// Interrupt controller.
    pub ic: InterruptController,
    /// Timer bank (TM0-TM3).
    pub timers: Timers,
    /// DMA controller (DMA0-DMA3).
    pub dma: DmaController,
    /// Serial I/O controller.
    pub sio: sio::Sio,
    /// Picture Processing Unit (PPU).
    pub ppu: Ppu,
    /// Audio Processing Unit (APU).
    pub apu: Apu,
    /// Keypad (KEYINPUT / KEYCNT, key IRQ).
    pub keypad: Keypad,
    /// Last value driven on the bus (used to model open-bus reads).
    last_bus_value: u32,
    /// DMA internal data latch — separate from the CPU's open-bus register.
    /// Updated only by DMA reads; DMA writes to restricted regions return
    /// this value instead of the CPU's `last_bus_value`.
    dma_latch: u32,
    /// GBA-specific trace channel configuration.
    trace_config: GbaTraceConfig,
    /// mGBA debug console string buffer (`0x04FFF600`-`0x04FFF6FF`).
    mgba_debug_string: [u8; MGBA_DEBUG_STRING_LEN],
    /// Captured mGBA debug console output emitted via `0x04FFF700`.
    mgba_log: String,
    /// Whether the mGBA debug console has been enabled by writing `0xC0DE`.
    mgba_debug_enabled: bool,
    /// Whether the BIOS is locked. After the boot ROM finishes executing,
    /// BIOS reads from outside the BIOS region return open-bus instead of
    /// the BIOS contents.
    bios_locked: bool,
    /// Whether a full BIOS image has been loaded into the bus.
    bios_image_loaded: bool,
    /// Dynamic wait-state timing, recalculated on WAITCNT writes.
    waitstates: Waitstates,
    /// Undocumented register at 0x04000410, written by the real GBA BIOS
    /// during boot (value 0xFF). Stored separately because it falls outside
    /// the standard 1 KB I/O window.
    undoc_0x410: u8,
    /// HALTCNT write pending — set when software writes 0x04000301 with
    /// bit 7 clear (halt mode). The owning `Gba` polls this each tick and
    /// calls `cpu.halt()`.
    halt_requested: bool,
}

impl Default for GbaBus {
    fn default() -> Self {
        Self::new()
    }
}

fn check_region_size(region: &[u8], expected: usize, name: &str) -> Result<(), String> {
    if region.len() != expected {
        return Err(format!(
            "{name} size mismatch (expected {expected}, found {})",
            region.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
fn clear_gba_bus_trace_lines_for_tests() {
    GBA_BUS_TRACE_LINES.with(|lines| lines.borrow_mut().clear());
}

#[cfg(test)]
fn take_gba_bus_trace_lines_for_tests() -> Vec<String> {
    GBA_BUS_TRACE_LINES.with(|lines| lines.borrow_mut().drain(..).collect())
}

#[cfg(test)]
fn emit_gba_bus_trace_line(line: String) {
    GBA_BUS_TRACE_LINES.with(|lines| lines.borrow_mut().push(line));
}

#[cfg(not(test))]
fn emit_gba_bus_trace_line(line: String) {
    println!("{line}");
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
            cart_save: SaveBackend::None,
            // Cartridge-backed save media powers up in erased state.
            sram: vec![0xFF; SRAM_SIZE],
            io: IoRegisters::new(),
            ic: InterruptController::new(),
            timers: Timers::new(),
            dma: DmaController::new(),
            sio: sio::Sio::new(),
            ppu: Ppu::new(),
            apu: Apu::new(),
            keypad: Keypad::new(),
            last_bus_value: 0,
            dma_latch: 0,
            trace_config: GbaTraceConfig::default(),
            mgba_debug_string: [0; MGBA_DEBUG_STRING_LEN],
            mgba_log: String::new(),
            mgba_debug_enabled: false,
            bios_locked: false,
            bios_image_loaded: false,
            waitstates: Waitstates::new(),
            undoc_0x410: 0,
            halt_requested: false,
        }
    }

    /// Load a BIOS image. Up to [`BIOS_SIZE`] bytes are copied. Resets the
    /// BIOS lock flag so subsequent reads return BIOS contents.
    pub fn load_bios(&mut self, data: &[u8]) {
        let n = data.len().min(BIOS_SIZE);
        self.bios[..n].copy_from_slice(&data[..n]);
        self.bios_locked = false;
        self.bios_image_loaded = n == BIOS_SIZE;
    }

    /// Whether a full BIOS image is currently loaded.
    pub fn has_bios_image(&self) -> bool {
        self.bios_image_loaded
    }

    /// Configure GBA trace channels for bus-owned diagnostics.
    pub fn set_trace_config(&mut self, tracing: GbaTraceConfig) {
        self.trace_config = tracing;
    }

    /// Return a read-only snapshot of the cartridge SRAM mirror.
    pub fn sram_snapshot(&self) -> &[u8] {
        &self.sram
    }

    /// Return captured mGBA debug console output.
    pub fn mgba_log_snapshot(&self) -> &str {
        &self.mgba_log
    }

    #[cfg(test)]
    pub(crate) fn trace_config_for_tests(&self) -> GbaTraceConfig {
        self.trace_config
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

    /// Whether a HALTCNT write has requested the CPU enter halt state.
    pub fn halt_requested(&self) -> bool {
        self.halt_requested
    }

    /// Consume the pending halt request (called after `cpu.halt()`).
    pub fn clear_halt_request(&mut self) {
        self.halt_requested = false;
    }

    /// Debug: read a byte from the BIOS region at the given offset (no side effects).
    #[cfg(test)]
    pub fn debug_read_bios(&self, offset: usize) -> u8 {
        self.bios[offset % self.bios.len()]
    }

    /// Debug: read a byte from VRAM at the given offset (no side effects).
    #[cfg(test)]
    pub fn debug_read_vram(&self, offset: usize) -> u8 {
        self.vram[offset % self.vram.len()]
    }

    /// Load a cartridge ROM. Cap at [`ROM_MAX_SIZE`].
    pub fn load_rom(&mut self, data: &[u8]) {
        let n = data.len().min(ROM_MAX_SIZE);
        self.rom = data[..n].to_vec();
        self.cart_save = SaveBackend::None;
        self.sram.fill(0xFF);
    }

    /// Load a cartridge ROM and install its detected save backend.
    pub fn load_rom_with_save(&mut self, data: &[u8], save: SaveBackend) {
        let n = data.len().min(ROM_MAX_SIZE);
        self.rom = data[..n].to_vec();
        self.cart_save = save;

        // Keep the legacy SRAM mirror in sync for save-state memory snapshots.
        self.sram.fill(0xFF);
        if let SaveBackend::Sram(sram) = &self.cart_save {
            let snap = sram.snapshot();
            let n = snap.len().min(self.sram.len());
            self.sram[..n].copy_from_slice(&snap[..n]);
        }
    }

    /// Whether a cartridge has been inserted.
    pub fn has_cart(&self) -> bool {
        !self.rom.is_empty()
    }

    fn cart_read8(&self, addr: u32) -> u8 {
        let off = addr as usize;
        match &self.cart_save {
            SaveBackend::None => self.sram[off % SRAM_SIZE],
            SaveBackend::Eeprom(_) => 0xFF,
            SaveBackend::Sram(sram) => sram.read(off),
            SaveBackend::Flash(flash) => flash.read(off),
        }
    }

    fn cart_write8(&mut self, addr: u32, value: u8) {
        let off = addr as usize;
        match &mut self.cart_save {
            SaveBackend::None | SaveBackend::Eeprom(_) => {
                self.sram[off % SRAM_SIZE] = value;
            }
            SaveBackend::Sram(sram) => {
                sram.write(off, value);
                self.sram[off % SRAM_SIZE] = value;
            }
            SaveBackend::Flash(flash) => {
                flash.write(off, value);
                self.sram[off % SRAM_SIZE] = flash.read(off);
            }
        }
    }

    /// Step the bus peripherals (timers, DMA, PPU, APU) by `cycles` CPU
    /// cycles. Any pending IRQs are routed into [`Self::ic`]. PPU
    /// V-Blank/H-Blank edges are propagated to the DMA controller.
    pub fn step(&mut self, cycles: u32) {
        let overflow_mask = self.timers.step(cycles, &mut self.ic);
        self.handle_timer_overflow_fifo(overflow_mask);
        self.sio.step(cycles, &mut self.ic);
        self.apu.tick(cycles);
        let events = self.ppu.step(
            cycles,
            &mut self.ic,
            self.vram.as_slice(),
            self.pram.as_slice(),
            self.oam.as_slice(),
        );
        self.handle_ppu_events(events);
        self.run_pending_dma();
        self.step_dma_stalls();
    }

    fn step_dma_stalls(&mut self) {
        loop {
            let cycles = self.take_dma_stall_cycles();
            if cycles == 0 {
                break;
            }
            let overflow_mask = self.timers.step(cycles, &mut self.ic);
            self.handle_timer_overflow_fifo(overflow_mask);
            self.apu.tick(cycles);
            let events = self.ppu.step(
                cycles,
                &mut self.ic,
                self.vram.as_slice(),
                self.pram.as_slice(),
                self.oam.as_slice(),
            );
            self.handle_ppu_events(events);
            self.run_pending_dma();
        }
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

    /// When a timer overflows, advance the corresponding FIFO sample and
    /// request DMA replenishment if the FIFO is running low.
    ///
    /// Per GBATek: SOUNDCNT_H bit 10 selects the timer for FIFO A (0=TM0, 1=TM1),
    /// and bit 14 selects the timer for FIFO B.
    ///
    /// `overflow_counts` is `[TM0, TM1, TM2, TM3]` overflow counts returned by
    /// `Timers::step()`. Each FIFO advances once per overflow of its selected timer.
    fn handle_timer_overflow_fifo(&mut self, overflow_counts: [u32; 4]) {
        let soundcnt_h = self.apu.soundcnt_h;
        let fifo_a_timer: usize = if soundcnt_h & 0x0400 != 0 { 1 } else { 0 };
        let fifo_b_timer: usize = if soundcnt_h & 0x4000 != 0 { 1 } else { 0 };

        let a_overflows = overflow_counts[fifo_a_timer];
        if a_overflows > 0 {
            for _ in 0..a_overflows {
                self.apu.fifo_a.advance();
            }
            if self.apu.fifo_a.len() <= 16 {
                self.dma.notify_fifo(0);
            }
        }

        let b_overflows = overflow_counts[fifo_b_timer];
        if b_overflows > 0 {
            for _ in 0..b_overflows {
                self.apu.fifo_b.advance();
            }
            if self.apu.fifo_b.len() <= 16 {
                self.dma.notify_fifo(1);
            }
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

    /// Capture the bus memory regions and a few simple scalar fields
    /// for save-state serialization.
    ///
    /// This is intentionally limited to the parts of bus state that are
    /// already trivially serializable (raw byte regions plus the BIOS
    /// lock flag and last-bus-value).  Subsystem state owned by the bus
    /// (PPU, APU, IO, IC, timers, DMA, keypad) will be added as those
    /// modules grow `Serialize`/`Deserialize` support.
    ///
    /// The BIOS image is **not** captured — it is copyrighted firmware
    /// the user supplies separately at startup, so it must not be
    /// embedded in save-state files.  Only the [`bios_locked`](Self)
    /// flag is captured.
    pub fn capture_memory_state(&self) -> super::console::save_state::BusMemoryState {
        super::console::save_state::BusMemoryState {
            ewram: self.ewram.clone(),
            iwram: self.iwram.clone(),
            pram: self.pram.clone(),
            vram: self.vram.clone(),
            oam: self.oam.clone(),
            sram: self.sram.clone(),
            bios_locked: self.bios_locked,
            last_bus_value: self.last_bus_value,
            dma_latch: self.dma_latch,
        }
    }

    /// Restore the bus memory regions captured by
    /// [`capture_memory_state`](Self::capture_memory_state).
    ///
    /// The currently loaded BIOS image is preserved — only the BIOS
    /// lock flag is restored.
    ///
    /// Returns an error if any region's length does not match the
    /// expected GBA region size.
    pub fn restore_memory_state(
        &mut self,
        state: &super::console::save_state::BusMemoryState,
    ) -> Result<(), String> {
        check_region_size(&state.ewram, EWRAM_SIZE, "EWRAM")?;
        check_region_size(&state.iwram, IWRAM_SIZE, "IWRAM")?;
        check_region_size(&state.pram, PRAM_SIZE, "PRAM")?;
        check_region_size(&state.vram, VRAM_SIZE, "VRAM")?;
        check_region_size(&state.oam, OAM_SIZE, "OAM")?;
        check_region_size(&state.sram, SRAM_SIZE, "SRAM")?;
        self.ewram.clone_from(&state.ewram);
        self.iwram.clone_from(&state.iwram);
        self.pram.clone_from(&state.pram);
        self.vram.clone_from(&state.vram);
        self.oam.clone_from(&state.oam);
        self.sram.clone_from(&state.sram);
        match &mut self.cart_save {
            SaveBackend::Sram(sram) => sram.restore(&self.sram),
            // The save-state bus mirror is only 64 KB. Restoring a Flash backend
            // (especially Flash128) from this mirror would silently truncate data.
            // Keep the backend as-is until full Flash snapshot state is serialized.
            SaveBackend::Flash(_) => {}
            SaveBackend::None | SaveBackend::Eeprom(_) => {}
        }
        self.bios_locked = state.bios_locked;
        self.last_bus_value = state.last_bus_value;
        self.dma_latch = state.dma_latch;
        Ok(())
    }

    /// Return non-sequential access cycle count for `addr` and access width.
    /// During active PPU rendering, VRAM (0x6), Palette RAM (0x5), and OAM
    /// (0x7) incur an additional 1-cycle wait per GBATek "LCD VRAM Overview".
    pub fn n_cycles_width(&self, addr: u32, width: WidthClass) -> u32 {
        self.waitstates.n_cycles(addr, width) + self.video_mem_extra_cycles(addr)
    }

    /// Return sequential access cycle count for `addr` and access width.
    /// Applies the same PPU-driven wait-state addition as [`n_cycles_width`].
    pub fn s_cycles_width(&self, addr: u32, width: WidthClass) -> u32 {
        self.waitstates.s_cycles(addr, width) + self.video_mem_extra_cycles(addr)
    }

    /// Returns 1 when the PPU requires an extra wait cycle for access to
    /// the given address, 0 otherwise.
    ///
    /// Affects VRAM (region 0x6), Palette RAM (region 0x5), and OAM (region
    /// 0x7) during active rendering. See [`Ppu::vram_pram_active_wait`] and
    /// [`Ppu::oam_active_wait`] for the precise conditions.
    #[inline]
    fn video_mem_extra_cycles(&self, addr: u32) -> u32 {
        match (addr >> 24) & 0xF {
            0x5 | 0x6 => u32::from(self.ppu.vram_pram_active_wait()),
            0x7 => u32::from(self.ppu.oam_active_wait()),
            _ => 0,
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

    fn try_read_mgba_debug16(&self, addr: u32) -> Option<u16> {
        match addr {
            MGBA_DEBUG_ENABLE if self.mgba_debug_enabled => Some(MGBA_DEBUG_OPEN_VALUE),
            _ => None,
        }
    }

    fn write_mgba_debug8(&mut self, addr: u32, value: u8) -> bool {
        if (MGBA_DEBUG_STRING..MGBA_DEBUG_STRING + MGBA_DEBUG_STRING_LEN as u32).contains(&addr) {
            self.mgba_debug_string[(addr - MGBA_DEBUG_STRING) as usize] = value;
            return true;
        }
        false
    }

    fn write_mgba_debug16(&mut self, addr: u32, value: u16) -> bool {
        if addr == MGBA_DEBUG_ENABLE {
            self.mgba_debug_enabled = value == MGBA_DEBUG_ENABLE_VALUE;
            if self.mgba_debug_enabled {
                self.mgba_log.clear();
                self.mgba_debug_string.fill(0);
            }
            return true;
        }
        if addr == MGBA_DEBUG_FLAGS {
            if self.mgba_debug_enabled && value & MGBA_DEBUG_SEND_FLAG != 0 {
                let len = self
                    .mgba_debug_string
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(MGBA_DEBUG_STRING_LEN);
                let text = String::from_utf8_lossy(&self.mgba_debug_string[..len]);
                if self.trace_config.mgba_log > 0 {
                    emit_gba_bus_trace_line(format!("[GBA MGBA] {text}"));
                }
                self.mgba_log.push_str(&text);
                self.mgba_debug_string.fill(0);
            }
            return true;
        }
        if (MGBA_DEBUG_STRING..MGBA_DEBUG_STRING + MGBA_DEBUG_STRING_LEN as u32).contains(&addr) {
            let bytes = value.to_le_bytes();
            self.write_mgba_debug8(addr, bytes[0]);
            self.write_mgba_debug8(addr + 1, bytes[1]);
            return true;
        }
        false
    }

    fn write_mgba_debug32(&mut self, addr: u32, value: u32) -> bool {
        if (MGBA_DEBUG_STRING..MGBA_DEBUG_STRING + MGBA_DEBUG_STRING_LEN as u32).contains(&addr) {
            for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_mgba_debug8(addr + offset as u32, byte);
            }
            return true;
        }
        false
    }

    fn trace_dma_cnt_h_write(&self, addr: u32, value: u16) {
        if self.trace_config.dma == 0 || value & 0x8000 == 0 {
            return;
        }
        let Some(channel) = dma_cnt_h_channel(addr) else {
            return;
        };
        let dma = self.dma.channels[channel];
        emit_gba_bus_trace_line(format!(
            "[GBA DMA] CH{channel} SRC={:08X} DST={:08X} COUNT={:04X} CNT_H={value:04X}",
            dma.sad, dma.dad, dma.count
        ));
    }

    /// Map the cartridge ROM with mirroring across the three wait-state
    /// regions. Returns `None` when no cartridge is inserted or when the
    /// offset falls beyond the ROM image — callers substitute the GBATek
    /// "no-cart" open-bus pattern (addr >> 1).
    fn rom_byte(&self, offset: usize) -> Option<u8> {
        self.rom.get(offset).copied()
    }

    /// Read a 16-bit little-endian halfword from cart ROM, respecting
    /// mirroring. Returns `None` when no cartridge is inserted.
    fn rom_u16(&self, offset: usize) -> Option<u16> {
        Some(u16::from_le_bytes([
            self.rom_byte(offset)?,
            self.rom_byte(offset + 1)?,
        ]))
    }

    /// Non-mutating halfword read used by debugging/tracing paths.
    ///
    /// This preserves `last_bus_value` so enabling tracing does not
    /// perturb open-bus-visible behavior.
    pub fn peek16(&mut self, addr: u32) -> u16 {
        let prev = self.last_bus_value;
        let value = self.read16(addr);
        self.last_bus_value = prev;
        value
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

    /// Non-mutating word read used by debugging/tracing paths.
    ///
    /// This preserves `last_bus_value` so enabling tracing does not
    /// perturb open-bus-visible behavior.
    pub fn peek32(&mut self, addr: u32) -> u32 {
        let prev = self.last_bus_value;
        let value = self.read32(addr);
        self.last_bus_value = prev;
        value
    }
}

fn dma_cnt_h_channel(addr: u32) -> Option<usize> {
    if !(0x0400_00B0..=0x0400_00DF).contains(&addr) {
        return None;
    }
    let rel = addr - 0x0400_00B0;
    (rel % 12 == 10).then_some((rel / 12) as usize)
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
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(aligned) {
                    value as u32 | ((value as u32) << 16)
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    let lo = self.apu.read16(aligned) as u32;
                    // Only read the upper halfword if it is also within range.
                    let hi = if aligned + 2 <= 0x0400_00A6 {
                        self.apu.read16(aligned + 2) as u32
                    } else {
                        0
                    };
                    lo | (hi << 16)
                } else {
                    self.io
                        .try_read32(
                            aligned,
                            &self.ic,
                            &self.timers,
                            &self.dma,
                            &self.ppu,
                            &self.keypad,
                        )
                        .unwrap_or_else(|| self.open_bus_word())
                }
            }
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
                let b = self.cart_read8(aligned);
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
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(aligned) {
                    value
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.read16(aligned)
                } else if aligned == 0x0400_0128 {
                    self.sio.read_siocnt()
                } else {
                    let raw = self
                        .io
                        .try_read16(
                            aligned,
                            &self.ic,
                            &self.timers,
                            &self.dma,
                            &self.ppu,
                            &self.keypad,
                        )
                        .unwrap_or_else(|| self.open_bus_halfword(aligned));
                    // WAITCNT: bits 13, 15 are unused and read as 0.
                    if aligned == 0x0400_0204 {
                        raw & 0x5FFF
                    } else {
                        raw
                    }
                }
            }
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
                let b = self.cart_read8(addr);
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
            0x4 => {
                if let Some(value) = self.try_read_mgba_debug16(addr & !1) {
                    if addr & 1 == 0 {
                        value as u8
                    } else {
                        (value >> 8) as u8
                    }
                } else if addr == 0x0400_0410 {
                    self.undoc_0x410
                } else {
                    let aligned_hw = addr & !0x1;
                    if (0x0400_0060..=0x0400_00A6).contains(&aligned_hw) {
                        let hw = self.apu.read16(aligned_hw);
                        if addr & 1 == 0 {
                            hw as u8
                        } else {
                            (hw >> 8) as u8
                        }
                    } else {
                        self.io
                            .try_read8(
                                addr,
                                &self.ic,
                                &self.timers,
                                &self.dma,
                                &self.ppu,
                                &self.keypad,
                            )
                            .unwrap_or_else(|| self.open_bus_byte(addr))
                    }
                }
            }
            0x5 => self.pram[(addr as usize) % PRAM_SIZE],
            0x6 => self.vram[vram_offset(addr)],
            0x7 => self.oam[(addr as usize) % OAM_SIZE],
            0x8..=0xD => {
                let off = (addr & 0x01FF_FFFF) as usize;
                self.rom_byte(off).unwrap_or(open_bus_no_cart_byte(addr))
            }
            0xE | 0xF => self.cart_read8(addr),
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
            0x4 => {
                if self.write_mgba_debug32(aligned, value) {
                    return;
                }
                // FIFO A and B need full 32-bit word writes.
                if aligned == 0x0400_00A0 {
                    self.apu.write_fifo_a_word(value);
                } else if aligned == 0x0400_00A4 {
                    self.apu.write_fifo_b_word(value);
                } else if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.write16(aligned, value as u16);
                    // Only write the upper halfword if it is also within range.
                    if aligned + 2 <= 0x0400_00A6 {
                        self.apu.write16(aligned + 2, (value >> 16) as u16);
                    }
                } else {
                    // Intercept HALTCNT: write32 to 0x04000300 covers POSTFLG (byte 0),
                    // HALTCNT (byte 1), and two unused bytes.
                    if aligned == 0x0400_0300 {
                        let haltcnt_byte = ((value >> 8) & 0xFF) as u8;
                        if haltcnt_byte & 0x80 == 0 {
                            self.halt_requested = true;
                        }
                    }
                    self.io.write32(
                        aligned,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    // WAITCNT is at 0x0400_0204; a 32-bit write spans 0x204-0x207.
                    if aligned == 0x0400_0204 {
                        self.waitstates.recalculate(value as u16);
                    }
                    // SIOCNT is at 0x0400_0128 (low halfword of a 32-bit write).
                    if aligned == 0x0400_0128 {
                        self.sio.write_siocnt(value as u16);
                    }
                    // RCNT is at 0x0400_0134 (low halfword of a 32-bit write).
                    if aligned == 0x0400_0134 {
                        self.sio.write_rcnt(value as u16);
                    }
                }
            }
            0x5 => write_le_u32(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u32(&mut self.vram, off, value);
            }
            0x7 => write_le_u32(&mut self.oam, aligned as usize, value),
            0x8..=0xD => { /* Cartridge ROM is read-only via the bus */ }
            0xE | 0xF => {
                // Cart RAM is an 8-bit bus: a 32-bit store writes only the addressed byte lane.
                let shift = (addr & 0x3) * 8;
                let byte = ((value >> shift) & 0xFF) as u8;
                self.cart_write8(addr, byte);
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
            0x4 => {
                if self.write_mgba_debug16(aligned, value) {
                    return;
                }
                if (0x0400_0060..=0x0400_00A6).contains(&aligned) {
                    self.apu.write16(aligned, value);
                } else {
                    // Intercept HALTCNT: write16 to 0x04000300 covers POSTFLG (low byte)
                    // and HALTCNT (high byte).
                    if aligned == 0x0400_0300 {
                        let haltcnt_byte = (value >> 8) as u8;
                        if haltcnt_byte & 0x80 == 0 {
                            self.halt_requested = true;
                        }
                    }
                    self.io.write16(
                        aligned,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    self.trace_dma_cnt_h_write(aligned, value);
                    if aligned == 0x0400_0204 {
                        self.waitstates.recalculate(value);
                    }
                    if aligned == 0x0400_0128 {
                        self.sio.write_siocnt(value);
                    }
                    if aligned == 0x0400_0134 {
                        self.sio.write_rcnt(value);
                    }
                }
            }
            0x5 => write_le_u16(&mut self.pram, aligned as usize, value),
            0x6 => {
                let off = vram_offset(aligned);
                write_le_u16(&mut self.vram, off, value);
            }
            0x7 => write_le_u16(&mut self.oam, aligned as usize, value),
            0x8..=0xD => {}
            0xE | 0xF => {
                // Cart RAM is an 8-bit bus: halfword stores write only the addressed byte lane.
                let shift = (addr & 0x1) * 8;
                let byte = ((value as u32 >> shift) & 0xFF) as u8;
                self.cart_write8(addr, byte);
            }
            _ => {}
        }
        if touches_io && self.dma.any_pending() {
            self.run_pending_dma();
        }
    }

    fn write8(&mut self, addr: u32, value: u8) {
        if self.trace_config.bus > 0 {
            emit_gba_bus_trace_line(format!("[GBA BUS] W8 {addr:08X}={value:02X}"));
        }

        let shift = (addr & 3) * 8;
        self.last_bus_value =
            (self.last_bus_value & !(0xFFu32 << shift)) | ((value as u32) << shift);
        let touches_io = (addr >> 24) & 0xF == 0x4;
        match (addr >> 24) & 0xF {
            0x0 | 0x1 => {}
            0x2 => self.ewram[(addr as usize) % EWRAM_SIZE] = value,
            0x3 => self.iwram[(addr as usize) % IWRAM_SIZE] = value,
            0x4 => {
                if self.write_mgba_debug8(addr, value) {
                    return;
                }
                if addr == 0x0400_0410 {
                    self.undoc_0x410 = value;
                } else if addr == 0x0400_0301 {
                    // HALTCNT — bit 7 clear = halt mode, bit 7 set = stop mode (deferred).
                    if value & 0x80 == 0 {
                        self.halt_requested = true;
                    }
                } else if (0x0400_0060..=0x0400_00A7).contains(&addr) {
                    self.apu.write8(addr, value);
                } else {
                    self.io.write8(
                        addr,
                        value,
                        &mut self.ic,
                        &mut self.timers,
                        &mut self.dma,
                        &mut self.ppu,
                        &mut self.keypad,
                    );
                    // Byte writes to SIOCNT/RCNT must update the Sio module.
                    // Merge the written byte into the current register value.
                    let aligned = addr & !1;
                    if aligned == 0x0400_0128 {
                        let merged = self.io.backing_u16(0x0400_0128);
                        self.sio.write_siocnt(merged);
                    }
                    if aligned == 0x0400_0134 {
                        let merged = self.io.backing_u16(0x0400_0134);
                        self.sio.write_rcnt(merged);
                    }
                }
            }
            0x5 => {
                // Byte writes to PRAM duplicate the byte to a halfword.
                let off = (addr as usize & !1) % PRAM_SIZE;
                self.pram[off] = value;
                self.pram[off + 1] = value;
            }
            0x6 => {
                // Byte writes to BG VRAM duplicate the byte to a halfword.
                // Byte writes to OBJ VRAM (offset >= 0x10000) are ignored.
                // TODO: In bitmap modes 3/5, the BG/OBJ boundary is 0x14000
                // rather than 0x10000. This threshold is correct for tile modes
                // (0-2) and mode 4, but needs DISPCNT check for full accuracy.
                let off = vram_offset(addr);
                if off < 0x10000 {
                    let aligned = off & !1;
                    self.vram[aligned] = value;
                    self.vram[aligned + 1] = value;
                }
            }
            0x7 => { /* OAM ignores byte writes */ }
            0x8..=0xD => {}
            0xE | 0xF => self.cart_write8(addr, value),
            _ => {}
        }
        if touches_io && self.dma.any_pending() {
            self.run_pending_dma();
        }
    }

    fn n_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.n_cycles_width(addr, width)
    }

    fn s_cycles(&self, addr: u32, width: WidthClass) -> u32 {
        self.s_cycles_width(addr, width)
    }
}

/// DMA transfers use the bus' standard read/write paths so that DMA
/// destinations like PRAM/VRAM/OAM see the same byte-mirroring rules as
/// CPU stores.
impl dma::DmaBus for GbaBus {
    fn dma_read16(&mut self, addr: u32) -> u16 {
        // Swap in the DMA latch so open-bus reads return the DMA's own
        // latched value rather than the CPU's last_bus_value.
        let saved = self.last_bus_value;
        self.last_bus_value = self.dma_latch;
        let val = <Self as Bus>::read16(self, addr);
        self.dma_latch = self.last_bus_value;
        self.last_bus_value = saved;
        val
    }
    fn dma_write16(&mut self, addr: u32, value: u16) {
        let saved = self.last_bus_value;
        <Self as Bus>::write16(self, addr, value);
        self.last_bus_value = saved;
    }
    fn dma_read32(&mut self, addr: u32) -> u32 {
        let saved = self.last_bus_value;
        self.last_bus_value = self.dma_latch;
        let val = <Self as Bus>::read32(self, addr);
        self.dma_latch = self.last_bus_value;
        self.last_bus_value = saved;
        val
    }
    fn dma_write32(&mut self, addr: u32, value: u32) {
        let saved = self.last_bus_value;
        <Self as Bus>::write32(self, addr, value);
        self.last_bus_value = saved;
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
    use crate::gba::cartridge::Flash;

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
    fn cart_read32_uses_active_save_backend() {
        let mut bus = GbaBus::new();
        bus.load_rom_with_save(&[0; 0xC0], SaveBackend::Flash(Flash::new_64k()));

        // Program two bytes in flash and ensure read32 mirrors backend byte,
        // not stale data from the legacy SRAM mirror.
        bus.write8(0x0E00_5555, 0xAA);
        bus.write8(0x0E00_2AAA, 0x55);
        bus.write8(0x0E00_5555, 0xA0);
        bus.write8(0x0E00_0010, 0x42);

        assert_eq!(bus.read32(0x0E00_0010), 0x4242_4242);
    }

    #[test]
    fn n_and_s_cycles_width_match_gbatek_defaults() {
        let bus = GbaBus::new();
        assert_eq!(
            bus.n_cycles_width(0x0300_0000, WidthClass::HalfwordOrByte),
            1
        );
        assert_eq!(
            bus.n_cycles_width(0x0200_0000, WidthClass::HalfwordOrByte),
            3
        );
        assert_eq!(bus.n_cycles_width(0x0200_0000, WidthClass::Word), 6);
        // ROM WS0: N16=4, S16=2 (mGBA/GBATek convention)
        assert_eq!(
            bus.n_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            4
        );
        assert_eq!(
            bus.s_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            2
        );
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
        // WAITCNT (0x04000204) — unused bits 13, 15 read as 0.
        bus.write16(0x0400_0204, 0xBEEF);
        assert_eq!(bus.read16(0x0400_0204), 0xBEEF & 0x5FFF);
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
    fn bus_write8_emits_trace_when_enabled() {
        let mut bus = GbaBus::new();
        bus.set_trace_config(GbaTraceConfig {
            bus: 1,
            ..GbaTraceConfig::default()
        });

        clear_gba_bus_trace_lines_for_tests();
        bus.write8(0x0300_0000, 0x12);
        let lines = take_gba_bus_trace_lines_for_tests();

        assert_eq!(lines, vec!["[GBA BUS] W8 03000000=12".to_string()]);
    }

    #[test]
    fn dma_cnt_h_enable_emits_trace_when_enabled() {
        let mut bus = GbaBus::new();
        bus.set_trace_config(GbaTraceConfig {
            dma: 1,
            ..GbaTraceConfig::default()
        });

        bus.write32(0x0400_00B0, 0x0200_0000);
        bus.write32(0x0400_00B4, 0x0200_1000);
        bus.write16(0x0400_00B8, 1);
        clear_gba_bus_trace_lines_for_tests();
        bus.write16(0x0400_00BA, 0x8000 | 0x0400);
        let lines = take_gba_bus_trace_lines_for_tests();

        assert_eq!(
            lines,
            vec!["[GBA DMA] CH0 SRC=02000000 DST=02001000 COUNT=0001 CNT_H=8400".to_string()]
        );
    }

    #[test]
    fn mgba_debug_console_open_returns_expected_magic() {
        let mut bus = GbaBus::new();

        bus.write16(MGBA_DEBUG_ENABLE, MGBA_DEBUG_ENABLE_VALUE);

        assert_eq!(bus.read16(MGBA_DEBUG_ENABLE), MGBA_DEBUG_OPEN_VALUE);
    }

    #[test]
    fn mgba_debug_console_captures_string_when_flags_send() {
        let mut bus = GbaBus::new();
        bus.write16(MGBA_DEBUG_ENABLE, MGBA_DEBUG_ENABLE_VALUE);

        for (offset, byte) in b"Memory tests: 1436/1552\n\0".iter().enumerate() {
            bus.write8(MGBA_DEBUG_STRING + offset as u32, *byte);
        }
        bus.write16(MGBA_DEBUG_FLAGS, MGBA_DEBUG_SEND_FLAG | 0x0002);

        assert_eq!(bus.mgba_log_snapshot(), "Memory tests: 1436/1552\n");
    }

    #[test]
    fn mgba_debug_console_enable_clears_previous_log_and_string_buffer() {
        let mut bus = GbaBus::new();
        bus.write16(MGBA_DEBUG_ENABLE, MGBA_DEBUG_ENABLE_VALUE);

        for (offset, byte) in b"stale log\0".iter().enumerate() {
            bus.write8(MGBA_DEBUG_STRING + offset as u32, *byte);
        }
        bus.write16(MGBA_DEBUG_FLAGS, MGBA_DEBUG_SEND_FLAG);
        bus.write8(MGBA_DEBUG_STRING, b'x');

        bus.write16(MGBA_DEBUG_ENABLE, MGBA_DEBUG_ENABLE_VALUE);
        bus.write16(MGBA_DEBUG_FLAGS, MGBA_DEBUG_SEND_FLAG);

        assert_eq!(bus.mgba_log_snapshot(), "");
    }

    #[test]
    fn sram_snapshot_reflects_cart_save_writes() {
        let mut bus = GbaBus::new();
        bus.write8(0x0E00_0000, b'N');
        bus.write8(0x0E00_0001, b'E');
        bus.write8(0x0E00_0002, b'S');

        assert_eq!(&bus.sram_snapshot()[..3], b"NES");
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
    fn bus_step_advances_peripherals_during_dma_stalls() {
        let mut bus = GbaBus::new();
        for i in 0..4 {
            bus.write32(0x0200_0000 + i * 4, 0xAABB_0000 + i);
        }
        bus.write32(0x0400_00B0, 0x0200_0000);
        bus.write32(0x0400_00B4, 0x0200_1000);
        bus.write16(0x0400_00B8, 4);
        bus.write16(0x0400_00BA, 0x8000 | 0x0400);

        bus.write16(0x0400_0100, 0);
        bus.write16(0x0400_0102, 0x80);
        bus.step(1);

        assert_eq!(bus.read16(0x0400_0100), 9);
        assert_eq!(bus.take_dma_stall_cycles(), 0);
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
        // Mode 3 uses affine — set identity matrix so rendering is linear.
        bus.write16(crate::gba::ppu::REG_BG2PA, 0x0100);
        bus.write16(crate::gba::ppu::REG_BG2PD, 0x0100);
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

    // ---------------------------------------------------------------
    // ROM out-of-bounds reads return addr>>1 pattern (open bus)
    // Per GBATek: when reading beyond ROM size, the cartridge bus
    // returns the halfword address / 2 (i.e., addr >> 1).
    // ---------------------------------------------------------------

    #[test]
    fn rom_oob_read16_returns_addr_shr1() {
        let mut bus = GbaBus::new();
        // Load a small 256-byte ROM.
        bus.load_rom(&[0u8; 256]);
        // Address 0x0924_68AC is well beyond 256 bytes.
        let v = bus.read16(0x0924_68AC);
        // Expected: (0x092468AC >> 1) & 0xFFFF = 0x3456
        assert_eq!(v, 0x3456);
    }

    #[test]
    fn rom_oob_read32_returns_addr_shr1_pattern() {
        let mut bus = GbaBus::new();
        bus.load_rom(&[0u8; 256]);
        let v = bus.read32(0x0924_68AC);
        // Low halfword: (0x092468AC >> 1) & 0xFFFF = 0x3456
        // High halfword: (0x092468AE >> 1) & 0xFFFF = 0x3457
        assert_eq!(v, 0x3457_3456);
    }

    #[test]
    fn rom_oob_read8_returns_addr_shr1_byte() {
        let mut bus = GbaBus::new();
        bus.load_rom(&[0u8; 256]);
        // 0x092468AC >> 1 = 0x049234D6 → halfword 0x3456
        // byte 0 (even) = 0x56, byte 1 (odd) = 0x34
        assert_eq!(bus.read8(0x0924_68AC), 0x56);
        assert_eq!(bus.read8(0x0924_68AD), 0x34);
    }

    // ---------------------------------------------------------------
    // VRAM OBJ region byte writes must be ignored
    // Per GBATek: byte writes to OBJ tile VRAM (offset >= 0x10000
    // in modes 0-2, >= 0x14000 in modes 3-5) are ignored.
    // The mgba suite tests non-bitmap mode (modes 0-2).
    // ---------------------------------------------------------------

    #[test]
    fn vram_obj_byte_write_is_ignored() {
        let mut bus = GbaBus::new();
        // Write initial data to OBJ VRAM (0x06010000+).
        bus.write16(0x0601_0000, 0xBB66);
        // Byte write to OBJ VRAM should be ignored.
        bus.write8(0x0601_0000, 0xD8);
        // Original data should be unchanged.
        assert_eq!(bus.read16(0x0601_0000), 0xBB66);
    }

    #[test]
    fn vram_bg_byte_write_still_duplicates() {
        let mut bus = GbaBus::new();
        // BG VRAM (< 0x06010000) byte writes should still duplicate.
        bus.write16(0x0600_FFE0, 0xBB66);
        bus.write8(0x0600_FFE0, 0xD8);
        assert_eq!(bus.read16(0x0600_FFE0), 0xD8D8);
    }

    #[test]
    fn dma_read32_uses_dma_latch_not_cpu_bus_value() {
        // The DMA controller has its own internal data latch, separate from
        // the CPU's open-bus value. When DMA reads from a restricted region
        // (e.g. locked BIOS), it returns the DMA latch — not the CPU's
        // last_bus_value.
        use crate::gba::bus::dma::DmaBus;
        let mut bus = GbaBus::new();
        bus.lock_bios();

        // Write known data to IWRAM.
        bus.write32(0x0300_0000, 0xCAFE_BABE);

        // DMA reads IWRAM → primes the DMA latch.
        let val = bus.dma_read32(0x0300_0000);
        assert_eq!(val, 0xCAFE_BABE);

        // CPU reads from EWRAM → changes last_bus_value but NOT DMA latch.
        bus.write32(0x0200_0000, 0xDEAD_BEEF);
        let _ = bus.read32(0x0200_0000);

        // DMA reads locked BIOS → should get DMA latch (0xCAFE_BABE),
        // not the CPU's last_bus_value (0xDEAD_BEEF).
        let bios_val = bus.dma_read32(0x0000_0000);
        assert_eq!(bios_val, 0xCAFE_BABE);
    }

    #[test]
    fn dma_read16_uses_dma_latch_halfword() {
        // 16-bit DMA reads should update only half the DMA latch, matching
        // the halfword alignment within the 32-bit latch register.
        use crate::gba::bus::dma::DmaBus;
        let mut bus = GbaBus::new();
        bus.lock_bios();

        // Prime DMA latch via two 16-bit reads from IWRAM.
        bus.write32(0x0300_0000, 0xAAAA_BBBB);
        let _ = bus.dma_read16(0x0300_0000); // low half → 0xBBBB
        let _ = bus.dma_read16(0x0300_0002); // high half → 0xAAAA
        // DMA latch is now 0xAAAA_BBBB.

        // CPU activity changes last_bus_value.
        bus.write32(0x0200_0000, 0x1111_2222);
        let _ = bus.read32(0x0200_0000);

        // DMA reads locked BIOS at aligned addr → returns DMA latch low half.
        let bios_lo = bus.dma_read16(0x0000_0000);
        assert_eq!(bios_lo, 0xBBBB);
    }

    #[test]
    fn dma_read_does_not_corrupt_cpu_last_bus_value() {
        // DMA reads must not change the CPU's open-bus value.
        use crate::gba::bus::dma::DmaBus;
        let mut bus = GbaBus::new();

        // Pre-populate IWRAM before setting the CPU sentinel.
        bus.write32(0x0300_0000, 0xBEEF_CAFE);

        // Set CPU's last_bus_value to a known sentinel.
        bus.write32(0x0200_0000, 0x5555_6666);
        let _ = bus.read32(0x0200_0000);

        // DMA reads from IWRAM — must not disturb CPU bus value.
        let _ = bus.dma_read32(0x0300_0000);

        // CPU's open-bus should still return the sentinel, not the DMA value.
        // Read from a region that returns open-bus (locked BIOS).
        bus.lock_bios();
        let cpu_open = bus.read32(0x0000_0000);
        assert_eq!(cpu_open, 0x5555_6666);
    }

    #[test]
    fn dma_write_does_not_corrupt_cpu_last_bus_value() {
        // DMA writes must not change the CPU's open-bus value.
        use crate::gba::bus::dma::DmaBus;
        let mut bus = GbaBus::new();

        // Set CPU's last_bus_value to a known sentinel.
        bus.write32(0x0200_0000, 0xAAAA_BBBB);
        let _ = bus.read32(0x0200_0000);

        // DMA writes to EWRAM — must not disturb CPU bus value.
        bus.dma_write32(0x0200_1000, 0xDEAD_BEEF);

        // CPU's open-bus should still return the sentinel.
        bus.lock_bios();
        let cpu_open = bus.read32(0x0000_0000);
        assert_eq!(cpu_open, 0xAAAA_BBBB);
    }

    // =========================================================================
    // WAITCNT + Dynamic Bus Timing Tests (#2394)
    // =========================================================================

    #[test]
    fn waitcnt_default_rom_ws0_n16_is_4() {
        let bus = GbaBus::new();
        // ROM WS0 region (0x0800_0000): default N16 = 4 per GBATek/mGBA
        assert_eq!(
            bus.n_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            4
        );
    }

    #[test]
    fn waitcnt_default_rom_ws0_s16_is_2() {
        let bus = GbaBus::new();
        // ROM WS0 region: default S16 = 2
        assert_eq!(
            bus.s_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            2
        );
    }

    #[test]
    fn waitcnt_default_rom_ws0_n32_is_7() {
        let bus = GbaBus::new();
        // ROM WS0: N32 = N16 + 1 + S16 = 4 + 1 + 2 = 7
        assert_eq!(bus.n_cycles_width(0x0800_0000, WidthClass::Word), 7);
    }

    #[test]
    fn waitcnt_default_rom_ws0_s32_is_5() {
        let bus = GbaBus::new();
        // ROM WS0: S32 = 2×S16 + 1 = 2×2 + 1 = 5
        assert_eq!(bus.s_cycles_width(0x0800_0000, WidthClass::Word), 5);
    }

    #[test]
    fn waitcnt_default_rom_ws1_n16_is_4() {
        let bus = GbaBus::new();
        // ROM WS1 region (0x0A00_0000): default N16 = 4
        assert_eq!(
            bus.n_cycles_width(0x0A00_0000, WidthClass::HalfwordOrByte),
            4
        );
    }

    #[test]
    fn waitcnt_default_rom_ws1_s16_is_4() {
        let bus = GbaBus::new();
        // ROM WS1: default S16 = 4 (different from WS0!)
        assert_eq!(
            bus.s_cycles_width(0x0A00_0000, WidthClass::HalfwordOrByte),
            4
        );
    }

    #[test]
    fn waitcnt_default_rom_ws2_s16_is_8() {
        let bus = GbaBus::new();
        // ROM WS2 (0x0C00_0000): default S16 = 8
        assert_eq!(
            bus.s_cycles_width(0x0C00_0000, WidthClass::HalfwordOrByte),
            8
        );
    }

    #[test]
    fn waitcnt_default_sram_n16_is_4() {
        let bus = GbaBus::new();
        // SRAM region (0x0E00_0000): default = 4
        assert_eq!(
            bus.n_cycles_width(0x0E00_0000, WidthClass::HalfwordOrByte),
            4
        );
    }

    #[test]
    fn waitcnt_write_changes_rom_ws0_timing() {
        let mut bus = GbaBus::new();
        // Write WAITCNT: bits 2-3 = 0b10 → WS0 N = 2, bit 4 = 1 → WS0 S = 1
        let waitcnt: u16 = 0b00_0000_0001_1000; // WS0 N=2(idx 2), WS0 S=1(idx 1)
        bus.write16(0x0400_0204, waitcnt);
        assert_eq!(
            bus.n_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            2
        );
        assert_eq!(
            bus.s_cycles_width(0x0800_0000, WidthClass::HalfwordOrByte),
            1
        );
    }

    #[test]
    fn waitcnt_read_back_returns_written_value() {
        let mut bus = GbaBus::new();
        // WAITCNT is readable — write a value and read it back.
        // Bits 13 and 15 are unused and should read as 0.
        bus.write16(0x0400_0204, 0xEF1F); // set bits 13, 15 (unused) + valid bits
        let readback = bus.read16(0x0400_0204);
        // Verify unused bits 13 and 15 are cleared on read
        assert_eq!(readback & (1 << 13), 0, "bit 13 should read as 0");
        assert_eq!(readback & (1 << 15), 0, "bit 15 should read as 0");
        // Verify writable bits are preserved (mask 0x5FFF = bits 0-12, 14)
        assert_eq!(readback, 0xEF1F & 0x5FFF);
    }

    #[test]
    fn waitcnt_ws1_ws2_independent_from_ws0() {
        let mut bus = GbaBus::new();
        // Set WS0 to fastest (N=2, S=1) but leave WS1/WS2 at defaults
        let waitcnt: u16 = 0b00_0000_0001_1000;
        bus.write16(0x0400_0204, waitcnt);
        // WS1 should still be at defaults (N=4, S=4)
        assert_eq!(
            bus.n_cycles_width(0x0A00_0000, WidthClass::HalfwordOrByte),
            4
        );
        assert_eq!(
            bus.s_cycles_width(0x0A00_0000, WidthClass::HalfwordOrByte),
            4
        );
    }

    // ---------------------------------------------------------------
    // HALTCNT register (0x04000301) — halt_requested flag
    // ---------------------------------------------------------------

    #[test]
    fn halt_requested_starts_false() {
        let bus = GbaBus::new();
        assert!(!bus.halt_requested());
    }

    #[test]
    fn halt_requested_cleared_by_clear() {
        let mut bus = GbaBus::new();
        bus.write8(0x0400_0301, 0x00);
        assert!(bus.halt_requested(), "write8 0x00 should request halt");
        bus.clear_halt_request();
        assert!(!bus.halt_requested(), "clear should reset the flag");
    }

    #[test]
    fn haltcnt_write8_halt_mode_sets_flag() {
        let mut bus = GbaBus::new();
        bus.write8(0x0400_0301, 0x00);
        assert!(bus.halt_requested(), "bit 7 clear → halt mode");
    }

    #[test]
    fn haltcnt_write8_stop_mode_does_not_set_flag() {
        let mut bus = GbaBus::new();
        bus.write8(0x0400_0301, 0x80);
        assert!(
            !bus.halt_requested(),
            "bit 7 set → stop mode, not supported yet"
        );
    }

    #[test]
    fn haltcnt_write16_halt_mode_sets_flag() {
        let mut bus = GbaBus::new();
        // Write16 to 0x04000300: high byte (HALTCNT) = 0x00 → halt mode.
        bus.write16(0x0400_0300, 0x0001);
        assert!(
            bus.halt_requested(),
            "write16 with high byte 0x00 → halt mode"
        );
    }

    #[test]
    fn haltcnt_write16_stop_mode_does_not_set_flag() {
        let mut bus = GbaBus::new();
        // High byte = 0x80 → stop mode.
        bus.write16(0x0400_0300, 0x8001);
        assert!(
            !bus.halt_requested(),
            "write16 with high byte 0x80 → stop mode"
        );
    }

    #[test]
    fn haltcnt_write32_halt_mode_sets_flag() {
        let mut bus = GbaBus::new();
        // write32 to 0x04000300: byte 1 (bits 8-15) = HALTCNT = 0x00 → halt mode.
        bus.write32(0x0400_0300, 0x0000_0001);
        assert!(
            bus.halt_requested(),
            "write32 with HALTCNT byte 0x00 → halt mode"
        );
    }

    #[test]
    fn haltcnt_write32_stop_mode_does_not_set_flag() {
        let mut bus = GbaBus::new();
        // write32 to 0x04000300: byte 1 (bits 8-15) = 0x80 → stop mode.
        bus.write32(0x0400_0300, 0x0000_8001);
        assert!(
            !bus.halt_requested(),
            "write32 with HALTCNT byte 0x80 → stop mode"
        );
    }

    #[test]
    fn timer_overflow_advances_fifo_a_and_triggers_dma() {
        // Configure FIFO A to use Timer 0 (SOUNDCNT_H bit 10 = 0).
        let mut bus = GbaBus::new();

        // Push some samples into FIFO A.
        bus.apu.fifo_a.push(10);
        bus.apu.fifo_a.push(20);
        bus.apu.fifo_a.push(30);
        assert_eq!(bus.apu.fifo_a.current, 0); // not yet advanced

        // SOUNDCNT_H: FIFO A uses timer 0 (bit 10 = 0), enable A right (bit 8).
        bus.apu.soundcnt_h = 0x0100;

        // Set Timer 0 reload to 0xFFFF and enable (overflows after 1 tick at prescaler=1).
        bus.timers.write_cnt_l(0, 0xFFFF);
        bus.timers.write_cnt_h(0, 0x0080); // enable, prescaler=1

        // Step 1 cycle — should overflow TM0 and advance FIFO A.
        bus.step(1);

        assert_eq!(bus.apu.fifo_a.current, 10, "FIFO A should have advanced");
    }

    #[test]
    fn timer1_overflow_advances_fifo_b_when_configured() {
        // Configure FIFO B to use Timer 1 (SOUNDCNT_H bit 14 = 1).
        let mut bus = GbaBus::new();

        bus.apu.fifo_b.push(42);
        bus.apu.fifo_b.push(43);
        assert_eq!(bus.apu.fifo_b.current, 0);

        // SOUNDCNT_H: FIFO B uses timer 1 (bit 14 = 1), enable B right (bit 12).
        bus.apu.soundcnt_h = 0x5000; // bit 14 + bit 12

        // Enable Timer 1 to overflow immediately.
        bus.timers.write_cnt_l(1, 0xFFFF);
        bus.timers.write_cnt_h(1, 0x0080);

        bus.step(1);

        assert_eq!(bus.apu.fifo_b.current, 42, "FIFO B should have advanced");
    }

    #[test]
    fn timer_overflow_multiple_times_advances_fifo_a_multiple_times() {
        // Per GBATek: every timer overflow moves one 8-bit sample from FIFO to
        // the sound circuit. When a timer with reload=0xFFFF overflows on every
        // CPU cycle, stepping by 3 cycles must advance FIFO A exactly 3 times.
        let mut bus = GbaBus::new();

        bus.apu.fifo_a.push(11);
        bus.apu.fifo_a.push(22);
        bus.apu.fifo_a.push(33);
        assert_eq!(bus.apu.fifo_a.current, 0);

        // FIFO A uses Timer 0 (SOUNDCNT_H bit 10 = 0), enable A right (bit 8).
        bus.apu.soundcnt_h = 0x0100;

        // reload=0xFFFF → overflow on every cycle.
        bus.timers.write_cnt_l(0, 0xFFFF);
        bus.timers.write_cnt_h(0, 0x0080); // enable, prescaler=1

        // Step 3 cycles → TM0 overflows 3 times → FIFO A must advance 3 times.
        bus.step(3);

        assert_eq!(
            bus.apu.fifo_a.current, 33,
            "FIFO A should have advanced 3 times (current=33)"
        );
    }

    #[test]
    fn timer_overflow_multiple_times_advances_fifo_b_multiple_times() {
        // Same as above but for FIFO B on Timer 1.
        let mut bus = GbaBus::new();

        bus.apu.fifo_b.push(55);
        bus.apu.fifo_b.push(66);
        bus.apu.fifo_b.push(77);
        assert_eq!(bus.apu.fifo_b.current, 0);

        // FIFO B uses Timer 1 (SOUNDCNT_H bit 14 = 1), enable B right (bit 12).
        bus.apu.soundcnt_h = 0x5000;

        bus.timers.write_cnt_l(1, 0xFFFF);
        bus.timers.write_cnt_h(1, 0x0080);

        bus.step(3);

        assert_eq!(
            bus.apu.fifo_b.current, 77,
            "FIFO B should have advanced 3 times (current=77)"
        );
    }

    // ---------------------------------------------------------------
    // BG scroll registers (0x04000010–0x0400001E) are write-only.
    // Per GBATek I/O map, CPU reads should return open-bus, not the
    // last-written scroll value.
    // ---------------------------------------------------------------

    // =========================================================================
    // PPU Video Memory Access Timing Tests
    // Per GBATek "LCD VRAM Overview": CPU access to VRAM/OAM/Palette during
    // active display inserts a 1-cycle wait. OAM is additionally constrained
    // during H-Blank unless DISPCNT bit 5 (H-Blank Interval Free) is set.
    // =========================================================================

    #[test]
    fn vram_pram_extra_wait_during_active_display() {
        // Default state: vcount=0, line_cycle=0 — PPU is in active display.
        let bus = GbaBus::new();
        // PRAM base N16=1, N32=2; should be +1 during active display.
        assert_eq!(
            bus.n_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            2,
            "PRAM n_cycles16 should be +1 during active display"
        );
        assert_eq!(
            bus.n_cycles_width(0x0500_0000, WidthClass::Word),
            3,
            "PRAM n_cycles32 should be +1 during active display"
        );
        // VRAM base N16=1, N32=2; should be +1 during active display.
        assert_eq!(
            bus.n_cycles_width(0x0600_0000, WidthClass::HalfwordOrByte),
            2,
            "VRAM n_cycles16 should be +1 during active display"
        );
        assert_eq!(
            bus.n_cycles_width(0x0600_0000, WidthClass::Word),
            3,
            "VRAM n_cycles32 should be +1 during active display"
        );
        // s_cycles should also get the +1.
        assert_eq!(
            bus.s_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            2,
            "PRAM s_cycles16 should be +1 during active display"
        );
        assert_eq!(
            bus.s_cycles_width(0x0600_0000, WidthClass::HalfwordOrByte),
            2,
            "VRAM s_cycles16 should be +1 during active display"
        );
    }

    #[test]
    fn oam_extra_wait_during_active_display() {
        // Default state: vcount=0, line_cycle=0 — PPU is in active display.
        let bus = GbaBus::new();
        // OAM base N16=1, N32=1; should be +1 during active display.
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            2,
            "OAM n_cycles16 should be +1 during active display"
        );
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::Word),
            2,
            "OAM n_cycles32 should be +1 during active display"
        );
        assert_eq!(
            bus.s_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            2,
            "OAM s_cycles16 should be +1 during active display"
        );
    }

    #[test]
    fn vram_pram_no_extra_wait_during_hblank() {
        let mut bus = GbaBus::new();
        // Advance to H-blank on scanline 0.
        bus.step(crate::gba::ppu::HBLANK_START_CYCLE);
        // VRAM/PRAM should return base cycles — no extra wait during H-blank.
        assert_eq!(
            bus.n_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            1,
            "PRAM n_cycles16 should be base-only during H-blank"
        );
        assert_eq!(
            bus.n_cycles_width(0x0600_0000, WidthClass::HalfwordOrByte),
            1,
            "VRAM n_cycles16 should be base-only during H-blank"
        );
        assert_eq!(
            bus.s_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            1,
            "PRAM s_cycles16 should be base-only during H-blank"
        );
    }

    #[test]
    fn oam_extra_wait_during_hblank_without_hblank_interval_free() {
        // DISPCNT=0: HBLANK_INTERVAL_FREE is not set.
        let mut bus = GbaBus::new();
        bus.step(crate::gba::ppu::HBLANK_START_CYCLE);
        // OAM should still need the extra wait during H-blank when bit5 is clear.
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            2,
            "OAM n_cycles16 should be +1 during H-blank without HBLANK_INTERVAL_FREE"
        );
    }

    #[test]
    fn oam_no_extra_wait_during_hblank_with_hblank_interval_free() {
        let mut bus = GbaBus::new();
        // Set HBLANK_INTERVAL_FREE (DISPCNT bit 5) before advancing.
        bus.ppu
            .write_dispcnt(crate::gba::ppu::dispcnt::HBLANK_INTERVAL_FREE);
        bus.step(crate::gba::ppu::HBLANK_START_CYCLE);
        // OAM is freely accessible during H-blank when HBLANK_INTERVAL_FREE is set.
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            1,
            "OAM n_cycles16 should be base-only during H-blank with HBLANK_INTERVAL_FREE"
        );
    }

    #[test]
    fn video_mem_no_extra_wait_during_vblank() {
        let mut bus = GbaBus::new();
        // Step through all visible scanlines to enter V-blank.
        bus.step(crate::gba::ppu::VISIBLE_SCANLINES * crate::gba::ppu::CYCLES_PER_SCANLINE);
        // All video memory should be freely accessible during V-blank.
        assert_eq!(
            bus.n_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            1,
            "PRAM n_cycles16 should be base-only during V-blank"
        );
        assert_eq!(
            bus.n_cycles_width(0x0600_0000, WidthClass::HalfwordOrByte),
            1,
            "VRAM n_cycles16 should be base-only during V-blank"
        );
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            1,
            "OAM n_cycles16 should be base-only during V-blank"
        );
    }

    #[test]
    fn video_mem_no_extra_wait_during_forced_blank() {
        let mut bus = GbaBus::new();
        // Set FORCED_BLANK (DISPCNT bit 7); PPU is still at scanline 0 active display,
        // but forced blank overrides the wait-state restriction.
        bus.ppu
            .write_dispcnt(crate::gba::ppu::dispcnt::FORCED_BLANK);
        assert_eq!(
            bus.n_cycles_width(0x0500_0000, WidthClass::HalfwordOrByte),
            1,
            "PRAM n_cycles16 should be base-only during forced blank"
        );
        assert_eq!(
            bus.n_cycles_width(0x0600_0000, WidthClass::HalfwordOrByte),
            1,
            "VRAM n_cycles16 should be base-only during forced blank"
        );
        assert_eq!(
            bus.n_cycles_width(0x0700_0000, WidthClass::HalfwordOrByte),
            1,
            "OAM n_cycles16 should be base-only during forced blank"
        );
    }

    #[test]
    fn bg_scroll_registers_are_write_only_return_open_bus() {
        let mut bus = GbaBus::new();

        // Write non-zero values to all eight BG scroll registers.
        bus.write16(0x0400_0010, 0x0055); // BG0HOFS
        bus.write16(0x0400_0012, 0x00AA); // BG0VOFS
        bus.write16(0x0400_0014, 0x0055); // BG1HOFS
        bus.write16(0x0400_0016, 0x00AA); // BG1VOFS
        bus.write16(0x0400_0018, 0x0055); // BG2HOFS
        bus.write16(0x0400_001A, 0x00AA); // BG2VOFS
        bus.write16(0x0400_001C, 0x0055); // BG3HOFS
        bus.write16(0x0400_001E, 0x00AA); // BG3VOFS

        let addrs: &[(u32, &str)] = &[
            (0x0400_0010, "BG0HOFS"),
            (0x0400_0012, "BG0VOFS"),
            (0x0400_0014, "BG1HOFS"),
            (0x0400_0016, "BG1VOFS"),
            (0x0400_0018, "BG2HOFS"),
            (0x0400_001A, "BG2VOFS"),
            (0x0400_001C, "BG3HOFS"),
            (0x0400_001E, "BG3VOFS"),
        ];

        // Re-establish a clearly different open-bus sentinel before reading.
        // Reads of write-only scroll registers must return this open-bus
        // value (via last_bus_value), not the scroll values we wrote.
        bus.write32(0x0200_0000, 0x1234_5678);
        let _ = bus.read32(0x0200_0000); // last_bus_value = 0x1234_5678

        for &(addr, name) in addrs {
            let val = bus.read16(addr);
            // The open-bus halfword returned is either 0x1234 or 0x5678
            // depending on the aligned half — both differ from the scroll
            // values we wrote (0x0055 and 0x00AA).
            assert_ne!(
                val, 0x0055,
                "{name} at {addr:#010X}: read returned scroll value, expected open-bus"
            );
            assert_ne!(
                val, 0x00AA,
                "{name} at {addr:#010X}: read returned scroll value, expected open-bus"
            );
        }
    }
}
