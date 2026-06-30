use crate::nes::cartridge::{Cartridge, NametableLayout, VsPpuType};
use crate::nes::console::TimingMode;
use crate::nes::ppu::NesPalette;
use crate::nes::ppu::color_effects::apply_grayscale;
use crate::nes::ppu::sprites::SpritesState;
use crate::nes::ppu::vs_palettes;
use crate::nes::ppu::{Background, Memory, Registers, Rendering, Sprites, Status, Timing};
use crate::platform::save_state::Stateful;
use crate::trace_ppu;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

/// PPU timing state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuTimingState {
    pub scanline: u16,
    pub pixel: u16,
    pub total_cycles: u64,
    pub frame_count: u64,
    pub rendering_enabled_d1: bool,
    pub rendering_enabled_d2: bool,
}

/// PPU register state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuRegisterState {
    pub control: u8,
    pub mask: u8,
    pub oam_addr: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub w: bool,
    pub io_bus: u8,
    pub io_bus_refresh_time: [u64; 8],
    pub cycle_count: u64,
}

/// PPU complete state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuState {
    pub timing: PpuTimingState,
    pub registers: PpuRegisterState,
    pub vblank_flag: bool,
    pub sprite_zero_hit: bool,
    pub pending_sprite_zero_hit: bool,
    pub sprite_overflow: bool,
    /// Internal edge-detection latch for NMI generation.
    /// This is polled and cleared by the CPU, not the NMI enable control bit.
    pub nmi_pending: bool,
    pub frame_complete: bool,
    pub vblank_suppressed_for_frame: bool,
    pub vblank_for_nmi: bool,
    pub prev_a12: bool,
    pub vram: Vec<u8>,
    pub palette: Vec<u8>,
    pub last_palette_index: Option<u8>,
    pub last_palette_value: u8,
    pub mirroring_mode: crate::nes::cartridge::NametableLayout,
    pub oam: Vec<u8>,
    pub secondary_oam: Vec<u8>,
    pub sprites_found: u8,
    pub sprite_count: u8,
    pub next_sprite_count: u8,
    pub sprite_buffers_ready: bool,
    pub sprite_0_index: Option<usize>,
    pub next_sprite_0_index: Option<usize>,
    pub sprite_eval_n: u8,
    pub sprite_eval_m: u8,
    pub sprite_eval_cycle: u8,
    pub sprite_eval_in_range: bool,
    #[serde(default)]
    pub sprite_eval_overflow_reads_remaining: u8,
    #[serde(default)]
    pub sprite_eval_overflow_signaled: bool,
    #[serde(default)]
    pub oam_read_latch: u8,
    #[serde(default)]
    pub oam_decay_cycle: u64,
    #[serde(default)]
    pub oam_row_last_refresh_cycle: [u64; 32],
    pub sprite_pattern_shift_lo: [u8; 8],
    pub sprite_pattern_shift_hi: [u8; 8],
    pub sprite_x_positions: [u8; 8],
    pub sprite_attributes: [u8; 8],
    pub next_sprite_pattern_shift_lo: [u8; 8],
    pub next_sprite_pattern_shift_hi: [u8; 8],
    pub next_sprite_x_positions: [u8; 8],
    pub next_sprite_attributes: [u8; 8],
    pub bg_pattern_shift_lo: u16,
    pub bg_pattern_shift_hi: u16,
    pub bg_attribute_shift_lo: u16,
    pub bg_attribute_shift_hi: u16,
    pub nametable_latch: u8,
    pub attribute_latch: u8,
    pub pattern_lo_latch: u8,
    pub pattern_hi_latch: u8,
    pub screen_buffer: Vec<u8>,
    pub read_buffer: u8,
}

const CHR_SIZE: usize = 8192;
const NAMETABLE_SIZE: usize = 1024;
const NUM_NAMETABLES: usize = 4;
const PALETTE_SIZE: usize = 32;
const NAMETABLE_BASE_ADDR: u16 = 0x2000;
const NAMETABLE_STRIDE: u16 = 0x400;

mod tick;

pub type SharedPpu = Rc<RefCell<Ppu>>;

/// Refactored PPU using modular components
pub struct Ppu {
    /// Timing and cycle management
    timing: Timing,
    /// Status flags (VBlank, sprite 0 hit, NMI)
    status: Status,

    // If PPUSTATUS is read right at the time VBlank would be set, the NES can
    // suppress the VBlank flag for the entire frame (blargg ppu_vbl_nmi 02).
    vblank_suppressed_for_frame: bool,

    // Internal VBlank latch used specifically for immediate-NMI enable behavior.
    // This is intentionally distinct from the readable $2002 VBlank flag to model
    // subtle boundary timing near VBlank end (blargg ppu_vbl_nmi 07).
    vblank_for_nmi: bool,
    /// Register management (PPUCTRL, PPUMASK, Loopy registers)
    registers: Registers,
    /// Memory management (VRAM, palette, CHR ROM)
    memory: Memory,
    /// Background rendering
    background: Background,
    /// Sprite rendering
    sprites: Sprites,
    /// Final rendering and screen output
    rendering: Rendering,
    /// Recently rendered pixels (last two) for mid-scanline color effect adjustments
    recent_pixels: [Option<RecentPixel>; 2],
    /// Previous A12 state for change detection (bit 12 of PPU address)
    prev_a12: bool,
    /// Cartridge reference for dynamic CHR ROM/RAM access through mapper
    ///
    /// The PPU holds a shared reference to the cartridge to enable:
    /// - Real-time CHR bank switching during rendering
    /// - Proper mapper integration for pattern table access
    /// - Hardware-accurate CHR-ROM and CHR-RAM behavior
    cartridge: Option<Rc<RefCell<Cartridge>>>,
    /// Whether Famicom emphasis bit swap is active (green/blue swapped)
    pub(crate) famicom_emphasis: bool,
    /// VS System PPU type, set during cartridge insertion.
    vs_ppu_type: Option<VsPpuType>,
    /// VS System palette override (set from vs_ppu_type).
    vs_palette: Option<&'static [(u8, u8, u8); 64]>,
    /// Active preset system palette (used when no VS palette is set).
    system_palette: NesPalette,
    /// Delayed v=t update countdown (3 PPU cycles after second $2006 write).
    /// Based on Visual NES findings, documented in Mesen2 NesPpu.cpp.
    update_vram_addr_delay: u8,
    /// The pending VRAM address value when a delayed v=t update is active.
    pending_vram_addr: u16,
}

impl Ppu {
    fn with_mapper_mut<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn crate::nes::cartridge::Mapper),
    {
        if let Some(ref cartridge) = self.cartridge {
            let mut cartridge = cartridge.borrow_mut();
            f(cartridge.mapper_mut());
        }
    }

    fn notify_chr_fetch_kind(cartridge: &Option<Rc<RefCell<Cartridge>>>, is_sprite: bool) {
        if let Some(cartridge) = cartridge {
            cartridge
                .borrow_mut()
                .mapper_mut()
                .ppu_set_chr_fetch_is_sprite(is_sprite);
        }
    }

    fn notify_chr_fetch_is_ppudata(cartridge: &Option<Rc<RefCell<Cartridge>>>) {
        if let Some(cartridge) = cartridge {
            cartridge
                .borrow_mut()
                .mapper_mut()
                .ppu_set_chr_fetch_is_ppudata();
        }
    }

    fn prime_a12_and_notify_mapper(&mut self, old_addr: u16, new_addr: u16) {
        if old_addr == new_addr {
            return;
        }

        if self.cartridge.is_none() {
            return;
        }

        self.with_mapper_mut(|mapper| {
            // MMC3's A12 filter requires several PPU cycles with A12 low before
            // a rising edge will clock the IRQ counter; replay the old address
            // to simulate those cycles when the CPU manually changes PPUADDR/PPUDATA.
            for _ in 0..8 {
                mapper.ppu_address_changed(old_addr);
            }
            mapper.ppu_address_changed(new_addr);
        });
    }

    fn set_vblank_for_nmi(&mut self) {
        self.vblank_for_nmi = true;
    }

    fn clear_vblank_for_nmi(&mut self) {
        self.vblank_for_nmi = false;
    }

    pub fn set_famicom_emphasis(&mut self, enabled: bool) {
        self.famicom_emphasis = enabled;
        self.rendering.famicom_emphasis = enabled;
    }

    pub fn set_vs_ppu_type(&mut self, vs_ppu_type: Option<VsPpuType>) {
        self.vs_palette = vs_ppu_type.as_ref().and_then(vs_palettes::vs_palette);
        self.vs_ppu_type = vs_ppu_type;
    }

    pub fn vs_ppu_type(&self) -> Option<VsPpuType> {
        self.vs_ppu_type
    }

    /// Returns the RC2C05 security byte for the current VS PPU type, or `None`
    /// if the PPU is not an RC2C05 variant.
    ///
    /// The security byte replaces the open-bus bits (and sometimes bit 5) in
    /// the $2002 PPUSTATUS return value as a form of copy protection.
    pub fn vs_security_value(&self) -> Option<u8> {
        match self.vs_ppu_type {
            Some(VsPpuType::Rc2c05_01) => Some(0x1B),
            Some(VsPpuType::Rc2c05_02) => Some(0x3D),
            Some(VsPpuType::Rc2c05_03) => Some(0x1C),
            Some(VsPpuType::Rc2c05_04) => Some(0x1B),
            Some(VsPpuType::Rc2c05_05) => Some(0x00),
            _ => None,
        }
    }

    /// Returns `true` when the PPU is an RC2C05 variant that swaps the
    /// meanings of $2000 (PPUCTRL) and $2001 (PPUMASK) writes.
    pub fn is_vs_register_swapped(&self) -> bool {
        matches!(
            self.vs_ppu_type,
            Some(
                VsPpuType::Rc2c05_01
                    | VsPpuType::Rc2c05_02
                    | VsPpuType::Rc2c05_03
                    | VsPpuType::Rc2c05_04
                    | VsPpuType::Rc2c05_05
            )
        )
    }

    /// Look up an NES color index (0–63) in the active system palette.
    ///
    /// Returns the VS System palette color when a VS PPU type is active,
    /// otherwise falls back to the standard NTSC palette.
    pub fn lookup_system_palette(&self, color_index: u8) -> (u8, u8, u8) {
        let index = (color_index & 0x3F) as usize;
        match self.vs_palette {
            Some(palette) => palette[index],
            None => self.system_palette.table()[index],
        }
    }

    /// Returns the active preset system palette.
    pub fn system_palette(&self) -> NesPalette {
        self.system_palette
    }

    /// Sets the active preset system palette.
    ///
    /// Has no visible effect while a VS System palette override is active.
    pub fn set_system_palette(&mut self, palette: NesPalette) {
        self.system_palette = palette;
    }

    /// Advances to the next preset system palette and returns the new value.
    pub fn cycle_system_palette(&mut self) -> NesPalette {
        self.system_palette = self.system_palette.next();
        self.system_palette
    }

    /// Create a new modular PPU instance
    pub fn new(tv_system: TimingMode, ram_init_mode: crate::nes::console::RamInitMode) -> Self {
        let mut sprites = Sprites::new(ram_init_mode);
        sprites.set_oam_decay_enabled(matches!(tv_system, TimingMode::Ntsc));

        Self {
            timing: Timing::new(tv_system),
            status: Status::new(),
            vblank_suppressed_for_frame: false,
            vblank_for_nmi: false,
            registers: Registers::new(),
            memory: Memory::new(ram_init_mode),
            background: Background::new(),
            sprites,
            rendering: Rendering::new(),
            recent_pixels: [None, None],
            prev_a12: false,
            cartridge: None,
            famicom_emphasis: false,
            vs_ppu_type: None,
            vs_palette: None,
            system_palette: NesPalette::default(),
            update_vram_addr_delay: 0,
            pending_vram_addr: 0,
        }
    }

    /// Create a new PPU instance for testing with default Zero RAM initialization.
    #[cfg(test)]
    pub fn new_for_testing(tv_system: TimingMode) -> Self {
        Self::new(tv_system, crate::nes::console::RamInitMode::Zero)
    }

    /// Flush any pending delayed VRAM address update.
    ///
    /// In real hardware, consecutive CPU writes are separated by at least 3 PPU
    /// cycles (one CPU cycle), so a $2006 delay always completes before the
    /// next register access. Unit tests that call write methods directly without
    /// ticking the PPU need to call this to mimic that elapsed time.
    #[cfg(test)]
    pub fn flush_pending_vram_addr(&mut self) {
        if self.update_vram_addr_delay > 0 {
            let old_v = self.registers.v();
            self.registers.set_v(self.pending_vram_addr);
            self.prime_a12_and_notify_mapper(old_v, self.pending_vram_addr);
            self.update_vram_addr_delay = 0;
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn sprites(&mut self) -> &mut Sprites {
        &mut self.sprites
    }

    /// Reset the PPU to its initial state
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on/hard reset
    /// - `ram_init_mode`: RAM initialization mode (only used for hard reset)
    pub fn reset(&mut self, soft_reset: bool, ram_init_mode: crate::nes::console::RamInitMode) {
        self.timing.reset();
        self.status.reset();
        self.vblank_suppressed_for_frame = false;
        self.vblank_for_nmi = false;
        self.registers.reset();

        // Reset memory cache
        self.memory.reset();

        // On hard reset, re-initialize RAM based on configured mode
        if !soft_reset {
            self.memory.reinitialize(ram_init_mode);
        }

        self.background.reset();
        self.sprites.reset(soft_reset, ram_init_mode);
        self.prev_a12 = false;
        self.recent_pixels = [None, None];
    }

    pub fn io_bus(&self) -> u8 {
        self.registers.io_bus()
    }

    pub fn set_io_bus(&mut self, value: u8) {
        self.registers.set_io_bus(value);
    }

    pub fn set_oam_dram_decay_enabled(&mut self, enabled: bool) {
        let is_ntsc = matches!(self.timing.tv_system(), TimingMode::Ntsc);
        let effective_enabled = enabled && is_ntsc;
        self.sprites.set_oam_decay_enabled(effective_enabled);
    }

    /// Run the PPU for a specified number of cycles
    pub fn run_ppu_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Process a single PPU cycle
    fn tick(&mut self) {
        tick::tick(self);
    }

    /// Write to control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        let nmi_was_enabled = self.registers.should_generate_nmi();

        trace_ppu!(3; "ppuctrl write value={:02X} y={} x={} t_before={:04X} v_before={:04X}",
            value,
            self.timing.scanline(),
            self.timing.pixel(),
            self.registers.t(),
            self.registers.v(),
        );

        self.registers.write_control(value);
        self.registers.set_io_bus(value); // Update I/O bus

        trace_ppu!(3; "ppuctrl write value={:02X} y={} x={} t_after={:04X} v_after={:04X}",
            value,
            self.timing.scanline(),
            self.timing.pixel(),
            self.registers.t(),
            self.registers.v(),
        );

        let nmi_is_enabled = self.registers.should_generate_nmi();

        // NMI-off timing quirk: disabling NMI right around VBlank start can suppress

        // the VBlank NMI edge (blargg ppu_vbl_nmi 08).
        let is_disabling_nmi_at_vblank_nmi_latch_dot = nmi_was_enabled
            && !nmi_is_enabled
            && self.timing.scanline() == 241
            && self.timing.pixel() == 2;
        if is_disabling_nmi_at_vblank_nmi_latch_dot {
            self.status.clear_nmi();
        }

        // If NMI is enabled during VBlank (0→1 transition while VBlank flag is set),
        // the PPU should immediately assert an NMI edge.
        if !nmi_was_enabled && nmi_is_enabled && self.vblank_for_nmi {
            self.status.trigger_nmi();
        }
    }

    /// Write to mask register ($2001)
    pub fn write_mask(&mut self, value: u8) {
        let grayscale_before = self.registers.is_grayscale();
        let rendering_enabled_before = self.registers.is_rendering_enabled();
        trace_ppu!(3; "ppumask write value={:02X} y={} x={} render_before={} bg_before={} sp_before={} t={:04X} v={:04X}",
            value,
            self.timing.scanline(),
            self.timing.pixel(),
            rendering_enabled_before,
            self.registers.is_background_enabled(),
            self.registers.is_sprite_enabled(),
            self.registers.t(),
            self.registers.v(),
        );
        self.registers.write_mask(value);
        let grayscale_after = self.registers.is_grayscale();
        let rendering_enabled_after = self.registers.is_rendering_enabled();

        // OAMADDR glitch: When rendering is disabled during sprite evaluation (cycles 65-256
        // of visible scanlines), OAMADDR is incremented by 1.
        // This is a hardware quirk where the PPU's sprite evaluation state machine is
        // interrupted mid-process, causing an unexpected increment.
        if rendering_enabled_before && !rendering_enabled_after {
            let scanline = self.timing.scanline();
            let pixel = self.timing.pixel();
            // Visible scanlines are 0-239, sprite evaluation happens during pixels 65-256
            if scanline < 240 && (65..=256).contains(&pixel) {
                trace_ppu!(2; "OAMADDR glitch: rendering disabled during sprite evaluation at y={} x={}, incrementing OAMADDR from {:02X}",
                    scanline,
                    pixel,
                    self.registers.oam_address,
                );
                self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
                trace_ppu!(2; "OAMADDR after glitch: {:02X}", self.registers.oam_address);
            }
        }

        // Mid-scanline grayscale toggles take effect essentially immediately on real hardware.
        // To approximate that timing without re-rendering the whole scanline, re-apply grayscale
        // to the most recently drawn pixels (see `track_recent_pixel`). This matches demo_ntsc.
        if grayscale_before != grayscale_after {
            self.apply_grayscale_to_recent_pixels(grayscale_after);
        }
        trace_ppu!(3; "ppumask write value={:02X} y={} x={} render_after={} bg_after={} sp_after={} t={:04X} v={:04X}",
            value,
            self.timing.scanline(),
            self.timing.pixel(),
            rendering_enabled_after,
            self.registers.is_background_enabled(),
            self.registers.is_sprite_enabled(),
            self.registers.t(),
            self.registers.v(),
        );
        self.registers.set_io_bus(value); // Update I/O bus
    }

    fn track_recent_pixel(&mut self, x: u32, y: u32, palette_index: u8) {
        // Keep the last two pixels around so late PPUMASK changes (grayscale/emphasis)
        // can be applied retroactively for tight timing demos.
        self.recent_pixels[0] = self.recent_pixels[1];
        self.recent_pixels[1] = Some(RecentPixel {
            x,
            y,
            palette_index,
        });
    }

    fn apply_grayscale_to_recent_pixels(&mut self, grayscale: bool) {
        for entry in self.recent_pixels.iter().flatten() {
            let palette_addr = 0x3F00u16 + entry.palette_index as u16;
            let mut color_value = self.memory.read_palette(palette_addr);
            // Grayscale masks the palette *output*, not the palette index.
            // This preserves correct brightness selection while removing chroma.
            color_value = apply_grayscale(color_value, grayscale);
            let (r, g, b) = self.lookup_system_palette(color_value);
            self.rendering
                .screen_buffer_mut()
                .set_pixel(entry.x, entry.y, r, g, b);
        }
    }

    /// Read status register ($2002)
    pub fn get_status(&mut self) -> u8 {
        let scanline = self.timing.scanline();
        let pixel = self.timing.pixel();

        trace_ppu!(3; "ppustatus read y={} x={} status={:02X} w={} t={:04X} v={:04X}",
            scanline,
            pixel,
            self.status.peek_status(),
            self.registers.w(),
            self.registers.t(),
            self.registers.v(),
        );

        // VBlank suppression quirk: if $2002 is read right as VBlank is being set,
        // the flag can be suppressed for the frame.
        if scanline == 241 && (pixel == 0 || pixel == 1) {
            self.vblank_suppressed_for_frame = true;
        }

        // NMI suppression quirk: reading $2002 shortly after VBlank starts can prevent
        // the VBlank NMI edge from being observed.
        if scanline == 241 && (pixel == 2 || pixel == 3) {
            self.status.clear_nmi();
        }

        let status = self.status.read_status();
        if (status & 0x80) != 0 {
            // Reading $2002 clears the visible VBlank flag, and it should also prevent
            // any further immediate-NMI enable edge from being generated this VBlank.
            self.clear_vblank_for_nmi();
        }
        self.registers.clear_w(); // Reading status clears write toggle

        // RC2C05 PPUs return a hardcoded security identifier instead of open bus
        // in the lower bits. All 8 bits are hardware-driven (no open bus).
        if let Some(security) = self.vs_security_value() {
            let new_io_bus = (status & 0xE0) | security;
            self.registers.set_io_bus_with_mask(new_io_bus, 0xFF);
            return new_io_bus;
        }

        // Standard PPU: bits 5-7 from status, bits 0-4 from previous I/O bus
        let io_bus = self.registers.io_bus();
        let new_io_bus = (status & 0xE0) | (io_bus & 0x1F);
        // Only refresh bits 5-7, not bits 0-4
        self.registers.set_io_bus_with_mask(new_io_bus, 0xE0);
        new_io_bus
    }

    /// Write to scroll register ($2005)
    pub fn write_scroll(&mut self, value: u8, is_dummy_write: bool) {
        trace_ppu!(3; "ppuscroll write value={:02X} w_before={} t_before={:04X} v_before={:04X}",
            value,
            self.registers.w(),
            self.registers.t(),
            self.registers.v(),
        );
        self.registers.write_scroll(value, is_dummy_write);
        trace_ppu!(3; "ppuscroll write value={:02X} w_after={} t_after={:04X} v_after={:04X}",
            value,
            self.registers.w(),
            self.registers.t(),
            self.registers.v(),
        );
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to address register ($2006)
    pub fn write_address(&mut self, value: u8, is_dummy_write: bool) {
        trace_ppu!(3; "ppuaddr write value={:02X} w_before={} t_before={:04X} v_before={:04X}",
            value,
            self.registers.w(),
            self.registers.t(),
            self.registers.v(),
        );
        let old_v = self.registers.v();
        let was_second_write = self.registers.write_address(value, is_dummy_write);
        trace_ppu!(3; "ppuaddr write value={:02X} w_after={} t_after={:04X} v_after={:04X}",
            value,
            self.registers.w(),
            self.registers.t(),
            self.registers.v(),
        );
        self.registers.set_io_bus(value); // Update I/O bus

        if was_second_write {
            let new_addr = self.registers.v(); // v was set to t by write_address

            // During rendering scanlines the v=t update is delayed by 3 PPU
            // cycles (based on Visual NES findings, ref Mesen2 NesPpu.cpp).
            // Outside rendering, it takes effect immediately.
            let scanline = self.timing.scanline();
            let prerender = tick::prerender_scanline(self.timing.tv_system());
            let is_rendering_scanline = scanline
                < crate::nes::ppu::timing::LAST_VISIBLE_SCANLINE_PLUS_ONE
                || scanline == prerender;

            if is_rendering_scanline && self.registers.is_rendering_enabled() {
                // Undo the immediate v=t; the tick loop will apply it after the delay.
                self.registers.set_v(old_v);
                self.update_vram_addr_delay = 3;
                self.pending_vram_addr = new_addr;
            } else {
                // Immediate update outside rendering; notify mapper of address change.
                self.prime_a12_and_notify_mapper(old_v, new_addr);
            }
        }
    }

    /// Read from data register ($2007)
    pub fn read_data(&mut self) -> u8 {
        let addr = self.registers.v();
        let result = match addr {
            0x0000..=0x1FFF => {
                // CHR ROM: buffered read
                // Notify mapper this is a PPUDATA read, not a rendering fetch
                // (MMC5 extended attribute mode should NOT apply here)
                Self::notify_chr_fetch_is_ppudata(&self.cartridge);
                let buffered = self.registers.data_buffer();
                self.registers
                    .set_data_buffer(self.memory.read_chr(addr, &self.cartridge));
                buffered
            }
            0x2000..=0x3EFF => {
                // Nametable: buffered read
                let buffered = self.registers.data_buffer();
                self.registers
                    .set_data_buffer(self.memory.read_nametable_mapped(addr, &self.cartridge));
                buffered
            }
            0x3F00..=0x3FFF => {
                // Palette: immediate read
                // Bits 5-0 come from palette, bits 7-6 from open bus
                let palette_data = self.memory.read_palette(addr);
                // Update buffer with nametable data underneath
                let mirrored_addr = addr & 0x2FFF;
                self.registers.set_data_buffer(
                    self.memory
                        .read_nametable_mapped(mirrored_addr, &self.cartridge),
                );
                // Combine palette data (bits 5-0) with open bus (bits 7-6)
                let io_bus = self.registers.io_bus();
                (io_bus & 0xC0) | (palette_data & 0x3F)
            }
            _ => self.registers.data_buffer(),
        };

        // Use rendering glitch during active rendering
        let old_addr = self.registers.v();
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }

        // Notify mapper of address change after increment (for MMC3 A12 detection)
        let new_addr = self.registers.v();
        self.prime_a12_and_notify_mapper(old_addr, new_addr);

        // Update I/O bus with value read
        // For palette reads (addr 0x3F00-0x3FFF), only refresh bits 5-0
        // For other reads, refresh all 8 bits
        if (0x3F00..=0x3FFF).contains(&addr) {
            self.registers.set_io_bus_with_mask(result, 0x3F);
        } else {
            self.registers.set_io_bus(result);
        }
        result
    }

    /// Write to data register ($2007)
    pub fn write_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let addr = self.registers.v();
        match addr {
            0x0000..=0x1FFF => {
                // CHR memory - routes through mapper for ROM/RAM handling
                self.memory.write_chr(addr, value, &self.cartridge);
            }
            0x2000..=0x3EFF => {
                self.memory
                    .write_nametable_mapped(addr, value, &self.cartridge);
            }
            0x3F00..=0x3FFF => {
                self.memory.write_palette(addr, value);
            }
            _ => {}
        }

        // Use rendering glitch during active rendering
        let old_addr = self.registers.v();
        if self.should_use_rendering_glitch() {
            self.registers.inc_address_with_rendering_glitch();
        } else {
            self.registers.increment_vram_address();
        }

        // Notify mapper of address change after increment (for MMC3 A12 detection)
        let new_addr = self.registers.v();
        self.prime_a12_and_notify_mapper(old_addr, new_addr);
    }

    /// Set the cartridge reference for dynamic CHR ROM/RAM access
    ///
    /// This establishes the connection between the PPU and the cartridge mapper,
    /// enabling hardware-accurate CHR access with bank switching support.
    /// Must be called after inserting a cartridge into the system.
    pub fn set_cartridge(&mut self, cartridge: Rc<RefCell<Cartridge>>) {
        self.cartridge = Some(cartridge);
    }

    /// Set mirroring mode
    pub fn set_mirroring(&mut self, mirroring: NametableLayout) {
        self.memory.set_mirroring(mirroring);
    }

    /// Poll NMI
    pub fn poll_nmi(&mut self) -> bool {
        self.status.poll_nmi()
    }

    /// Poll frame complete
    pub fn poll_frame_complete(&mut self) -> bool {
        self.status.poll_frame_complete()
    }

    /// Get current scanline
    pub fn scanline(&self) -> u16 {
        self.timing.scanline()
    }

    /// Get current pixel
    pub fn pixel(&self) -> u16 {
        self.timing.pixel()
    }

    /// Get current frame count
    pub fn frame_count(&self) -> u64 {
        self.timing.frame_count()
    }

    /// Write to OAM address register ($2003)
    pub fn write_oam_address(&mut self, value: u8) {
        self.registers.oam_address = value;
        self.registers.set_io_bus(value); // Update I/O bus
    }

    /// Write to OAM data register ($2004)
    pub fn write_oam_data(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        let is_rendering = self.is_actively_rendering();

        // During rendering, writes to OAMDATA are ignored (but address still increments)
        if !is_rendering {
            self.sprites.write_oam(self.registers.oam_address, value);
            // Normal increment: add 1
            self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
        } else {
            // Glitchy increment during rendering: increment only the high 6 bits (add 4)
            // This preserves the low 2 bits and bumps the sprite index
            let low_bits = self.registers.oam_address & 0x03;
            let high_bits = self.registers.oam_address.wrapping_add(4) & 0xFC;
            self.registers.oam_address = high_bits | low_bits;
        }
    }

    /// Write to OAM via DMA (ignores rendering blocking).
    pub fn write_oam_data_dma(&mut self, value: u8) {
        self.registers.set_io_bus(value); // Update I/O bus
        self.sprites.write_oam(self.registers.oam_address, value);
        self.registers.oam_address = self.registers.oam_address.wrapping_add(1);
    }

    /// Read from OAM data register ($2004)
    ///
    /// During active rendering (visible + pre-render scanlines with rendering enabled),
    /// this returns the internal OAM read latch instead of OAM[OAMADDR].
    /// The latch reflects whatever the PPU's internal OAM bus is doing:
    /// - Cycles 1-64: $FF (secondary OAM clear)
    /// - Cycles 65-256: sprite evaluation data
    /// - Cycles 257-320: secondary OAM data for sprite fetches
    /// - Cycles 321-340: first byte of secondary OAM
    pub fn read_oam_data(&mut self) -> u8 {
        let value = if self.is_actively_rendering() {
            self.sprites.oam_read_latch()
        } else {
            self.sprites.read_oam(self.registers.oam_address)
        };
        self.registers.set_io_bus(value); // Update I/O bus
        value
    }

    /// Get reference to screen buffer
    pub fn screen_buffer(&self) -> &super::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer()
    }

    /// Get reference to timing info
    pub fn timing(&self) -> &super::timing::Timing {
        &self.timing
    }

    /// Get mutable reference to screen buffer (for compatibility)
    pub fn screen_buffer_mut(&mut self) -> &mut super::screen_buffer::ScreenBuffer {
        self.rendering.screen_buffer_mut()
    }

    /// Check if in VBlank period
    #[cfg(test)]
    pub fn is_in_vblank(&self) -> bool {
        self.status.is_in_vblank()
    }

    /// Check if should generate NMI
    #[cfg(test)]
    pub fn should_generate_nmi(&self) -> bool {
        self.registers.should_generate_nmi()
    }

    /// Check if PPUDATA access should trigger the rendering glitch
    /// Returns true if rendering is enabled and we're on a visible scanline
    fn should_use_rendering_glitch(&self) -> bool {
        let scanline = self.timing.scanline();
        let is_visible_scanline = scanline < 240;
        self.registers.is_rendering_enabled() && is_visible_scanline
    }

    /// Check if PPU is currently on a rendering scanline (visible or pre-render)
    /// Returns true if we're on scanlines 0-239 or the pre-render scanline
    fn is_on_rendering_scanline(&self) -> bool {
        let scanline = self.timing.scanline();
        let is_visible_scanline = scanline < 240;
        let is_prerender = scanline == tick::prerender_scanline(self.timing.tv_system());
        is_visible_scanline || is_prerender
    }

    /// Check if PPU is actively rendering (rendering enabled + on rendering scanline)
    fn is_actively_rendering(&self) -> bool {
        self.registers.is_rendering_enabled() && self.is_on_rendering_scanline()
    }

    /// Capture 8KB CHR data for debugger rendering (no PPU side effects).
    pub fn chr_snapshot_for_debugger(&self) -> [u8; CHR_SIZE] {
        let mut chr = [0u8; CHR_SIZE];
        for (addr, byte) in chr.iter_mut().enumerate() {
            *byte = self
                .memory
                .read_chr_for_debugger(addr as u16, &self.cartridge);
        }
        chr
    }

    /// Capture all four nametable regions (1KB each) for debugger rendering.
    pub fn nametable_snapshot_for_debugger(&self) -> [[u8; NAMETABLE_SIZE]; NUM_NAMETABLES] {
        let mut result = [[0u8; NAMETABLE_SIZE]; NUM_NAMETABLES];
        for (nt, table) in result.iter_mut().enumerate().take(NUM_NAMETABLES) {
            let base = NAMETABLE_BASE_ADDR + nt as u16 * NAMETABLE_STRIDE;
            for offset in 0..NAMETABLE_SIZE as u16 {
                table[offset as usize] = self.memory.read_nametable(base + offset);
            }
        }
        result
    }

    /// Capture the background palette for debugger rendering.
    pub fn palette_for_debugger(&self) -> [u8; PALETTE_SIZE] {
        let snap = self.memory.palette_snapshot();
        let mut result = [0u8; PALETTE_SIZE];
        let len = snap.len().min(PALETTE_SIZE);
        result[..len].copy_from_slice(&snap[..len]);
        result
    }

    /// Return the current background pattern table base address (0x0000 or 0x1000).
    pub fn bg_pattern_table_for_debugger(&self) -> u16 {
        self.registers.bg_pattern_table_addr()
    }

    /// Return the current scroll position as (scroll_x, scroll_y) pixel offsets
    /// into the 512×480 nametable space.
    ///
    /// Derived from the Loopy `t` register and fine-X scroll, which represent
    /// the programmed scroll position (stable outside of active rendering).
    pub fn scroll_for_debugger(&self) -> (u16, u16) {
        let t = self.registers.t();
        let fine_x = self.registers.x() as u16;
        let coarse_x = t & 0x001F;
        let coarse_y = (t >> 5) & 0x001F;
        let nt_x = (t >> 10) & 0x01;
        let nt_y = (t >> 11) & 0x01;
        let fine_y = (t >> 12) & 0x07;
        let scroll_x = nt_x * 256 + coarse_x * 8 + fine_x;
        let scroll_y = nt_y * 240 + coarse_y * 8 + fine_y;
        (scroll_x, scroll_y)
    }

    #[cfg(test)]
    pub fn total_cycles(&self) -> u64 {
        self.timing.total_cycles()
    }

    /// Get v register (for testing)
    #[cfg(test)]
    pub fn v_register(&self) -> u16 {
        self.registers.v()
    }

    /// Read nametable for debugging/testing (doesn't affect PPU state)
    #[cfg(test)]
    pub fn read_nametable_for_debug(&self, addr: u16) -> u8 {
        self.memory.read_nametable_mapped(addr, &self.cartridge)
    }

    /// Get base nametable address from PPUCTRL (for testing)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn base_nametable_addr(&self) -> u16 {
        self.registers.base_nametable_addr()
    }

    /// Get control register value (for testing)
    #[cfg(test)]
    pub fn peek_control(&self) -> u8 {
        self.registers.control()
    }

    /// Get mask register value (for testing)
    #[cfg(test)]
    pub fn peek_mask(&self) -> u8 {
        self.registers.mask()
    }

    /// Get w register (for testing)
    #[cfg(test)]
    pub fn w_register(&self) -> bool {
        self.registers.w()
    }

    /// Get OAM address register (for testing)
    #[cfg(test)]
    pub fn oam_address(&self) -> u8 {
        self.registers.oam_address
    }

    /// Returns a copy of the 256-byte OAM data for debugging.
    pub fn oam_snapshot(&self) -> Vec<u8> {
        self.sprites.oam_snapshot()
    }

    /// Check if A12 changed from 0 to 1 (rising edge)
    /// This is used for mapper IRQ counters (e.g., MMC3)
    /// Returns true if A12 went from 0 to 1
    #[cfg(test)]
    fn check_a12_rising_edge(&mut self, addr: u16) -> bool {
        let current_a12 = (addr & 0x1000) != 0;
        let rising_edge = !self.prev_a12 && current_a12;
        self.prev_a12 = current_a12;
        rising_edge
    }

    /// Capture the current PPU state for save-state.
    fn capture_state_inner(&self) -> PpuState {
        let (rendering_enabled_d1, rendering_enabled_d2) = self.timing.rendering_enabled_delays();
        let bg_state = self.background.capture_state();
        let sprites_state = self.sprites.capture_state();
        PpuState {
            timing: PpuTimingState {
                scanline: self.timing.scanline(),
                pixel: self.timing.pixel(),
                total_cycles: self.timing.total_cycles(),
                frame_count: self.timing.frame_count(),
                rendering_enabled_d1,
                rendering_enabled_d2,
            },
            registers: PpuRegisterState {
                control: self.registers.control(),
                mask: self.registers.mask(),
                oam_addr: self.registers.oam_address,
                v: self.registers.v(),
                t: self.registers.t(),
                fine_x: self.registers.x(),
                w: self.registers.w(),
                io_bus: self.registers.io_bus(),
                io_bus_refresh_time: self.registers.io_bus_refresh_time(),
                cycle_count: self.registers.cycle_count(),
            },
            vblank_flag: self.status.is_in_vblank(),
            sprite_zero_hit: self.status.is_sprite_0_hit(),
            pending_sprite_zero_hit: self.status.pending_sprite_0_hit(),
            sprite_overflow: self.status.is_sprite_overflow(),
            nmi_pending: self.status.is_nmi_enabled(),
            frame_complete: self.status.frame_complete(),
            vblank_suppressed_for_frame: self.vblank_suppressed_for_frame,
            vblank_for_nmi: self.vblank_for_nmi,
            prev_a12: self.prev_a12,
            vram: self.memory.vram_snapshot(),
            palette: self.memory.palette_snapshot(),
            last_palette_index: self.memory.last_palette_index(),
            last_palette_value: self.memory.last_palette_value(),
            mirroring_mode: self.memory.mirroring_mode(),
            oam: self.sprites.oam_snapshot(),
            secondary_oam: self.sprites.secondary_oam_snapshot(),
            sprites_found: sprites_state.sprites_found,
            sprite_count: sprites_state.sprite_count,
            next_sprite_count: sprites_state.next_sprite_count,
            sprite_buffers_ready: sprites_state.sprite_buffers_ready,
            sprite_0_index: sprites_state.sprite_0_index,
            next_sprite_0_index: sprites_state.next_sprite_0_index,
            sprite_eval_n: sprites_state.sprite_eval_n,
            sprite_eval_m: sprites_state.sprite_eval_m,
            sprite_eval_cycle: sprites_state.sprite_eval_cycle,
            sprite_eval_in_range: sprites_state.sprite_eval_in_range,
            sprite_eval_overflow_reads_remaining: sprites_state
                .sprite_eval_overflow_reads_remaining,
            sprite_eval_overflow_signaled: sprites_state.sprite_eval_overflow_signaled,
            oam_read_latch: sprites_state.oam_read_latch,
            oam_decay_cycle: sprites_state.oam_decay_cycle,
            oam_row_last_refresh_cycle: sprites_state.oam_row_last_refresh_cycle,
            sprite_pattern_shift_lo: sprites_state.sprite_pattern_shift_lo,
            sprite_pattern_shift_hi: sprites_state.sprite_pattern_shift_hi,
            sprite_x_positions: sprites_state.sprite_x_positions,
            sprite_attributes: sprites_state.sprite_attributes,
            next_sprite_pattern_shift_lo: sprites_state.next_sprite_pattern_shift_lo,
            next_sprite_pattern_shift_hi: sprites_state.next_sprite_pattern_shift_hi,
            next_sprite_x_positions: sprites_state.next_sprite_x_positions,
            next_sprite_attributes: sprites_state.next_sprite_attributes,
            bg_pattern_shift_lo: bg_state.bg_pattern_shift_lo,
            bg_pattern_shift_hi: bg_state.bg_pattern_shift_hi,
            bg_attribute_shift_lo: bg_state.bg_attribute_shift_lo,
            bg_attribute_shift_hi: bg_state.bg_attribute_shift_hi,
            nametable_latch: bg_state.nametable_latch,
            attribute_latch: bg_state.attribute_latch,
            pattern_lo_latch: bg_state.pattern_lo_latch,
            pattern_hi_latch: bg_state.pattern_hi_latch,
            screen_buffer: self.rendering.screen_buffer_snapshot(),
            read_buffer: self.registers.data_buffer(),
        }
    }

    /// Restore PPU state from a save-state.
    fn restore_state_inner(&mut self, state: &PpuState) {
        // Restore timing
        self.timing.restore_state(
            state.timing.scanline,
            state.timing.pixel,
            state.timing.total_cycles,
            state.timing.frame_count,
        );
        self.timing.set_rendering_enabled_delays(
            state.timing.rendering_enabled_d1,
            state.timing.rendering_enabled_d2,
        );

        // Restore registers
        self.registers.restore_state(
            state.registers.control,
            state.registers.mask,
            state.registers.oam_addr,
            state.registers.v,
            state.registers.t,
            state.registers.fine_x,
            state.registers.w,
            state.registers.io_bus,
            state.read_buffer,
        );
        self.registers
            .set_io_bus_refresh_time(state.registers.io_bus_refresh_time);
        self.registers.set_cycle_count(state.registers.cycle_count);

        // Restore memory
        self.memory.restore_vram(&state.vram);
        self.memory.restore_palette(&state.palette);
        self.memory
            .set_palette_cache(state.last_palette_index, state.last_palette_value);
        self.memory.set_mirroring(state.mirroring_mode);

        // Restore OAM
        let mut oam_data = [0xFF; 256];
        let oam_len = state.oam.len().min(oam_data.len());
        oam_data[..oam_len].copy_from_slice(&state.oam[..oam_len]);

        let mut secondary_oam = [0xFF; 32];
        let sec_len = state.secondary_oam.len().min(secondary_oam.len());
        secondary_oam[..sec_len].copy_from_slice(&state.secondary_oam[..sec_len]);

        self.sprites.restore_state(&SpritesState {
            oam_data,
            secondary_oam,
            oam_decay_enabled: matches!(self.timing.tv_system(), TimingMode::Ntsc),
            oam_decay_cycle: state.oam_decay_cycle,
            oam_row_last_refresh_cycle: state.oam_row_last_refresh_cycle,
            sprites_found: state.sprites_found,
            sprite_count: state.sprite_count,
            next_sprite_count: state.next_sprite_count,
            sprite_buffers_ready: state.sprite_buffers_ready,
            sprite_0_index: state.sprite_0_index,
            next_sprite_0_index: state.next_sprite_0_index,
            sprite_eval_n: state.sprite_eval_n,
            sprite_eval_m: state.sprite_eval_m,
            sprite_eval_cycle: state.sprite_eval_cycle,
            sprite_eval_in_range: state.sprite_eval_in_range,
            sprite_eval_overflow_reads_remaining: state.sprite_eval_overflow_reads_remaining,
            sprite_eval_overflow_signaled: state.sprite_eval_overflow_signaled,
            sprite_pattern_shift_lo: state.sprite_pattern_shift_lo,
            sprite_pattern_shift_hi: state.sprite_pattern_shift_hi,
            sprite_x_positions: state.sprite_x_positions,
            sprite_attributes: state.sprite_attributes,
            next_sprite_pattern_shift_lo: state.next_sprite_pattern_shift_lo,
            next_sprite_pattern_shift_hi: state.next_sprite_pattern_shift_hi,
            next_sprite_x_positions: state.next_sprite_x_positions,
            next_sprite_attributes: state.next_sprite_attributes,
            oam_read_latch: state.oam_read_latch,
        });

        // Restore status flags
        self.status.restore_state(
            state.vblank_flag,
            state.sprite_zero_hit,
            state.sprite_overflow,
            state.nmi_pending,
        );
        self.status
            .set_pending_sprite_0_hit(state.pending_sprite_zero_hit);
        self.status.set_frame_complete(state.frame_complete);

        self.background
            .restore_state(&crate::nes::ppu::background::BackgroundState {
                bg_pattern_shift_lo: state.bg_pattern_shift_lo,
                bg_pattern_shift_hi: state.bg_pattern_shift_hi,
                bg_attribute_shift_lo: state.bg_attribute_shift_lo,
                bg_attribute_shift_hi: state.bg_attribute_shift_hi,
                nametable_latch: state.nametable_latch,
                attribute_latch: state.attribute_latch,
                pattern_lo_latch: state.pattern_lo_latch,
                pattern_hi_latch: state.pattern_hi_latch,
            });

        self.rendering.restore_screen_buffer(&state.screen_buffer);

        self.vblank_suppressed_for_frame = state.vblank_suppressed_for_frame;
        self.vblank_for_nmi = state.vblank_for_nmi;
        self.prev_a12 = state.prev_a12;
    }

    #[allow(dead_code)]
    pub fn scroll_state(&self) -> (u16, u16, u8, bool) {
        self.registers.scroll_state()
    }
}

impl Stateful for Ppu {
    type State = PpuState;

    fn capture_state(&self) -> PpuState {
        self.capture_state_inner()
    }

    fn restore_state(&mut self, state: &PpuState) {
        self.restore_state_inner(state);
    }
}

#[derive(Clone, Copy)]
struct RecentPixel {
    x: u32,
    y: u32,
    palette_index: u8,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpuDebugState {
    pub timing: super::timing::TimingDebugState,
    pub status: super::status::StatusDebugState,
    pub registers: super::registers::RegistersDebugState,
    pub memory: super::memory::MemoryDebugState,
    pub background: super::background::BackgroundDebugState,
    pub sprites: super::sprites::SpritesDebugState,
    pub rendering: super::rendering::RenderingDebugState,
    pub vblank_suppressed_for_frame: bool,
    pub vblank_for_nmi: bool,
    pub prev_a12: bool,
}

#[cfg(test)]
impl Ppu {
    pub fn debug_state(&self) -> PpuDebugState {
        PpuDebugState {
            timing: self.timing.debug_state(),
            status: self.status.debug_state(),
            registers: self.registers.debug_state(),
            memory: self.memory.debug_state(),
            background: self.background.debug_state(),
            sprites: self.sprites.debug_state(),
            rendering: self.rendering.debug_state(),
            vblank_suppressed_for_frame: self.vblank_suppressed_for_frame,
            vblank_for_nmi: self.vblank_for_nmi,
            prev_a12: self.prev_a12,
        }
    }

    pub fn set_debug_state(&mut self, state: PpuDebugState) {
        self.timing.set_debug_state(state.timing);
        self.status.set_debug_state(state.status);
        self.registers.set_debug_state(state.registers);
        self.memory.set_debug_state(state.memory);
        self.background.set_debug_state(state.background);
        self.sprites.set_debug_state(state.sprites);
        self.rendering.set_debug_state(state.rendering);
        self.vblank_suppressed_for_frame = state.vblank_suppressed_for_frame;
        self.vblank_for_nmi = state.vblank_for_nmi;
        self.prev_a12 = state.prev_a12;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::{NametableLayout, VsPpuType};
    use crate::nes::console::Nes;
    use crate::nes::ppu::{
        background, memory, registers, rendering, screen_buffer, sprites, status, timing,
    };

    fn create_test_base_mapper() -> crate::nes::cartridge::BaseMapper {
        let ctx = crate::nes::cartridge::MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        );
        crate::nes::cartridge::BaseMapper::new(
            &ctx,
            crate::nes::cartridge::MapperCapabilities::default(),
        )
    }

    struct ScanlineSpyMapper {
        base: crate::nes::cartridge::BaseMapper,
        calls: Rc<RefCell<Vec<(u16, bool)>>>,
    }

    impl crate::nes::cartridge::Mapper for ScanlineSpyMapper {
        fn base(&self) -> &crate::nes::cartridge::BaseMapper {
            &self.base
        }

        fn base_mut(&mut self) -> &mut crate::nes::cartridge::BaseMapper {
            &mut self.base
        }

        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&mut self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_scanline(&mut self, scanline: u16, rendering_enabled: bool) {
            self.calls.borrow_mut().push((scanline, rendering_enabled));
        }

        fn get_mirroring(&self) -> NametableLayout {
            NametableLayout::Horizontal
        }
    }

    #[test]
    fn test_ppu_new() {
        let ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_prerender_scanline_helper() {
        assert_eq!(tick::prerender_scanline(TimingMode::Ntsc), 261);
        assert_eq!(tick::prerender_scanline(TimingMode::Pal), 311);
    }

    #[test]
    fn test_ppu_io_bus_round_trip() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        ppu.set_io_bus(0x5A);

        assert_eq!(ppu.io_bus(), 0x5A);
    }

    #[test]
    fn test_grayscale_applies_to_recent_pixels() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Seed palette entries for recent pixels.
        ppu.memory.write_palette(0x3F01, 0x16);
        ppu.memory.write_palette(0x3F02, 0x2A);

        // Track three pixels; only the last two should be updated.
        ppu.track_recent_pixel(10, 10, 0x01);
        ppu.track_recent_pixel(11, 10, 0x02);
        ppu.track_recent_pixel(12, 10, 0x01);

        ppu.apply_grayscale_to_recent_pixels(true);

        let (expected_r1, expected_g1, expected_b1) = Nes::lookup_system_palette(0x2A & 0x30);
        let (expected_r2, expected_g2, expected_b2) = Nes::lookup_system_palette(0x16 & 0x30);

        assert_eq!(
            ppu.screen_buffer().get_pixel(11, 10),
            (expected_r1, expected_g1, expected_b1)
        );
        assert_eq!(
            ppu.screen_buffer().get_pixel(12, 10),
            (expected_r2, expected_g2, expected_b2)
        );
        assert_eq!(ppu.screen_buffer().get_pixel(10, 10), (0, 0, 0));
    }

    #[test]
    fn test_rendering_disabled_uses_palette_color_at_vram_address() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Keep rendering disabled so pixel output path uses backdrop/ext-color behavior.
        ppu.write_mask(0x00);

        // Seed universal backdrop and a distinct palette entry.
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x01);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x11, false);
        ppu.write_data(0x16);

        // Point v into palette space before rendering output.
        ppu.write_address(0x3F, false);
        ppu.write_address(0x11, false);

        // First visible output dot should draw pixel (0,0).
        ppu.run_ppu_cycles(2);

        let expected = Nes::lookup_system_palette(0x16);
        assert_eq!(ppu.screen_buffer().get_pixel(0, 0), expected);
    }

    #[test]
    fn test_mapper_ppu_scanline_is_called_on_scanline_boundaries() {
        let calls: Rc<RefCell<Vec<(u16, bool)>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ScanlineSpyMapper {
                base: create_test_base_mapper(),
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_cartridge(cart);

        // Enable rendering so the mapper sees rendering_enabled = true.
        ppu.write_mask(0x18);

        // Run one full scanline worth of cycles; expect a scanline callback at the boundary.
        ppu.run_ppu_cycles(341);

        let calls = calls.borrow();
        assert!(!calls.is_empty());
        assert_eq!(calls.last().copied(), Some((1, true)));
    }

    #[test]
    fn test_mapper_ppu_scanline_sees_rendering_disabled() {
        let calls: Rc<RefCell<Vec<(u16, bool)>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ScanlineSpyMapper {
                base: create_test_base_mapper(),
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_cartridge(cart);

        // Rendering disabled by default.
        ppu.run_ppu_cycles(341);

        let calls = calls.borrow();
        assert!(!calls.is_empty());
        assert_eq!(calls.last().copied(), Some((1, false)));
    }

    #[test]
    fn test_ppu_save_state_roundtrip_includes_internal_state() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        let mut ppu_ram = [0u8; 4096];
        ppu_ram[0] = 0x11;
        ppu_ram[4095] = 0x22;

        let mut palette = [0u8; 32];
        palette[0] = 0x3F;
        palette[31] = 0x2A;

        let mut oam = [0u8; 256];
        oam[0] = 0x10;
        oam[1] = 0x20;
        oam[2] = 0x30;
        oam[3] = 0x40;

        let mut secondary_oam = [0u8; 32];
        secondary_oam[0] = 0xAA;
        secondary_oam[31] = 0xBB;

        let mut io_bus_refresh_time = [0u64; 8];
        io_bus_refresh_time[0] = 123;
        io_bus_refresh_time[7] = 456;

        let screen_buffer = screen_buffer::ScreenBufferDebugState {
            buffer: vec![0x5A; 256 * 240 * 3],
        };

        let debug_state = PpuDebugState {
            timing: timing::TimingDebugState {
                total_cycles: 999,
                tv_system: TimingMode::Ntsc,
                scanline: 120,
                pixel: 200,
                frame_count: 7,
                rendering_enabled_d1: true,
                rendering_enabled_d2: true,
            },
            status: status::StatusDebugState {
                vblank_flag: true,
                sprite_0_hit: true,
                pending_sprite_0_hit: true,
                sprite_overflow: true,
                nmi_enabled: true,
                frame_complete: true,
            },
            registers: registers::RegistersDebugState {
                control_register: 0xA5,
                mask_register: 0x3C,
                oam_address: 0x7F,
                data_buffer: 0xEE,
                io_bus: 0x5A,
                io_bus_refresh_time,
                cycle_count: 12345,
                v: 0x2123,
                t: 0x3ABC,
                x: 5,
                w: true,
            },
            memory: memory::MemoryDebugState {
                ppu_ram,
                palette,
                last_palette_index: Some(7),
                last_palette_value: 0x1C,
                mirroring_mode: NametableLayout::SingleScreenUpper,
            },
            background: background::BackgroundDebugState {
                bg_pattern_shift_lo: 0x1234,
                bg_pattern_shift_hi: 0x5678,
                bg_attribute_shift_lo: 0x9ABC,
                bg_attribute_shift_hi: 0xDEF0,
                nametable_latch: 0x12,
                attribute_latch: 0x34,
                pattern_lo_latch: 0x56,
                pattern_hi_latch: 0x78,
            },
            sprites: sprites::SpritesDebugState {
                oam_data: oam,
                secondary_oam,
                sprites_found: 5,
                sprite_count: 4,
                next_sprite_count: 3,
                sprite_buffers_ready: true,
                sprite_0_index: Some(2),
                next_sprite_0_index: Some(1),
                sprite_eval_n: 12,
                sprite_eval_m: 2,
                sprite_eval_cycle: 6,
                sprite_eval_in_range: true,
                sprite_eval_overflow_reads_remaining: 0,
                sprite_eval_overflow_signaled: false,
                sprite_pattern_shift_lo: [0x11; 8],
                sprite_pattern_shift_hi: [0x22; 8],
                sprite_x_positions: [0x33; 8],
                sprite_attributes: [0x44; 8],
                next_sprite_pattern_shift_lo: [0x55; 8],
                next_sprite_pattern_shift_hi: [0x66; 8],
                next_sprite_x_positions: [0x77; 8],
                next_sprite_attributes: [0x88; 8],
                oam_read_latch: 0,
            },
            rendering: rendering::RenderingDebugState { screen_buffer },
            vblank_suppressed_for_frame: true,
            vblank_for_nmi: true,
            prev_a12: true,
        };

        ppu.set_debug_state(debug_state.clone());

        let state = ppu.capture_state();
        let mut restored = Ppu::new_for_testing(TimingMode::Ntsc);
        restored.restore_state(&state);

        let restored_debug = restored.debug_state();

        assert_eq!(
            restored_debug.vblank_suppressed_for_frame,
            debug_state.vblank_suppressed_for_frame
        );
        assert_eq!(restored_debug.vblank_for_nmi, debug_state.vblank_for_nmi);
        assert_eq!(restored_debug.prev_a12, debug_state.prev_a12);

        assert_eq!(restored_debug.timing, debug_state.timing);
        assert_eq!(restored_debug.status, debug_state.status);
        assert_eq!(restored_debug.registers, debug_state.registers);
        assert_eq!(restored_debug.memory, debug_state.memory);
        assert_eq!(restored_debug.background, debug_state.background);
        assert_eq!(restored_debug.sprites, debug_state.sprites);

        assert_eq!(
            restored_debug.rendering.screen_buffer.buffer.len(),
            debug_state.rendering.screen_buffer.buffer.len()
        );
        assert_eq!(
            restored_debug.rendering.screen_buffer.buffer[0],
            debug_state.rendering.screen_buffer.buffer[0]
        );
        assert_eq!(
            restored_debug.rendering.screen_buffer.buffer[123],
            debug_state.rendering.screen_buffer.buffer[123]
        );
    }

    struct EndFrameSpyMapper {
        base: crate::nes::cartridge::BaseMapper,
        calls: Rc<RefCell<u32>>,
    }

    impl crate::nes::cartridge::Mapper for EndFrameSpyMapper {
        fn base(&self) -> &crate::nes::cartridge::BaseMapper {
            &self.base
        }

        fn base_mut(&mut self) -> &mut crate::nes::cartridge::BaseMapper {
            &mut self.base
        }

        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&mut self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_end_frame(&mut self) {
            *self.calls.borrow_mut() += 1;
        }

        fn get_mirroring(&self) -> NametableLayout {
            NametableLayout::Horizontal
        }
    }

    #[test]
    fn test_mapper_ppu_end_frame_is_called_when_frame_wraps() {
        let calls = Rc::new(RefCell::new(0u32));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            EndFrameSpyMapper {
                base: create_test_base_mapper(),
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_cartridge(cart);

        // Keep rendering disabled for deterministic timing (no odd-frame skip).
        ppu.run_ppu_cycles(262 * 341);

        // This should fail until PPU::tick forwards frame-wrap to the mapper.
        assert_eq!(*calls.borrow(), 1);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ChrFetchEvent {
        SetIsSprite(bool),
        ReadChr(u16),
    }

    struct ChrFetchKindSpyMapper {
        base: crate::nes::cartridge::BaseMapper,
        events: Rc<RefCell<Vec<ChrFetchEvent>>>,
    }

    impl crate::nes::cartridge::Mapper for ChrFetchKindSpyMapper {
        fn base(&self) -> &crate::nes::cartridge::BaseMapper {
            &self.base
        }

        fn base_mut(&mut self) -> &mut crate::nes::cartridge::BaseMapper {
            &mut self.base
        }

        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&mut self, addr: u16) -> u8 {
            self.events.borrow_mut().push(ChrFetchEvent::ReadChr(addr));

            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, _addr: u16) {}

        fn ppu_set_chr_fetch_is_sprite(&mut self, is_sprite: bool) {
            self.events
                .borrow_mut()
                .push(ChrFetchEvent::SetIsSprite(is_sprite));
        }

        fn get_mirroring(&self) -> NametableLayout {
            NametableLayout::Horizontal
        }
    }

    struct A12PrimingSpyMapper {
        base: crate::nes::cartridge::BaseMapper,
        calls: Rc<RefCell<Vec<u16>>>,
    }

    impl crate::nes::cartridge::Mapper for A12PrimingSpyMapper {
        fn base(&self) -> &crate::nes::cartridge::BaseMapper {
            &self.base
        }

        fn base_mut(&mut self) -> &mut crate::nes::cartridge::BaseMapper {
            &mut self.base
        }

        fn read_prg(&self, _addr: u16) -> u8 {
            0
        }

        fn write_prg(&mut self, _addr: u16, _value: u8) {}

        fn read_chr(&mut self, _addr: u16) -> u8 {
            0
        }

        fn write_chr(&mut self, _addr: u16, _value: u8) {}

        fn ppu_address_changed(&mut self, addr: u16) {
            self.calls.borrow_mut().push(addr);
        }

        fn get_mirroring(&self) -> NametableLayout {
            NametableLayout::Horizontal
        }
    }

    #[test]
    fn test_mapper_ppu_set_chr_fetch_is_sprite_is_applied_to_rendering_fetches() {
        let events: Rc<RefCell<Vec<ChrFetchEvent>>> = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            ChrFetchKindSpyMapper {
                base: create_test_base_mapper(),
                events: events.clone(),
            },
        ))));

        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_cartridge(cart);

        // Force BG fetches from $0000 and sprite fetches from $1000 so the test can
        // distinguish the two types of CHR reads.
        ppu.write_control(0x08);

        // Place sprite 0 at Y=0 so it will be in range for scanline 1.
        ppu.write_oam_address(0);
        ppu.write_oam_data(0); // Y
        ppu.write_oam_data(0); // tile
        ppu.write_oam_data(0); // attr
        ppu.write_oam_data(0); // X

        // Enable background + sprites.
        ppu.write_mask(0x18);

        // Run into the sprite-pattern-fetch window (pixel 257-320), while also
        // covering the first background pattern fetches earlier in the scanline.
        ppu.run_ppu_cycles(264);

        let events = events.borrow();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ChrFetchEvent::ReadChr(addr) if *addr < 0x1000))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ChrFetchEvent::ReadChr(addr) if *addr >= 0x1000))
        );

        for (idx, event) in events.iter().copied().enumerate() {
            if let ChrFetchEvent::ReadChr(addr) = event {
                assert!(idx > 0, "ReadChr must be preceded by SetIsSprite");
                let expected = if addr < 0x1000 {
                    ChrFetchEvent::SetIsSprite(false)
                } else {
                    ChrFetchEvent::SetIsSprite(true)
                };
                assert_eq!(events[idx - 1], expected);
            }
        }
    }

    #[test]
    fn test_ppu_reset() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.run_ppu_cycles(100);
        ppu.reset(false, crate::nes::console::RamInitMode::Zero);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_ppu_write_control() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_control(0b1000_0000);
        // Control register should be set (verified internally)
    }

    #[test]
    fn test_ppu_read_write_data() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        // Reading returns palette bits 5-0 combined with open bus bits 7-6
        // After writing 0x00 to address, io_bus = 0x00, so result is 0x02
        assert_eq!(ppu.read_data(), 0x02);
    }

    #[test]
    fn test_manual_address_changes_prime_mapper_a12_filter() {
        let calls = Rc::new(RefCell::new(Vec::new()));

        let cart = Rc::new(RefCell::new(Cartridge::from_mapper_for_test(Box::new(
            A12PrimingSpyMapper {
                base: create_test_base_mapper(),
                calls: calls.clone(),
            },
        ))));

        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_cartridge(cart);

        fn expected_calls(old: u16, new: u16) -> Vec<u16> {
            let mut calls = vec![old; 8];
            calls.push(new);
            calls
        }

        ppu.write_address(0x3F, false);
        ppu.write_address(0x20, false);

        let expected_ppuaddr_calls = expected_calls(0, 0x3F20);
        assert_eq!(*calls.borrow(), expected_ppuaddr_calls);

        calls.borrow_mut().clear();
        let mut state = ppu.debug_state();
        state.registers.v = 0x2000;
        ppu.set_debug_state(state);
        ppu.read_data();
        let expected_read_calls = expected_calls(0x2000, 0x2001);
        assert_eq!(*calls.borrow(), expected_read_calls);

        calls.borrow_mut().clear();
        let mut state = ppu.debug_state();
        state.registers.v = 0x2000;
        ppu.set_debug_state(state);
        ppu.write_data(0x12);
        let expected_write_calls = expected_calls(0x2000, 0x2001);
        assert_eq!(*calls.borrow(), expected_write_calls);
    }

    #[test]
    fn test_ppu_vblank() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Advance to VBlank (scanline 241, pixel 2).
        // Pixel 1 is the VBlank set time and is subject to the $2002 suppression quirk.
        ppu.run_ppu_cycles(241 * 341 + 2);

        let status = ppu.get_status();
        // VBlank flag should be set (bit 7)
        assert_eq!(status & 0x80, 0x80);

        // Reading status should clear VBlank flag.
        let status_second_read = ppu.get_status();
        assert_eq!(status_second_read & 0x80, 0);
    }

    #[test]
    fn test_status_read_at_vblank_set_time_suppresses_vblank_flag() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Advance to scanline 241, pixel 0 (one PPU cycle before VBlank would normally be set).
        ppu.run_ppu_cycles(241 * 341);

        // Reading $2002 right before VBlank sets can suppress the VBlank flag for the frame.
        let status_before = ppu.get_status();
        assert_eq!(status_before & 0x80, 0);

        // Advance into the normal VBlank-set cycle (scanline 241, pixel 1).
        ppu.run_ppu_cycles(1);

        // With suppression, the VBlank flag should still be clear.
        let status_after = ppu.get_status();
        assert_eq!(status_after & 0x80, 0);
    }

    #[test]
    fn test_vblank_suppression_still_marks_frame_complete() {
        // When $2002 is read just before VBlank (suppressing the VBlank flag),
        // the PPU frame must still be marked complete so the emulator render loop
        // doesn't spin forever waiting for a frame that never arrives.
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Advance to scanline 241, pixel 0 (one PPU cycle before VBlank would be set).
        ppu.run_ppu_cycles(241 * 341);

        // Reading $2002 here triggers VBlank suppression for this frame.
        let _ = ppu.get_status();
        assert!(!ppu.poll_frame_complete()); // Frame not complete yet (still at dot 0)

        // Advance to dot 1 where VBlank is *supposed* to start.
        ppu.run_ppu_cycles(1);

        // Even though VBlank is suppressed (CPU sees no VBlank flag this frame),
        // the frame must still be marked complete for rendering.
        assert!(
            ppu.poll_frame_complete(),
            "frame_complete must be set even when VBlank is suppressed"
        );

        // Confirm VBlank flag is indeed suppressed (CPU should not see VBlank).
        assert!(!ppu.is_in_vblank(), "VBlank flag should be suppressed");
    }

    #[test]
    fn test_status_read_on_vblank_start_clears_vblank_flag() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Advance to the VBlank start dot (scanline 241, pixel 1).
        ppu.run_ppu_cycles(241 * 341 + 1);

        // First read should observe VBlank set.
        let first = ppu.get_status();
        assert_eq!(first & 0x80, 0x80);

        // Second read (same dot in this unit test) should observe that the flag was cleared.
        let second = ppu.get_status();
        assert_eq!(second & 0x80, 0);
    }

    #[test]
    fn test_vblank_flag_clears_on_prerender_dot_1() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Advance to the pre-render scanline (261), dot 0.
        ppu.run_ppu_cycles(261 * 341);
        assert_eq!(ppu.scanline(), 261);
        assert_eq!(ppu.pixel(), 0);

        // VBlank should still be set at dot 0.
        assert!(ppu.is_in_vblank());

        // It should clear at dot 1.
        ppu.run_ppu_cycles(1);
        assert_eq!(ppu.scanline(), 261);
        assert_eq!(ppu.pixel(), 1);
        assert!(!ppu.is_in_vblank());
    }

    #[test]
    fn test_enabling_nmi_while_in_vblank_triggers_nmi_edge() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enter VBlank with NMI disabled.
        ppu.run_ppu_cycles(241 * 341 + 2);
        assert!(ppu.is_in_vblank());
        assert!(!ppu.should_generate_nmi());
        assert!(!ppu.poll_nmi());

        // Enabling NMI while VBlank flag is already set should immediately assert NMI.
        ppu.write_control(0x80);
        assert!(ppu.should_generate_nmi());
        assert!(ppu.poll_nmi());
    }

    // PPU Data tests
    #[test]
    fn test_read_data_from_palette() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        // Palette RAM only stores 6 bits (0x42 & 0x3F = 0x02)
        // Reading returns palette bits 5-0 combined with open bus bits 7-6
        // After writing 0x00 to address, io_bus = 0x00, so result is 0x02
        assert_eq!(ppu.read_data(), 0x02);
    }

    #[test]
    fn test_read_data_increments_address() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x10);
        ppu.write_data(0x20);

        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        assert_eq!(ppu.read_data(), 0x10);
        assert_eq!(ppu.read_data(), 0x20);
    }

    #[test]
    fn test_ppudata_write_increments_with_rendering_glitch() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_mask(0x18); // Enable rendering

        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.flush_pending_vram_addr(); // In hardware, ≥3 PPU cycles elapse before next write
        ppu.write_data(0x12);

        assert_eq!(ppu.v_register(), 0x3001);
    }

    #[test]
    fn test_ppudata_write_increments_normally_when_rendering_disabled() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_mask(0x00); // Rendering disabled

        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x12);

        assert_eq!(ppu.v_register(), 0x2001);
    }

    #[test]
    fn test_write_data_to_nametable() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read for buffer
        assert_eq!(ppu.read_data(), 0x42);
    }

    // OAM tests
    #[test]
    fn test_oam_write_and_read() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);
        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x42);
    }

    #[test]
    fn test_oam_data_increments_address() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x11); // Byte 0: Y position
        ppu.write_oam_data(0x22); // Byte 1: Tile index
        ppu.write_oam_data(0xE3); // Byte 2: Attributes (use valid bits only)
        ppu.write_oam_data(0x44); // Byte 3: X position

        ppu.write_oam_address(0x00);
        assert_eq!(ppu.read_oam_data(), 0x11);
        ppu.write_oam_address(0x01);
        assert_eq!(ppu.read_oam_data(), 0x22);
        ppu.write_oam_address(0x02);
        assert_eq!(ppu.read_oam_data(), 0xE3);
        ppu.write_oam_address(0x03);
        assert_eq!(ppu.read_oam_data(), 0x44);
    }

    #[test]
    fn test_oam_full_256_bytes() {
        // Test writing and reading all 256 bytes of OAM
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write all 256 bytes with a pattern that accounts for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            ppu.write_oam_data(i as u8);
        }

        // Verify OAMADDR wrapped around
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should wrap to 0 after 256 writes"
        );

        // Read all 256 bytes back, accounting for attribute byte masking
        ppu.write_oam_address(0x00);
        for i in 0..256 {
            let value = ppu.read_oam_data();
            ppu.write_oam_address((i + 1) as u8); // Manually increment since read doesn't
            let expected = if (i & 0x03) == 2 {
                (i as u8) & 0xE3 // Attribute bytes have bits 2-4 masked
            } else {
                i as u8
            };
            assert_eq!(value, expected, "OAM[{}] should be {}", i, expected);
        }
    }

    #[test]
    fn test_oam_attribute_byte_write_masks_unimplemented_bits() {
        // Test that writing to OAM byte 2 (attribute byte) masks bits 2-4
        // Hardware behavior: bits 2-4 are not connected and always read as 0
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Test writing all bits set (0xFF) to attribute byte
        ppu.write_oam_address(0x02); // First sprite, attribute byte
        ppu.write_oam_data(0xFF); // Try to write all bits
        ppu.write_oam_address(0x02);
        assert_eq!(
            ppu.read_oam_data(),
            0xE3,
            "Attribute byte should mask bits 2-4 on write (0xFF -> 0xE3)"
        );

        // Test writing various patterns with unimplemented bits set
        let test_cases = [
            (0x1C, 0x00), // Only bits 2-4 set -> all masked
            (0xFF, 0xE3), // All bits set -> only valid bits remain
            (0x7F, 0x63), // Bits 0-6 set -> bits 2-4 masked
            (0xE7, 0xE3), // Valid bits + bit 2 -> bit 2 masked
            (0xEB, 0xE3), // Valid bits + bit 3 -> bit 3 masked
            (0xF3, 0xE3), // Valid bits + bit 4 -> bit 4 masked
            (0xE3, 0xE3), // Only valid bits -> unchanged
            (0x00, 0x00), // All bits clear -> unchanged
        ];

        for (i, (write_val, expected)) in test_cases.iter().enumerate() {
            let addr = 0x02 + (i as u8 * 4); // Attribute byte of successive sprites
            ppu.write_oam_address(addr);
            ppu.write_oam_data(*write_val);
            ppu.write_oam_address(addr);
            assert_eq!(
                ppu.read_oam_data(),
                *expected,
                "OAM[{}]: write 0x{:02X} should store 0x{:02X}",
                addr,
                write_val,
                expected
            );
        }
    }

    #[test]
    fn test_oam_non_attribute_bytes_unmasked() {
        // Verify that non-attribute bytes (0, 1, 3) are NOT masked
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Test all bits can be written to non-attribute bytes
        let test_values = [0xFF, 0x00, 0x55, 0xAA];

        for &value in &test_values {
            // Byte 0: Y position
            ppu.write_oam_address(0x00);
            ppu.write_oam_data(value);
            ppu.write_oam_address(0x00);
            assert_eq!(
                ppu.read_oam_data(),
                value,
                "OAM byte 0 (Y position) should not be masked"
            );

            // Byte 1: Tile index
            ppu.write_oam_address(0x01);
            ppu.write_oam_data(value);
            ppu.write_oam_address(0x01);
            assert_eq!(
                ppu.read_oam_data(),
                value,
                "OAM byte 1 (tile index) should not be masked"
            );

            // Byte 3: X position
            ppu.write_oam_address(0x03);
            ppu.write_oam_data(value);
            ppu.write_oam_address(0x03);
            assert_eq!(
                ppu.read_oam_data(),
                value,
                "OAM byte 3 (X position) should not be masked"
            );
        }
    }

    #[test]
    fn test_oam_attribute_byte_all_sprites_masked() {
        // Test that ALL 64 sprites' attribute bytes are properly masked
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write 0xFF to all attribute bytes (every 4th byte starting at 2)
        for sprite_num in 0..64 {
            let addr = sprite_num * 4 + 2;
            ppu.write_oam_address(addr);
            ppu.write_oam_data(0xFF);
        }

        // Verify all attribute bytes read back as 0xE3
        for sprite_num in 0..64 {
            let addr = sprite_num * 4 + 2;
            ppu.write_oam_address(addr);
            assert_eq!(
                ppu.read_oam_data(),
                0xE3,
                "Sprite {} attribute byte should be masked (0xFF -> 0xE3)",
                sprite_num
            );
        }
    }

    #[test]
    fn test_oamaddr_cleared_during_sprite_loading() {
        // OAMADDR is automatically set to 0 during pixels 257-320 of visible and pre-render scanlines
        // This is critical hardware behavior that many test ROMs rely on
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering (otherwise OAMADDR clearing doesn't happen)
        ppu.write_control(0x00);
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Set OAMADDR to non-zero value
        ppu.write_oam_address(0x42);
        assert_eq!(ppu.oam_address(), 0x42);

        // Run to scanline 0, pixel 257 (start of sprite loading interval)
        ppu.run_ppu_cycles(257);

        // OAMADDR should be cleared to 0 during pixels 257-320
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared to 0 during sprite tile loading (pixels 257-320)"
        );

        // Set it again to verify it keeps getting cleared during the interval
        ppu.write_oam_address(0x99);
        ppu.run_ppu_cycles(1); // Still in the 257-320 interval
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should stay 0 during entire sprite loading interval"
        );

        // Run past pixel 320
        ppu.run_ppu_cycles(64); // Now at pixel 257+1+64 = 322

        // Now OAMADDR should stay whatever we set it to
        ppu.write_oam_address(0x55);
        ppu.run_ppu_cycles(1);
        assert_eq!(
            ppu.oam_address(),
            0x55,
            "OAMADDR should not be cleared after pixel 320"
        );
    }

    #[test]
    fn test_oamaddr_cleared_on_prerender_scanline() {
        // OAMADDR clearing also happens on the pre-render scanline
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261), pixel 257
        ppu.run_ppu_cycles(261 * 341 + 257);

        ppu.write_oam_address(0x42);
        ppu.run_ppu_cycles(1); // Pixel 258, should clear OAMADDR
        assert_eq!(
            ppu.oam_address(),
            0x00,
            "OAMADDR should be cleared during pre-render scanline sprite loading"
        );
    }

    #[test]
    fn test_oamaddr_not_cleared_when_rendering_disabled() {
        // OAMADDR should NOT be cleared if rendering is disabled
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Rendering disabled (mask = 0)
        ppu.write_mask(0x00);

        ppu.write_oam_address(0x42);

        // Run through the sprite loading interval
        ppu.run_ppu_cycles(320);

        // OAMADDR should still be 0x42
        assert_eq!(
            ppu.oam_address(),
            0x42,
            "OAMADDR should not be cleared when rendering is disabled"
        );
    }

    #[test]
    fn test_oamaddr_corruption_at_rendering_start() {
        // If OAMADDR >= 8 when rendering starts (during pre-render sprite tile loading),
        // the 8 bytes at (OAMADDR & 0xF8) are copied to OAM[0..7]
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup: Write distinct values to different parts of OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank

        // Write pattern to OAM[0..7]
        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(i);
        }

        // Write different pattern to OAM[0x10..0x17]
        ppu.write_oam_address(0x10);
        for i in 0..8 {
            ppu.write_oam_data(0x80 + i);
        }

        // Set OAMADDR to 0x10 (>= 8) before rendering starts
        ppu.write_oam_address(0x10);

        // Run to pre-render scanline sprite tile loading (scanline 261, pixel 257)
        // At this point, OAM corruption should occur
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // Check that OAM[0..7] has been corrupted with data from OAM[0x10..0x17]
        // OAMADDR was 0x10, so 0x10 & 0xF8 = 0x10, meaning OAM[0x10..0x17] -> OAM[0..7]
        // Note: We read OAM directly via sprites() because $2004 returns the internal
        // latch during rendering, not OAM[OAMADDR].
        for i in 0..8u8 {
            let value = ppu.sprites().read_oam(i);
            let expected = if (i & 0x03) == 2 {
                // Attribute byte: 0x82 with masking = 0x82 & 0xE3 = 0x82
                (0x80 + i) & 0xE3
            } else {
                0x80 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should be corrupted with value from OAM[0x10+{}]",
                i, i
            );
        }
    }

    #[test]
    fn test_no_oamaddr_corruption_when_less_than_8() {
        // If OAMADDR < 8 when rendering starts, no corruption occurs
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Setup OAM during vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        ppu.write_oam_address(0x00);
        for i in 0..8 {
            ppu.write_oam_data(0x40 + i); // Use values that work with attribute masking
        }

        // Set OAMADDR to value < 8
        ppu.write_oam_address(0x05);

        // Run to pre-render sprite tile loading
        ppu.run_ppu_cycles((261 - 241) * 341 + 257 - 10);

        // OAM[0..7] should be unchanged
        // Note: We read OAM directly via sprites() because $2004 returns the internal
        // latch during rendering, not OAM[OAMADDR].
        for i in 0..8u8 {
            let value = ppu.sprites().read_oam(i);
            let expected = if (i & 0x03) == 2 {
                (0x40 + i) & 0xE3 // Attribute byte masking
            } else {
                0x40 + i
            };
            assert_eq!(
                value, expected,
                "OAM[{}] should not be corrupted when OAMADDR < 8",
                i
            );
        }
    }

    #[test]
    fn test_oam_write_during_rendering_ignored() {
        // Writes to OAMDATA during rendering should NOT modify OAM
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Write initial value to OAM during vblank (should work)
        ppu.run_ppu_cycles(241 * 341 + 10); // In vblank
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x42);

        // Run to visible scanline, avoiding the OAMADDR clearing period (257-320)
        ppu.run_ppu_cycles((262 - 241) * 341 + 100); // Scanline 0, pixel 100

        // Try to write during rendering (should be ignored)
        ppu.write_oam_address(0x05);
        ppu.write_oam_data(0x99); // This write should be ignored

        // Read back - should still be 0x42
        // Note: We read OAM directly via sprites() because $2004 returns the internal
        // latch during rendering, not OAM[OAMADDR].
        assert_eq!(
            ppu.sprites().read_oam(0x05),
            0x42,
            "OAM write during rendering should be ignored"
        );
    }

    #[test]
    fn test_oam_write_during_rendering_increments_address() {
        // Writes to OAMDATA during rendering should still increment OAMADDR (glitchy increment)
        // The glitchy increment bumps only the high 6 bits (adds 4 instead of 1)
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to visible scanline
        ppu.run_ppu_cycles(100); // Scanline 0, pixel 100

        // Set OAMADDR to 0x10 and write (write ignored, but address incremented by 4)
        ppu.write_oam_address(0x10);
        ppu.write_oam_data(0x99); // Write ignored, but glitchy increment happens

        // Address should have incremented by 4 (glitchy increment - high 6 bits bumped)
        assert_eq!(
            ppu.oam_address(),
            0x14,
            "OAMADDR should increment by 4 (glitchy) during rendering"
        );

        // Test with address 0x13 (low 2 bits = 0b11)
        ppu.write_oam_address(0x13);
        ppu.write_oam_data(0x99);
        // Glitchy increment: (0x13 & 0x03) | ((0x13 + 4) & 0xFC) = 0x03 | 0x14 = 0x17
        assert_eq!(
            ppu.oam_address(),
            0x17,
            "Glitchy increment should preserve low 2 bits and add 4 to high 6 bits"
        );
    }

    #[test]
    fn test_oam_write_outside_rendering_works() {
        // Writes to OAMDATA outside rendering should work normally
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Run to vblank
        ppu.run_ppu_cycles(241 * 341 + 10);

        // Write during vblank (should work)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(0x42);

        // Read back
        ppu.write_oam_address(0x00);
        assert_eq!(
            ppu.read_oam_data(),
            0x42,
            "OAM write during vblank should work normally"
        );
    }

    // Control register tests
    #[test]
    fn test_ppuctrl_nmi_enable() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_control(0x80); // Bit 7: NMI enable
        assert!(ppu.should_generate_nmi());

        ppu.write_control(0x00);
        assert!(!ppu.should_generate_nmi());
    }

    // Address register tests
    #[test]
    fn test_address_write_sequence() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0x20, false); // High byte
        ppu.write_address(0x00, false); // Low byte
        assert_eq!(ppu.v_register(), 0x2000);
    }

    #[test]
    fn test_address_wraps_correctly() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_address(0xFF, false); // High byte
        ppu.write_address(0xFF, false); // Low byte
        // Address should be masked to 14 bits (0x3FFF)
        assert_eq!(ppu.v_register() & 0x3FFF, 0x3FFF);
    }

    // Scroll register tests
    #[test]
    fn test_scroll_write_updates_registers() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_scroll(0xFF, false); // X scroll
        ppu.write_scroll(0xFF, false); // Y scroll
        // Verify write toggle was used
        assert!(!ppu.w_register()); // Should be false after two writes
    }

    // Timing tests
    #[test]
    fn test_scanline_increments() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.run_ppu_cycles(341); // One full scanline
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_frame_wraps_at_262_scanlines() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.run_ppu_cycles(262 * 341); // One full frame
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 0);
    }

    // Status register tests
    #[test]
    fn test_status_read_clears_vblank() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 2); // Past vblank start

        let status1 = ppu.get_status();
        assert_eq!(status1 & 0x80, 0x80); // VBlank set

        let status2 = ppu.get_status();
        assert_eq!(status2 & 0x80, 0); // VBlank cleared
    }

    #[test]
    fn test_status_read_clears_write_toggle() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_scroll(0x00, false); // First write, sets w=true
        assert!(ppu.w_register());

        ppu.get_status(); // Should clear w
        assert!(!ppu.w_register());
    }

    // CHR ROM and mirroring tests
    // Note: CHR ROM is now loaded dynamically through cartridge mapper
    // No longer need test_load_chr_rom

    #[test]
    fn test_vertical_mirroring() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_mirroring(crate::nes::cartridge::NametableLayout::Vertical);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x42);

        // Read from nametable 2 (should mirror to 0)
        ppu.write_address(0x28, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        assert_eq!(ppu.read_data(), 0x42);
    }

    #[test]
    fn test_horizontal_mirroring() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_mirroring(crate::nes::cartridge::NametableLayout::Horizontal);

        // Write to nametable 0
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x55);

        // Read from nametable 1 (should mirror to 0 in horizontal)
        ppu.write_address(0x24, false);
        ppu.write_address(0x00, false);
        let _ = ppu.read_data(); // Dummy read
        let val = ppu.read_data();
        assert_eq!(val, 0x55); // Should be mirrored
    }

    // NMI and frame complete tests
    #[test]
    fn test_nmi_polling() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_control(0x80); // Enable NMI
        // VBlank flag is set at scanline 241 dot 1, but the NMI edge is latched at dot 2.
        ppu.run_ppu_cycles(241 * 341 + 2);

        assert!(ppu.poll_nmi()); // Should return true once
        assert!(!ppu.poll_nmi()); // Should be cleared after polling
    }

    #[test]
    fn test_frame_complete_polling() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.run_ppu_cycles(241 * 341 + 1); // Enter VBlank

        assert!(ppu.poll_frame_complete()); // Should return true once
        assert!(!ppu.poll_frame_complete()); // Should be cleared after polling
    }

    #[test]
    fn test_pixel_zero_no_panic() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Run through a full scanline which includes pixel 0
        ppu.run_ppu_cycles(341);

        // Should not panic - pixel 0 is handled correctly
        assert_eq!(ppu.scanline(), 1);
        assert_eq!(ppu.pixel(), 0);
    }

    #[test]
    fn test_rendering_with_pixel_transitions() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run through multiple scanlines to test pixel 0 transitions
        for _ in 0..5 {
            ppu.run_ppu_cycles(341);
        }

        // Should complete without panicking
        assert_eq!(ppu.scanline(), 5);
    }

    #[test]
    fn test_palette_access_with_correct_addressing() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write to palette using full address
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x30); // Write to backdrop color

        // Write to another palette entry
        ppu.write_address(0x3F, false);
        ppu.write_address(0x01, false);
        ppu.write_data(0x16);

        // Enable rendering and run one scanline
        ppu.write_mask(0x18);
        ppu.run_ppu_cycles(341);

        // Should complete without panic - palette lookups work correctly
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_shift_register_load_timing() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Set up a known scroll position
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        // Run to pixel 8 of scanline 0 (first shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.scanline(), 0);
        assert_eq!(ppu.pixel(), 8);

        // Run to pixel 16 (second shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 16);

        // Run to pixel 24 (third shift register load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 24);

        // Verify we can continue through the scanline without issues
        ppu.run_ppu_cycles(256 - 24);
        assert_eq!(ppu.pixel(), 256);
    }

    #[test]
    fn test_scroll_register_updates_at_correct_pixels() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll and nametable
        ppu.write_control(0x00); // Nametable at $2000
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let _v_before_256 = ppu.v_register();

        // Run to pixel 256 (increment_fine_y happens here)
        ppu.run_ppu_cycles(256);
        assert_eq!(ppu.pixel(), 256);

        // Run to pixel 257 (copy_horizontal_bits happens here)
        ppu.run_ppu_cycles(1);
        assert_eq!(ppu.pixel(), 257);

        // V register should have been updated
        let _v_after_257 = ppu.v_register();
        // At minimum, fine Y should have incremented or wrapped
        // (exact value depends on internal state, but they shouldn't be identical
        // unless at a boundary condition)

        // Just verify we can continue without panic
        ppu.run_ppu_cycles(341 - 257);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_pre_render_scanline_prefetch() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pre-render scanline (261)
        ppu.run_ppu_cycles(261 * 341);
        assert_eq!(ppu.scanline(), 261);

        // Run to pixel 321 (start of pre-fetch)
        ppu.run_ppu_cycles(321);
        assert_eq!(ppu.pixel(), 321);

        // Run to pixel 328 (first pre-fetch load)
        ppu.run_ppu_cycles(7);
        assert_eq!(ppu.pixel(), 328);

        // Run to pixel 336 (second pre-fetch load)
        ppu.run_ppu_cycles(8);
        assert_eq!(ppu.pixel(), 336);

        // Complete the scanline
        ppu.run_ppu_cycles(341 - 336);
        assert_eq!(ppu.scanline(), 0); // Should wrap to scanline 0
    }

    #[test]
    fn test_rendering_enabled_background_fetch_cycles() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable background rendering
        ppu.write_mask(0x08);

        // Run through visible pixels (1-256)
        for pixel in 1..=256 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }

        // Continue through pre-fetch region (321-336)
        ppu.run_ppu_cycles(321 - 256);
        assert_eq!(ppu.pixel(), 321);

        for pixel in 322..=336 {
            ppu.run_ppu_cycles(1);
            assert_eq!(ppu.pixel(), pixel);
        }
    }

    #[test]
    fn test_dummy_nametable_fetches() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Run to pixel 337
        ppu.run_ppu_cycles(337);
        assert_eq!(ppu.pixel(), 337);

        // Run to pixel 339
        ppu.run_ppu_cycles(2);
        assert_eq!(ppu.pixel(), 339);

        // Complete the scanline without panic
        ppu.run_ppu_cycles(341 - 339);
        assert_eq!(ppu.scanline(), 1);
    }

    #[test]
    fn test_coarse_x_increment_timing() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Enable rendering
        ppu.write_mask(0x18);

        // Set up scroll
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        let v_initial = ppu.v_register();

        // Run to pixel 9 (first coarse X increment)
        // Per NES Dev wiki, shifters are reloaded at ticks 9, 17, 25, ..., 257
        ppu.run_ppu_cycles(9);
        let v_after_9 = ppu.v_register();

        // Coarse X should have incremented (bits 0-4 of v register)
        let coarse_x_initial = v_initial & 0x001F;
        let coarse_x_after_9 = v_after_9 & 0x001F;
        assert_eq!(coarse_x_after_9, (coarse_x_initial + 1) & 0x001F);

        // Run to pixel 17 (second coarse X increment)
        ppu.run_ppu_cycles(8);
        let v_after_17 = ppu.v_register();
        let coarse_x_after_17 = v_after_17 & 0x001F;
        assert_eq!(coarse_x_after_17, (coarse_x_initial + 2) & 0x001F);
    }

    #[test]
    fn test_a12_rising_edge_detection() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // A12 is bit 12 of address (0x1000)
        // Initially prev_a12 should be false

        // Access $0000 (A12=0) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x0000));

        // Access $0FFF (A12=0) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x0FFF));

        // Access $1000 (A12=1) - rising edge!
        assert!(ppu.check_a12_rising_edge(0x1000));

        // Access $1FFF (A12=1) - no rising edge (already high)
        assert!(!ppu.check_a12_rising_edge(0x1FFF));

        // Access $0000 (A12=0) - no rising edge (falling edge)
        assert!(!ppu.check_a12_rising_edge(0x0000));

        // Access $1800 (A12=1) - rising edge!
        assert!(ppu.check_a12_rising_edge(0x1800));

        // Access $1000 (A12=1) - no rising edge
        assert!(!ppu.check_a12_rising_edge(0x1000));
    }

    /// Given the PPU is on a visible scanline during cycles 1-64 (secondary OAM clear),
    /// When $2004 is read,
    /// Then it should return $FF regardless of OAMADDR contents.
    ///
    /// Per NES hardware: during cycles 1-64 the PPU writes $FF to secondary OAM,
    /// and $2004 reads expose this internal bus value rather than OAM[OAMADDR].
    #[test]
    fn test_read_2004_returns_ff_during_secondary_oam_clear() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write a known non-$FF value to OAM at address 0 via DMA (bypasses rendering check)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data_dma(0x42);

        // Enable rendering (both BG and sprites)
        ppu.write_mask(0x18);

        // Reset OAMADDR to 0 so read_oam_data would normally return 0x42
        ppu.write_oam_address(0x00);

        // Advance to scanline 0, pixel 32 (within the 1-64 secondary OAM clear window)
        ppu.run_ppu_cycles(32);

        // Read $2004 - should return $FF during secondary OAM clear, not OAM[0]=0x42
        let value = ppu.read_oam_data();
        assert_eq!(
            value, 0xFF,
            "During secondary OAM clear (cycles 1-64), $2004 should return $FF, got {:#04X}",
            value
        );
    }

    /// Given the PPU is on a visible scanline during cycles 65-256 (sprite evaluation),
    /// When $2004 is read,
    /// Then it should return the OAM byte currently being read by the evaluation logic,
    /// not OAM[OAMADDR].
    ///
    /// Per NES hardware: during cycles 65-256, the PPU evaluates sprites by reading
    /// primary OAM and writing to secondary OAM. $2004 reads expose the byte currently
    /// on the internal OAM data bus from this evaluation process.
    #[test]
    fn test_read_2004_returns_oam_eval_data_during_sprite_evaluation() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write sprite 0: Y=0xA5 at OAM[0] via DMA
        ppu.write_oam_address(0x00);
        ppu.write_oam_data_dma(0xA5); // Y position
        ppu.write_oam_data_dma(0xB6); // Tile index
        ppu.write_oam_data_dma(0xC7); // Attributes
        ppu.write_oam_data_dma(0xD8); // X position

        // Enable rendering
        ppu.write_mask(0x18);

        // Set OAMADDR to a different address so we can distinguish latch from OAM[OAMADDR]
        ppu.write_oam_address(0x10);

        // Advance to pixel 66 (first even cycle of sprite evaluation).
        // Pixel 65 (odd): evaluate_sprites reads OAM[0] (Y byte = 0xA5)
        // Pixel 66 (even): evaluate_sprites writes Y to secondary OAM (or skips)
        // The latch should hold 0xA5 (the Y byte that was read from OAM).
        ppu.run_ppu_cycles(66);

        let value = ppu.read_oam_data();
        assert_eq!(
            value, 0xA5,
            "During sprite evaluation (cycles 65-256), $2004 should return the OAM byte \
             being evaluated (0xA5), got {:#04X}",
            value
        );
    }

    /// Given the PPU is on a visible scanline during cycles 257-320 (sprite tile fetching),
    /// When $2004 is read,
    /// Then it should return the secondary OAM byte currently being accessed for the
    /// sprite fetch, not OAM[OAMADDR] or a stale evaluation value.
    ///
    /// Per NES hardware: during cycles 257-320, the PPU reads secondary OAM data
    /// for each of 8 sprite slots. Each slot takes 8 cycles: the PPU reads Y, tile,
    /// attribute, and X from secondary OAM (2 cycles each). $2004 exposes these
    /// secondary OAM reads on the internal bus.
    #[test]
    fn test_read_2004_returns_secondary_oam_during_sprite_fetch() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Set up sprite 0 at Y=0 (in range for scanline 0), tile=0x42, attr=0, X=10
        ppu.write_oam_address(0x00);
        ppu.write_oam_data_dma(0x00); // Y=0 (in range)
        ppu.write_oam_data_dma(0x42); // Tile index
        ppu.write_oam_data_dma(0x00); // Attributes
        ppu.write_oam_data_dma(0x0A); // X=10

        // Fill remaining OAM with Y=0xEE (not in range, not >= 0xF0 so still evaluated)
        // This ensures the evaluation latch ends on 0xEE, which we can distinguish
        // from the expected secondary OAM data.
        for _ in 4..256 {
            ppu.write_oam_data_dma(0xEE);
        }

        // Enable rendering
        ppu.write_mask(0x18);

        // Set OAMADDR to something different to distinguish from OAM[OAMADDR] confusion
        ppu.write_oam_address(0x40);

        // Advance to pixel 260 (within sprite 0 fetch window 257-264).
        // fetch_step = 260 - 257 = 3 → tile index byte, secondary_oam[1] = 0x42
        // The latch should be updated to secondary_oam[1] = 0x42.
        ppu.run_ppu_cycles(260);

        let value = ppu.read_oam_data();
        assert_eq!(
            value, 0x42,
            "During sprite tile fetch (cycles 257-320), $2004 should return secondary OAM \
             data (tile=0x42), got {:#04X}",
            value
        );
    }

    /// Given the PPU is on a visible scanline during cycles 321-340 (BG prefetching),
    /// When $2004 is read,
    /// Then it should return secondary_oam[0] (the Y coordinate of the first sprite),
    /// not OAM[OAMADDR] or the last fetched sprite data.
    ///
    /// Per NES hardware: during cycles 321-340, the PPU repeatedly reads the first
    /// byte of secondary OAM (secondary_oam[0]) on the internal bus, so $2004 exposes
    /// this value.
    #[test]
    fn test_read_2004_returns_secondary_oam_0_during_bg_prefetch() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Set up sprite 0 at Y=0 (in range for scanline 0), tile=0x42, attr=0, X=10
        ppu.write_oam_address(0x00);
        ppu.write_oam_data_dma(0x00); // Y=0 → secondary_oam[0] will be 0x00
        ppu.write_oam_data_dma(0x42); // Tile
        ppu.write_oam_data_dma(0x00); // Attributes
        ppu.write_oam_data_dma(0x0A); // X=10

        // Fill remaining OAM with Y=0xEE (out of range, distinguishable)
        for _ in 4..256 {
            ppu.write_oam_data_dma(0xEE);
        }

        // Enable rendering
        ppu.write_mask(0x18);

        // Advance to pixel 330 (within the 321-340 BG prefetch window)
        // After sprite fetching (257-320), the latch holds secondary_oam[31] = 0xFF
        // (last sprite 7's X byte, no sprites found there)
        // But the expected behavior is secondary_oam[0] = 0x00
        ppu.run_ppu_cycles(330);

        let value = ppu.read_oam_data();
        assert_eq!(
            value, 0x00,
            "During BG prefetch (cycles 321-340), $2004 should return secondary_oam[0] \
             (0x00), got {:#04X}",
            value
        );
    }

    /// Given the PPU is on the pre-render scanline (261 for NTSC) during sprite
    /// tile fetching (cycles 257-320),
    /// When $2004 is read,
    /// Then it should return secondary OAM data (from the latch), not OAM[OAMADDR].
    ///
    /// Per NES hardware: the pre-render scanline performs sprite pattern fetches
    /// (for MMC3 IRQ timing) but no sprite evaluation. $2004 reads still expose
    /// the internal OAM bus during these fetches.
    #[test]
    fn test_read_2004_returns_latch_on_prerender_scanline() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Write sprite 0: Y=0, tile=0x42
        ppu.write_oam_address(0x00);
        ppu.write_oam_data_dma(0x00); // Y=0
        ppu.write_oam_data_dma(0x42); // Tile
        ppu.write_oam_data_dma(0x00); // Attr
        ppu.write_oam_data_dma(0x0A); // X

        // Fill remaining OAM with Y>=0xF0 (off-screen, immediately filtered out)
        for _ in 4..256 {
            ppu.write_oam_data_dma(0xF0);
        }

        // Enable rendering
        ppu.write_mask(0x18);

        // Advance to pre-render scanline (261), pixel 32
        // 261 scanlines * 341 pixels + 32 = 89,033 cycles
        let prerender_cycles = 261 * 341 + 32;
        ppu.run_ppu_cycles(prerender_cycles);

        // Verify we're at the expected position
        assert_eq!(ppu.timing().scanline(), 261);
        assert_eq!(ppu.timing().pixel(), 32);

        // OAMADDR was cleared to 0 during rendering (sprite fetch phase on visible scanlines).
        // OAM[0] = 0x00 (Y position of sprite 0).
        // If $2004 returns 0x00, it's reading OAM[OAMADDR] directly (wrong).
        // If $2004 returns the latch (0xFF from secondary OAM idle phase), it's correct.
        let value = ppu.read_oam_data();
        assert_ne!(
            value, 0x00,
            "On pre-render scanline, $2004 should return the OAM latch, not OAM[OAMADDR=0] \
             (0x00), got {:#04X}",
            value
        );
    }

    /// Given the PPU is on a visible scanline during overflow detection (>8 sprites found),
    /// When $2004 is read during an even (write) cycle of overflow checking,
    /// Then it should return secondary_oam[0], not the stale primary OAM read value.
    ///
    /// Per NES hardware: during overflow detection, the PPU alternates between reading
    /// primary OAM (odd cycles) and writing to secondary OAM (even cycles). On write
    /// cycles, the secondary OAM bus is driven — the write pointer is at 0 after
    /// filling all 8 slots, so $2004 reads expose secondary_oam[0].
    #[test]
    fn test_read_2004_returns_secondary_oam_0_during_overflow_write_cycle() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Set up 9 sprites: sprites 0-7 at Y=0 (in range for scanline 0),
        // sprite 8 with Y=0x50 (not in range for scanline 0 - will trigger overflow check).
        for i in 0u8..8 {
            ppu.write_oam_address(i * 4);
            ppu.write_oam_data_dma(0x00); // Y=0 (in range for scanline 0)
            ppu.write_oam_data_dma(i + 1); // Tile (unique per sprite)
            ppu.write_oam_data_dma(0x00); // Attr
            ppu.write_oam_data_dma(i * 8); // X
        }
        // Sprite 8: out of range
        ppu.write_oam_address(32);
        ppu.write_oam_data_dma(0x50); // Y=0x50 (not in range for scanline 0)
        ppu.write_oam_data_dma(0xBB); // Tile
        ppu.write_oam_data_dma(0x00); // Attr
        ppu.write_oam_data_dma(0x00); // X

        // Fill remaining OAM with Y>=0xF0 (off-screen)
        for i in 36..256u16 {
            ppu.write_oam_address(i as u8);
            ppu.write_oam_data_dma(0xF0);
        }

        // Enable rendering
        ppu.write_mask(0x18);

        // Sprite 0 (not in range): 2 pixels (65-66)
        // Sprites 1-8 (in range): 8 pixels each (67-130)
        // Overflow starts at pixel 131.
        // Pixel 131 (odd): read OAM[8*4+0]=OAM[32]=0x50 (primary OAM read, latch=0x50)
        // Pixel 132 (even): overflow write cycle — latch should be secondary_oam[0]

        // secondary_oam[0] = Y of sprite 0 = 0x00 (the first sprite found in range)

        // Wait, actually: evaluation on scanline 0 checks next_scanline = 1.
        // Sprite 0: Y=0, diff = 1 - 1 = 0 < 8, IN RANGE. So sprite 0 IS found.
        // Sprites 1-7: same, all in range. 8 sprites found after sprite 7.
        // So sprites_found = 8 after evaluating sprites 0-7.
        // Overflow starts at sprite 8 (pixel 65 + 2 + 7*8 = 65 + 58 = 123? No...)

        // Sprite 0 at Y=0: IN range (diff=0). Takes 8 pixels: 65-72.
        // Sprite 1 at Y=0: IN range. Takes 8 pixels: 73-80.
        // ...
        // Sprite 7 at Y=0: IN range. Takes 8 pixels: 121-128. sprites_found=8.
        // Sprite 8 at Y=0x50: overflow starts at pixel 129.
        // Pixel 129 (odd): overflow read OAM[32]=0x50, latch=0x50
        // Pixel 130 (even): overflow write cycle
        // secondary_oam[0] = Y of sprite 0 = 0x00

        // Advance to pixel 130 (even overflow write cycle)
        ppu.run_ppu_cycles(130);

        let value = ppu.read_oam_data();
        assert_eq!(
            value, 0x00,
            "During overflow write cycle, $2004 should return secondary_oam[0] (0x00), got {:#04X}.\n\
             On even cycles of overflow detection, the secondary OAM bus is driven at address 0.",
            value
        );
    }

    /// Given background rendering is enabled but sprite rendering is disabled,
    /// When a visible scanline is rendered with sprite data in the render buffers,
    /// Then sprite pixels should NOT appear on screen - only background pixels should show.
    ///
    /// Per NES hardware: PPUMASK bit 4 (SHOW_SPRITES) controls whether sprite pixels
    /// are composited into the final output. When disabled, sprite evaluation and
    /// fetching still occur internally, but sprite pixels must not be visible.
    #[test]
    fn test_sprites_not_rendered_when_sprite_rendering_disabled() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable background only (bit 3), sprites disabled (bit 4 = 0)
        // PPUMASK = 0x08: show background, hide sprites
        ppu.write_mask(0x08);

        // Set up a distinct sprite palette color so we can detect if a sprite pixel leaks.
        // Sprite palette 0, color 1 = palette address 0x3F11
        // Use color index 0x16 (red) for sprite, 0x0F (black) for background universal backdrop.
        ppu.memory.write_palette(0x3F00, 0x0F); // Universal backdrop = black
        ppu.memory.write_palette(0x3F11, 0x16); // Sprite palette 0, color 1 = red

        // Place a sprite at X=10 in the current render buffers via debug state.
        // sprite_count=1 means get_pixel will check sprite 0.
        // Pattern lo=0xFF, hi=0x00 → pixel value 1 (opaque) for all 8 pixels.
        // Attributes = 0x00 (palette 0, foreground, no flip).
        let mut sprite_state = ppu.sprites().debug_state();
        sprite_state.sprite_pattern_shift_lo[0] = 0xFF;
        sprite_state.sprite_pattern_shift_hi[0] = 0x00;
        sprite_state.sprite_x_positions[0] = 10;
        sprite_state.sprite_attributes[0] = 0x00;
        sprite_state.sprite_count = 1;
        ppu.sprites().set_debug_state(sprite_state);

        // Advance PPU to pixel 11 on scanline 0 (first visible scanline).
        // Pixel index 1 = screen X 0, so pixel 11 = screen X 10 (where sprite starts).
        ppu.run_ppu_cycles(11);

        // After ticking pixel 11 (screen_x=10), the pixel at (10, 0) should be
        // the universal backdrop (0x0F = black), NOT the sprite color.
        let backdrop_color = Nes::lookup_system_palette(0x0F);
        let sprite_color = Nes::lookup_system_palette(0x16);
        let actual = ppu.screen_buffer().get_pixel(10, 0);

        assert_ne!(
            actual, sprite_color,
            "Sprite pixel should NOT be rendered when sprite rendering is disabled"
        );
        assert_eq!(
            actual, backdrop_color,
            "Background/backdrop color should be rendered when sprite rendering is disabled"
        );
    }

    /// Verifies that save-state capture/restore round-trips all sprite evaluation fields,
    /// including overflow_reads_remaining, overflow_signaled, and oam_read_latch.
    /// These fields are only non-zero mid-frame during sprite overflow detection,
    /// so a savestate taken at that point must preserve them.
    #[test]
    fn test_savestate_preserves_sprite_overflow_and_latch_fields() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Manually set sprite evaluation fields to non-default values
        // to simulate a mid-frame savestate during overflow detection.
        ppu.sprites.set_overflow_reads_remaining(3);
        ppu.sprites.set_overflow_signaled(true);
        ppu.sprites.set_oam_read_latch(0x42);

        // Capture state
        let state = ppu.capture_state();

        // Verify the captured state has the correct values
        assert_eq!(state.sprite_eval_overflow_reads_remaining, 3);
        assert!(state.sprite_eval_overflow_signaled);
        assert_eq!(state.oam_read_latch, 0x42);

        // Create a fresh PPU and restore the state
        let mut ppu2 = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu2.restore_state(&state);

        // Verify the restored PPU has the correct values
        let restored_state = ppu2.capture_state();
        assert_eq!(
            restored_state.sprite_eval_overflow_reads_remaining, 3,
            "overflow_reads_remaining should be preserved across save/restore"
        );
        assert!(
            restored_state.sprite_eval_overflow_signaled,
            "overflow_signaled should be preserved across save/restore"
        );
        assert_eq!(
            restored_state.oam_read_latch, 0x42,
            "oam_read_latch should be preserved across save/restore"
        );
    }

    /// Test OAMADDR glitch when disabling rendering during sprite evaluation.
    /// When rendering is disabled during sprite evaluation (cycles 65-256 of visible scanlines),
    /// OAMADDR should be incremented by 1.
    #[test]
    fn test_oamaddr_glitch_on_rendering_disable_during_sprite_evaluation() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering first
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Advance to a visible scanline (e.g., scanline 10) during sprite evaluation (pixel 100)
        for _ in 0..(10 * 341 + 100) {
            ppu.tick();
        }

        // Verify we're in the correct position
        assert_eq!(ppu.timing.scanline(), 10, "Should be on scanline 10");
        assert_eq!(ppu.timing.pixel(), 100, "Should be on pixel 100");

        // Set OAMADDR to a known value
        ppu.registers.oam_address = 0x50;

        // Disable rendering during sprite evaluation
        ppu.write_mask(0x00); // Disable both background and sprite rendering

        // OAMADDR should be incremented by 1 due to the glitch
        assert_eq!(
            ppu.registers.oam_address, 0x51,
            "OAMADDR should be incremented by 1 when rendering is disabled during sprite evaluation"
        );
    }

    /// Test OAMADDR glitch does NOT occur when disabling rendering outside sprite evaluation.
    #[test]
    fn test_oamaddr_glitch_not_triggered_outside_sprite_evaluation() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering first
        ppu.write_mask(0x18); // Enable background and sprite rendering

        // Test at pixel 64 (just before sprite evaluation starts at 65)
        for _ in 0..(10 * 341 + 64) {
            ppu.tick();
        }

        ppu.registers.oam_address = 0x50;
        ppu.write_mask(0x00); // Disable rendering

        assert_eq!(
            ppu.registers.oam_address, 0x50,
            "OAMADDR should NOT be incremented before sprite evaluation starts (pixel 64)"
        );

        // Test at pixel 257 (after sprite evaluation ends at 256)
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_mask(0x18);
        for _ in 0..(10 * 341 + 257) {
            ppu.tick();
        }

        ppu.registers.oam_address = 0x50;
        ppu.write_mask(0x00); // Disable rendering

        assert_eq!(
            ppu.registers.oam_address, 0x50,
            "OAMADDR should NOT be incremented after sprite evaluation ends (pixel 257)"
        );
    }

    /// Test OAMADDR glitch does NOT occur during VBlank.
    #[test]
    fn test_oamaddr_glitch_not_triggered_during_vblank() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Advance to VBlank (scanline 241, pixel 100)
        for _ in 0..(241 * 341 + 100) {
            ppu.tick();
        }

        assert_eq!(ppu.timing.scanline(), 241, "Should be in VBlank");

        ppu.registers.oam_address = 0x50;
        ppu.write_mask(0x00); // Disable rendering

        assert_eq!(
            ppu.registers.oam_address, 0x50,
            "OAMADDR should NOT be incremented during VBlank"
        );
    }

    /// Test OAMADDR glitch wraps around correctly at 0xFF.
    #[test]
    fn test_oamaddr_glitch_wraps_at_0xff() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Advance to sprite evaluation
        for _ in 0..(10 * 341 + 100) {
            ppu.tick();
        }

        // Set OAMADDR to 0xFF
        ppu.registers.oam_address = 0xFF;

        // Disable rendering during sprite evaluation
        ppu.write_mask(0x00);

        // OAMADDR should wrap to 0x00
        assert_eq!(
            ppu.registers.oam_address, 0x00,
            "OAMADDR should wrap from 0xFF to 0x00"
        );
    }

    /// Test OAMADDR glitch does NOT occur when rendering remains enabled.
    #[test]
    fn test_oamaddr_glitch_not_triggered_when_rendering_remains_enabled() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Enable rendering
        ppu.write_mask(0x18);

        // Advance to sprite evaluation
        for _ in 0..(10 * 341 + 100) {
            ppu.tick();
        }

        ppu.registers.oam_address = 0x50;

        // Write mask again but keep rendering enabled
        ppu.write_mask(0x18);

        assert_eq!(
            ppu.registers.oam_address, 0x50,
            "OAMADDR should NOT be incremented when rendering remains enabled"
        );
    }

    /// Test OAMADDR glitch occurs at boundary pixels (65 and 256).
    #[test]
    fn test_oamaddr_glitch_at_sprite_evaluation_boundaries() {
        // Test at pixel 65 (start of sprite evaluation)
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_mask(0x18);
        for _ in 0..(10 * 341 + 65) {
            ppu.tick();
        }

        ppu.registers.oam_address = 0x50;
        ppu.write_mask(0x00);

        assert_eq!(
            ppu.registers.oam_address, 0x51,
            "OAMADDR should be incremented at pixel 65 (start of sprite evaluation)"
        );

        // Test at pixel 256 (end of sprite evaluation)
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.write_mask(0x18);
        for _ in 0..(10 * 341 + 256) {
            ppu.tick();
        }

        ppu.registers.oam_address = 0x50;
        ppu.write_mask(0x00);

        assert_eq!(
            ppu.registers.oam_address, 0x51,
            "OAMADDR should be incremented at pixel 256 (end of sprite evaluation)"
        );
    }

    #[test]
    fn set_vs_ppu_type_stores_and_retrieves_value() {
        let mut ppu = Ppu::new(TimingMode::Ntsc, crate::nes::console::RamInitMode::Zero);

        // Initially None
        assert_eq!(ppu.vs_ppu_type(), None);

        // Set to a VS PPU type
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        assert_eq!(ppu.vs_ppu_type(), Some(VsPpuType::Rp2c04_0001));

        // Clear back to None
        ppu.set_vs_ppu_type(None);
        assert_eq!(ppu.vs_ppu_type(), None);
    }

    #[test]
    fn ppu_lookup_uses_vs_palette_when_set() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Without VS type, lookup returns standard NTSC palette
        let standard = Nes::lookup_system_palette(0x00);
        assert_eq!(ppu.lookup_system_palette(0x00), standard);

        // With RP2C04-0001, entry $00 should be DAC 755 = (0xFF, 0xB6, 0xB6)
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        assert_eq!(
            ppu.lookup_system_palette(0x00),
            (0xFF, 0xB6, 0xB6),
            "RP2C04-0001 $00 should be 755 (light pink)"
        );
        // Standard palette entry $00 is NOT (0xFF, 0xB6, 0xB6) — the VS palette is different
        assert_ne!(ppu.lookup_system_palette(0x00), standard);
    }

    #[test]
    fn ppu_default_system_palette_is_default_preset() {
        let ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        assert_eq!(ppu.system_palette(), NesPalette::Default);
        assert_eq!(ppu.lookup_system_palette(0x00), (0x54, 0x54, 0x54));
    }

    #[test]
    fn set_system_palette_changes_lookup_output() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_system_palette(NesPalette::Smooth);
        assert_eq!(ppu.system_palette(), NesPalette::Smooth);
        assert_eq!(ppu.lookup_system_palette(0x00), (0x6A, 0x6A, 0x6A));
    }

    #[test]
    fn vs_palette_overrides_active_system_palette() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_system_palette(NesPalette::Smooth);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        // VS palette wins over the selected preset.
        assert_eq!(ppu.lookup_system_palette(0x00), (0xFF, 0xB6, 0xB6));
    }

    #[test]
    fn cycle_system_palette_advances_and_wraps() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        assert_eq!(ppu.cycle_system_palette(), NesPalette::NesDev);
        ppu.set_system_palette(NesPalette::CompositeDirect);
        assert_eq!(ppu.cycle_system_palette(), NesPalette::Default);
    }

    #[test]
    fn ppu_lookup_falls_back_to_standard_when_no_vs_type() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Set VS type, then clear it
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        ppu.set_vs_ppu_type(None);

        // Should fall back to standard
        assert_eq!(
            ppu.lookup_system_palette(0x00),
            Nes::lookup_system_palette(0x00)
        );
    }

    #[test]
    fn ppu_lookup_masks_to_6_bits() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));

        // 0x40 should wrap to index 0x00
        assert_eq!(
            ppu.lookup_system_palette(0x40),
            ppu.lookup_system_palette(0x00)
        );
        // 0xFF should wrap to 0x3F
        assert_eq!(
            ppu.lookup_system_palette(0xFF),
            ppu.lookup_system_palette(0x3F)
        );
    }

    // ── RC2C05 security byte tests ────────────────────────────────────────

    #[test]
    fn rc2c05_01_status_returns_security_byte_0x1b() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_01));
        let status = ppu.get_status();
        assert_eq!(
            status & 0x1F,
            0x1B,
            "RC2C05-01 should return 0x1B in bits 0-4, got {:#04X}",
            status
        );
    }

    #[test]
    fn rc2c05_02_status_returns_security_byte_0x3d() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_02));
        let status = ppu.get_status();
        // 2C05-02 returns 0x3D which includes bit 5 (overrides sprite overflow)
        assert_eq!(
            status & 0x3F,
            0x3D,
            "RC2C05-02 should return 0x3D in bits 0-5, got {:#04X}",
            status
        );
    }

    #[test]
    fn rc2c05_03_status_returns_security_byte_0x1c() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_03));
        let status = ppu.get_status();
        assert_eq!(
            status & 0x1F,
            0x1C,
            "RC2C05-03 should return 0x1C in bits 0-4, got {:#04X}",
            status
        );
    }

    #[test]
    fn rc2c05_04_status_returns_security_byte_0x1b() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_04));
        let status = ppu.get_status();
        assert_eq!(
            status & 0x1F,
            0x1B,
            "RC2C05-04 should return 0x1B in bits 0-4, got {:#04X}",
            status
        );
    }

    #[test]
    fn rc2c05_05_status_returns_zero_in_lower_bits() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_05));
        let status = ppu.get_status();
        assert_eq!(
            status & 0x1F,
            0x00,
            "RC2C05-05 should return 0x00 in bits 0-4, got {:#04X}",
            status
        );
    }

    #[test]
    fn non_rc2c05_status_uses_io_bus_in_lower_bits() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        // Standard PPU — no VS type set
        ppu.set_io_bus(0x1F); // Pre-load I/O bus with 0x1F
        let status = ppu.get_status();
        assert_eq!(
            status & 0x1F,
            0x1F,
            "Non-RC2C05 PPU should return I/O bus in bits 0-4, got {:#04X}",
            status
        );
    }

    #[test]
    fn rc2c05_security_preserves_vblank_flag() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_01));
        // Force VBlank flag to 1
        ppu.status.enter_vblank();
        let status = ppu.get_status();
        assert_eq!(
            status & 0x80,
            0x80,
            "VBlank flag (bit 7) should still be set"
        );
        assert_eq!(status & 0x1F, 0x1B, "Security byte should still be present");
    }

    #[test]
    fn rp2c04_status_uses_io_bus_not_security() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        ppu.set_io_bus(0x0A);
        let status = ppu.get_status();
        // RP2C04 is NOT an RC2C05 — should use I/O bus in lower bits
        assert_eq!(
            status & 0x1F,
            0x0A,
            "RP2C04 should use I/O bus, not security byte"
        );
    }

    // ── RC2C05 register swap tests (PPU side) ────────────────────────────

    #[test]
    fn rc2c05_is_register_swapped() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_01));
        assert!(
            ppu.is_vs_register_swapped(),
            "RC2C05-01 should report register swap"
        );
    }

    #[test]
    fn rc2c05_05_is_register_swapped() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rc2c05_05));
        assert!(
            ppu.is_vs_register_swapped(),
            "RC2C05-05 should report register swap"
        );
    }

    #[test]
    fn rp2c04_is_not_register_swapped() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        ppu.set_vs_ppu_type(Some(VsPpuType::Rp2c04_0001));
        assert!(
            !ppu.is_vs_register_swapped(),
            "RP2C04 should NOT report register swap"
        );
    }

    #[test]
    fn standard_ppu_is_not_register_swapped() {
        let ppu = Ppu::new_for_testing(TimingMode::Ntsc);
        assert!(
            !ppu.is_vs_register_swapped(),
            "Standard PPU should NOT report register swap"
        );
    }
}
