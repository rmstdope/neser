use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use super::registers::Registers;
use super::screen_buffer::ScreenBuffer;
use super::timing::{PpuMode, Timing};
use super::{obj_fifo::ObjFetchModel, pixel_fifo::PixelFifoRenderer, sprites};
use crate::gb::model::CgbModel;
use crate::platform::debugging::ppu_trace_level;
use crate::trace_ppu;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum StopDisplayMode {
    #[default]
    Inactive,
    SolidWhite,
    SolidBlack,
    PreserveCurrent,
}

/// Dots per CPU M-cycle — the granularity at which Mode 3 length is observable.
const DOTS_PER_M_CYCLE: u16 = 4;
const WINDOW_SETUP_DOTS: u16 = 6;

/// DMG/CGB PPU.
///
/// Owns VRAM ($8000–$9FFF) and OAM ($FE00–$FE9F) buffers, all I/O registers,
/// timing state, and the rendered screen buffer.
///
/// In CGB mode (`cgb_mode = true`) the PPU additionally owns VRAM bank 1,
/// CGB color palette RAM, and handles the CGB-specific registers
/// (`$FF4F` VBK, `$FF68/$FF69` BCPS/BCPD, `$FF6A/$FF6B` OCPS/OCPD, `$FF6C` OPRI).
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ppu {
    #[serde_as(as = "[_; 0x2000]")]
    pub vram: [u8; 0x2000],
    #[serde_as(as = "[_; 0xA0]")]
    pub oam: [u8; 0xA0],
    timing: Timing,
    registers: Registers,
    screen_buffer: ScreenBuffer,
    #[serde(default)]
    pixel_fifo: PixelFifoRenderer,
    #[serde(default)]
    stop_display_mode: StopDisplayMode,
    /// Pending interrupt bits for IF register (bit 0 = VBlank, bit 1 = STAT).
    pending_interrupts: u8,
    /// Internal window-line counter (increments each scanline where window is drawn).
    window_line: u8,
    /// Previous STAT IRQ line state for edge detection.
    prev_stat_irq_line: bool,
    /// Frozen LYC=LY coincidence bit (STAT bit 2).
    ///
    /// Real hardware retains this bit when the LCD is turned off.
    /// Changing LYC while the LCD is off has no effect on this bit.
    /// When the LCD is re-enabled, the bit is updated immediately (LY=0).
    lyc_eq_ly_frozen: bool,

    // ── CGB-only fields ───────────────────────────────────────────────────────
    /// `true` when running in CGB mode; enables bank-1 VRAM and color palettes.
    pub cgb_mode: bool,
    /// CGB VRAM bank 1 ($8000–$9FFF when VBK=$01). Holds BG tile attributes and
    /// additional tile data for sprites.
    #[serde_as(as = "[_; 0x2000]")]
    pub vram_bank1: [u8; 0x2000],
    /// `$FF4F` VBK — VRAM bank select (bit 0: 0=bank0, 1=bank1; upper bits read as 1).
    pub vbk: u8,
    /// CGB BG color palette RAM — 8 palettes × 4 colors × 2 bytes (5-5-5 little-endian).
    #[serde_as(as = "[_; 64]")]
    pub bg_palette_ram: [u8; 64],
    /// CGB OBJ color palette RAM — 8 palettes × 4 colors × 2 bytes (5-5-5 little-endian).
    #[serde_as(as = "[_; 64]")]
    pub obj_palette_ram: [u8; 64],
    /// `$FF68` BCPS — BG Color Palette Specification (index + auto-increment flag).
    pub bcps: u8,
    /// `$FF6A` OCPS — OBJ Color Palette Specification (index + auto-increment flag).
    pub ocps: u8,
    /// `$FF6C` OPRI bit 0 — Object priority mode: `false`=OAM order (CGB), `true`=X-coord (DMG).
    pub opri: bool,
    /// Set by `tick_one_dot()` on Mode 3→Mode 0 (HBlank entry) for HDMA synchronization.
    /// Consumed by `take_hblank_entered()`.
    hblank_entered: bool,
    /// DMG compatibility mode: `true` when a DMG-only game runs on CGB hardware.
    ///
    /// In this mode, the CGB renderer must map DMG OAM palette bit (0=OBP0, 1=OBP1)
    /// to CGB OBJ palette 0/1 instead of using CGB OAM attribute bits 0-2.
    #[serde(default)]
    pub dmg_compat: bool,
    /// Selected CGB hardware revision for model-specific DMG-compat rendering quirks.
    ///
    /// Older save-states deserialize this as the default revision; `CgbBus`
    /// re-applies its configured model immediately after restoring the PPU.
    #[serde(default)]
    cgb_model: CgbModel,
    /// Whether SCY is only sampled at the B (GetTile) stage (`true` for CGB-D/E).
    /// Preserved across LCD disable/enable cycles.
    #[serde(default)]
    scy_b_stage_only: bool,
}

impl Ppu {
    const BOOT_REGISTERED_MARK_TILE_ADDR: usize = 0x0190;
    const BOOT_REGISTERED_MARK_TILE: [u8; 16] = [
        0x3C, 0x00, 0x42, 0x00, 0xB9, 0x00, 0xA5, 0x00, 0xB9, 0x00, 0xA5, 0x00, 0x42, 0x00, 0x3C,
        0x00,
    ];
    const BOOT_LOGO_TILE_DATA_ADDR: usize = 0x0010;
    const BOOT_LOGO_TILE_MAP_ADDR: usize = 0x1904;
    const BOOT_LOGO_TILE_MAP: [u8; 44] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x19, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
    ];

    pub fn new() -> Self {
        Self {
            vram: [0u8; 0x2000],
            oam: [0u8; 0xA0],
            timing: Timing::new(),
            registers: Registers::new(),
            screen_buffer: ScreenBuffer::new(),
            pixel_fifo: PixelFifoRenderer::new(),
            stop_display_mode: StopDisplayMode::Inactive,
            pending_interrupts: 0,
            window_line: 0,
            prev_stat_irq_line: false,
            lyc_eq_ly_frozen: false,
            cgb_mode: false,
            vram_bank1: [0u8; 0x2000],
            vbk: 0,
            bg_palette_ram: [0u8; 64],
            obj_palette_ram: [0u8; 64],
            bcps: 0,
            ocps: 0,
            opri: false,
            hblank_entered: false,
            dmg_compat: false,
            cgb_model: CgbModel::default(),
            scy_b_stage_only: false,
        }
    }

    /// Create a new PPU initialised for CGB (Game Boy Color) mode.
    pub fn new_cgb() -> Self {
        let mut ppu = Self::new();
        ppu.cgb_mode = true;
        ppu.timing
            .set_line_153_ly_zero_dot(line_153_ly_zero_dot(true, ppu.cgb_model));
        ppu
    }

    /// Seed the post-boot registered-mark tile at $8190.
    ///
    /// The DMG and CGB boot ROMs leave this tile in VRAM next to the decoded
    /// cartridge logo; monochrome DMG-0 is the documented exception.
    pub(crate) fn seed_boot_registered_mark_tile(&mut self) {
        let start = Self::BOOT_REGISTERED_MARK_TILE_ADDR;
        let end = start + Self::BOOT_REGISTERED_MARK_TILE.len();
        self.vram[start..end].copy_from_slice(&Self::BOOT_REGISTERED_MARK_TILE);
    }

    /// Seed decoded cartridge-logo tile data and tile map left by the boot ROM.
    pub(crate) fn seed_boot_logo_from_header(&mut self, header_logo: &[u8; 48]) {
        let mut dst = Self::BOOT_LOGO_TILE_DATA_ADDR;
        for mut byte in *header_logo {
            for _ in 0..2 {
                let value = Self::decode_boot_logo_nibble(&mut byte);
                self.vram[dst] = value;
                self.vram[dst + 2] = value;
                dst += 4;
            }
        }
        let map_start = Self::BOOT_LOGO_TILE_MAP_ADDR;
        let map_end = map_start + Self::BOOT_LOGO_TILE_MAP.len();
        self.vram[map_start..map_end].copy_from_slice(&Self::BOOT_LOGO_TILE_MAP);
    }

    fn decode_boot_logo_nibble(byte: &mut u8) -> u8 {
        let mut value = 0;
        for _ in 0..4 {
            let carry = (*byte & 0x80) != 0;
            *byte <<= 1;
            value = (value << 1) | u8::from(carry);
            value = (value << 1) | u8::from(carry);
        }
        value
    }

    /// Seed the CGB post-boot faded BG palette state.
    pub(crate) fn seed_cgb_boot_fade_bg_palettes(&mut self) {
        for color in self.bg_palette_ram.chunks_exact_mut(2) {
            color[0] = 0xFF;
            color[1] = 0x7F;
        }
    }

    /// Enable DMG compatibility mode for CGB running a DMG-only game.
    ///
    /// This affects sprite palette selection in the CGB renderer.
    pub fn set_dmg_compat(&mut self, enabled: bool) {
        self.dmg_compat = enabled;
    }

    /// Select the CGB hardware revision used by model-specific PPU behavior.
    pub fn set_cgb_model(&mut self, model: CgbModel) {
        self.cgb_model = model;
        self.scy_b_stage_only = matches!(model, CgbModel::CgbD | CgbModel::CgbE);
        self.pixel_fifo.set_scy_b_stage_only(self.scy_b_stage_only);
        self.timing
            .set_line_153_ly_zero_dot(line_153_ly_zero_dot(self.cgb_mode, self.cgb_model));
    }

    pub(crate) fn fixup_after_state_load(&mut self) {
        self.scy_b_stage_only = matches!(self.cgb_model, CgbModel::CgbD | CgbModel::CgbE);
        self.timing
            .set_line_153_ly_zero_dot(line_153_ly_zero_dot(self.cgb_mode, self.cgb_model));
        self.pixel_fifo.fixup_after_state_load(
            self.cgb_mode,
            self.scy_b_stage_only,
            self.registers.scy,
        );
    }

    pub(crate) fn enter_stop_display_mode(&mut self, mode: StopDisplayMode) {
        self.stop_display_mode = mode;
        match mode {
            StopDisplayMode::Inactive | StopDisplayMode::PreserveCurrent => {}
            StopDisplayMode::SolidWhite => self.screen_buffer.fill_rgb(0xFF, 0xFF, 0xFF),
            StopDisplayMode::SolidBlack => self.screen_buffer.fill_rgb(0x00, 0x00, 0x00),
        }
    }

    pub(crate) fn exit_stop_display_mode(&mut self) {
        self.stop_display_mode = StopDisplayMode::Inactive;
    }

    pub(crate) fn stop_display_mode(&self) -> StopDisplayMode {
        self.stop_display_mode
    }

    // ── Dot-level tick ────────────────────────────────────────────────────────

    /// Advance the PPU by `n` dots (T-cycles).
    ///
    /// Call from `DmgBus::tick(m_cycles)` with `n = m_cycles * 4`.
    /// No-op when the LCD is disabled (LCDC bit 7 = 0).
    ///
    /// Processes dots in groups of 4 (one M-cycle). The STAT mode bits on
    /// normal scans lag the actual mode by 4 T-cycles (one M-cycle); this is
    /// implemented by snapshotting the mode before each group of 4 ticks via
    /// `timing.save_stat_mode()`.
    pub fn tick_dots(&mut self, n: u32) {
        if !self.registers.lcd_enabled() {
            return;
        }
        let groups = n / 4;
        let remainder = n % 4;
        for _ in 0..groups {
            self.timing.save_stat_mode();
            for _ in 0..4 {
                self.tick_one_dot();
            }
        }
        if remainder > 0 {
            self.timing.save_stat_mode();
            for _ in 0..remainder {
                self.tick_one_dot();
            }
        }
    }

    fn tick_one_dot(&mut self) {
        // LCD off (LCDC bit 7 = 0): PPU is completely frozen.
        if self.registers.lcdc & 0x80 == 0 {
            return;
        }

        let lyc = self.registers.lyc;
        let events = self.timing.tick_dot(lyc);

        // Keep lyc_eq_ly_frozen in sync with the live comparison.
        // On scan 1, the LY value changes at dot=0 but the LYC=LY "match fires"
        // (i.e., lyc_eq_ly becomes true) is delayed until dot=4 (OAM Scan start).
        // Clearing lyc_eq_ly when LY diverges from LYC is still immediate at dot=0.
        let new_lyc_eq = self.timing.ly() == lyc;
        // Clearing lyc_eq_ly is always immediate; only the match-fire is delayed until dot=4 on scan 1.
        let lyc_fire_allowed =
            !self.timing.is_second_scanline_after_enable() || self.timing.dot() >= 4 || !new_lyc_eq;
        if lyc_fire_allowed {
            // Level 2: LYC=LY state changes
            if new_lyc_eq && !self.lyc_eq_ly_frozen {
                trace_ppu!(2; "lyc=ly set y={} dot={} lyc={:02X}", self.timing.ly(), self.timing.dot(), lyc);
            } else if !new_lyc_eq && self.lyc_eq_ly_frozen {
                trace_ppu!(2; "lyc=ly clear y={} dot={} lyc={:02X}", self.timing.ly(), self.timing.dot(), lyc);
            }
            self.lyc_eq_ly_frozen = new_lyc_eq;
        }

        // Level 5: Per-dot state dump (after lyc_eq_ly_frozen update for accurate STAT byte)
        trace_ppu!(5;
            "tick y={} dot={} mode={} lcdc={:02X} stat={:02X} ly={} lyc={:02X} scx={:02X} scy={:02X}",
            self.timing.ly(),
            self.timing.dot(),
            self.timing.mode() as u8,
            self.registers.lcdc,
            self.compose_stat_byte(),
            self.timing.ly(),
            self.registers.lyc,
            self.registers.scx,
            self.registers.scy,
        );

        // At Mode 2→Mode 3 transition, extend Mode 3 with OBJ penalty.
        if events.mode_changed && self.timing.mode() == PpuMode::PixelTransfer {
            self.apply_obj_penalty();
            self.pixel_fifo.begin_scanline(
                self.timing.ly(),
                self.timing.dot(),
                &self.oam,
                &self.registers,
                self.cgb_mode,
                self.dmg_compat,
            );
        }

        if self.timing.mode() == PpuMode::PixelTransfer || self.pixel_fifo.is_active() {
            self.render_pixel_fifo_dot();
        }

        // Signal HBlank entry for HDMA when Mode 3→Mode 0 transition fires.
        if events.render_scanline {
            self.hblank_entered = true;
        }

        // V-Blank interrupt (IF bit 0).
        if events.vblank_start {
            self.pending_interrupts |= 0x01;
        }

        // On DMG, the Mode 2 STAT source fires simultaneously with VBlank at line 144.
        // Fire Mode 2 STAT directly (edge-triggered) without changing mode_for_irq,
        // which would cause spurious fires during VBlank if Mode2IE stays enabled.
        if !self.cgb_mode
            && events.vblank_start
            && (self.registers.stat_irq_enables & 0x20 != 0)
            && !self.prev_stat_irq_line
        {
            self.pending_interrupts |= 0x02;
        }

        // Reset window-line counter at the start of a new frame.
        if events.new_frame {
            trace_ppu!(3; "window_line reset y={} dot={} wl={}", self.timing.ly(), self.timing.dot(), self.window_line);
            self.window_line = 0;
            // Level 1: Frame CRC (conditional on level >= 1)
            if ppu_trace_level() >= 1 {
                trace_ppu!(1; "frame crc={:08X}", self.screen_buffer.crc32());
            }
        }

        // STAT interrupt — edge-triggered on the STAT IRQ source line.
        self.update_stat_irq();
    }

    // ── OBJ penalty ───────────────────────────────────────────────────────────

    /// Apply SCX, window, and OBJ penalties to Mode 3 at the Mode 2→3 transition.
    ///
    /// SCX penalty: raw SCX mod 8 dots (unquantized), applied on all visible scanlines.
    /// Window penalty: fixed 6 dots when the current scanline actually begins drawing window pixels.
    /// OBJ penalty: dot-accurate, then floor-quantised to M-cycle boundaries (÷4×4).
    ///   DMG applies it only when LCDC.1 is set; CGB DMG-compat fetches OBJs
    ///   regardless of LCDC.1, matching the production FIFO's fetch policy.
    /// Combined: `extra_dots = scx_raw + window + floor(obj / 4) * 4`.
    fn apply_obj_penalty(&mut self) {
        let scanline = self.timing.ly();
        let scx_penalty = (self.registers.scx & 0x07) as u16;
        let window_penalty = self.window_penalty(scanline);
        let sprites_fetched = ObjFetchModel::for_dmg_render_path(self.cgb_mode, self.dmg_compat)
            .is_some_and(ObjFetchModel::ignores_lcdc_obj_enable)
            || self.registers.lcdc & 0x02 != 0;
        let obj_penalty = if sprites_fetched {
            let sprite_indices = sprites::scan_oam_line(scanline, &self.oam, self.registers.lcdc);
            trace_ppu!(2; "scanline sprites y={} count={}", scanline, sprite_indices.len());
            sprites::calculate_obj_penalty(&sprite_indices, &self.oam, self.registers.scx)
        } else {
            0
        };
        let extra_dots =
            scx_penalty + window_penalty + (obj_penalty / DOTS_PER_M_CYCLE) * DOTS_PER_M_CYCLE;
        self.timing.set_mode3_extra_dots(extra_dots);
    }

    fn render_pixel_fifo_dot(&mut self) {
        let completed_window_activations = self.pixel_fifo.tick(
            self.timing.dot(),
            &self.vram,
            &self.vram_bank1,
            &self.oam,
            &self.registers,
            &self.bg_palette_ram,
            &self.obj_palette_ram,
            self.window_line,
            self.cgb_mode,
            self.opri,
            self.dmg_compat,
            &mut self.screen_buffer,
            self.stop_display_mode != StopDisplayMode::Inactive,
        );
        if let Some(window_activations) = completed_window_activations {
            self.window_line = self.window_line.wrapping_add(window_activations);
        }
    }

    fn window_penalty(&self, scanline: u8) -> u16 {
        if !self.registers.window_enabled() || scanline < self.registers.wy {
            return 0;
        }

        let window_start = self.registers.wx.saturating_sub(7);
        if window_start >= ScreenBuffer::WIDTH as u8 {
            return 0;
        }

        WINDOW_SETUP_DOTS
    }

    // ── STAT IRQ line ─────────────────────────────────────────────────────────

    fn compose_stat_byte(&self) -> u8 {
        let mode_bits = if self.registers.lcd_enabled() {
            self.timing.mode_for_stat() as u8
        } else {
            // When LCD is off, mode bits report 0 (H-Blank).
            0
        };
        // LYC=LY bit: live when LCD is on, frozen when LCD is off.
        let lyc_bit = if self.lyc_eq_ly_frozen { 0x04 } else { 0x00 };
        (self.registers.stat_irq_enables & 0x78) | lyc_bit | mode_bits
    }

    /// Evaluate the STAT IRQ source line and fire a STAT interrupt on 0→1 edge.
    ///
    /// Delegates to `eval_stat_irq_line` using the current `stat_irq_enables` register.
    /// Mode IRQs are suppressed when `mode_for_irq == -1` (first scanline after LCD enable).
    fn update_stat_irq(&mut self) {
        let irq_line = self.eval_stat_irq_line(self.registers.stat_irq_enables);
        if irq_line && !self.prev_stat_irq_line {
            self.pending_interrupts |= 0x02;
            trace_ppu!(2; "stat irq y={} dot={} stat={:02X}", self.timing.ly(), self.timing.dot(), self.compose_stat_byte());
        }
        self.prev_stat_irq_line = irq_line;
    }

    /// Evaluate the STAT IRQ source line for a given set of enable bits.
    ///
    /// `en` is treated as the STAT register value: bits [6:3] select which sources
    /// can fire. Passing `0xFF` evaluates all sources as enabled (used by the DMG
    /// spurious interrupt quirk). Uses `mode_for_irq` for all mode-based sources:
    /// Mode 2 fires 4 dots early, Mode 0 becomes active according to the IRQ timing
    /// view of HBlank (which can begin before the raw mode bit changes), and
    /// Mode 3 returns -1 (suppressed), matching hardware behaviour.
    fn eval_stat_irq_line(&self, en: u8) -> bool {
        let mode_for_irq = self.timing.mode_for_irq();
        let suppress_mode_irqs = mode_for_irq < 0;
        let mode = self.timing.mode();
        (en & 0x40 != 0 && self.lyc_eq_ly_frozen)
            || (mode_for_irq == 2 && en & 0x20 != 0)
            // CGB fires the LY144 Mode 2 STAT source at scanline 143 dot 452,
            // one M-cycle before VBlank; DMG fires this source with VBlank.
            || (self.cgb_mode
                && self.timing.scanline() == 143
                && self.timing.dot() == 452
                && en & 0x20 != 0)
            || (!suppress_mode_irqs && en & 0x10 != 0 && mode == PpuMode::VBlank)
            || (mode_for_irq == 0 && en & 0x08 != 0)
    }

    /// DMG-only STAT write spurious interrupt quirk (Pan Docs: "Spurious STAT interrupts").
    ///
    /// On monochrome hardware, writing any value to $FF41 briefly acts as if $FF
    /// was written for one M-cycle. This means all four IRQ sources are momentarily
    /// enabled. If any source condition is currently true, the STAT interrupt fires.
    /// Uses edge detection via `prev_stat_irq_line` to avoid double-firing.
    fn handle_stat_write_spurious_irq(&mut self) {
        let irq_line = self.eval_stat_irq_line(0xFF);
        if irq_line && !self.prev_stat_irq_line {
            self.pending_interrupts |= 0x02;
            trace_ppu!(2; "stat spurious irq y={} dot={} stat={:02X}", self.timing.ly(), self.timing.dot(), self.compose_stat_byte());
        }
        self.prev_stat_irq_line = irq_line;
    }

    // ── Memory-mapped I/O ─────────────────────────────────────────────────────

    /// Read from VRAM address $8000–$9FFF.
    ///
    /// Returns 0xFF if the CPU is blocked (Mode 3 — Pixel Transfer while LCD on).
    /// When LCD is disabled VRAM is always accessible.
    /// In CGB mode, routes to bank 0 or bank 1 based on the VBK register.
    pub fn read_vram(&self, addr: u16) -> u8 {
        if self.registers.lcd_enabled() && self.timing.is_vram_blocked() {
            return 0xFF;
        }
        let offset = (addr - 0x8000) as usize;
        if self.cgb_mode && self.vbk & 0x01 != 0 {
            self.vram_bank1[offset]
        } else {
            self.vram[offset]
        }
    }

    /// Write to VRAM address $8000–$9FFF.
    ///
    /// Silently ignored if the CPU is blocked (Mode 3 — Pixel Transfer while LCD on).
    /// When LCD is disabled VRAM is always accessible.
    /// In CGB mode, routes to bank 0 or bank 1 based on the VBK register.
    pub fn write_vram(&mut self, addr: u16, val: u8) {
        if self.registers.lcd_enabled() && self.timing.is_vram_write_blocked() {
            return;
        }
        let offset = (addr - 0x8000) as usize;
        if self.cgb_mode && self.vbk & 0x01 != 0 {
            self.vram_bank1[offset] = val;
        } else {
            self.vram[offset] = val;
        }
    }

    /// Read from the forbidden zone $FEA0–$FEFF (DMG behavior).
    ///
    /// During Mode 2 (OAM Scan) while LCD is on: returns 0xFF and applies
    /// the OAM read-corruption side effect.
    /// During any other blocked period (Mode 3) while LCD is on: returns 0xFF
    /// (no corruption).
    /// When OAM is accessible: returns 0x00.
    pub fn read_forbidden_zone(&mut self) -> u8 {
        if self.registers.lcd_enabled() && self.timing.is_oam_blocked() {
            if self.timing.mode() == PpuMode::OamScan
                && let Some(row) = self.current_oam_row()
            {
                self.apply_oam_read_corruption(row);
            }
            return 0xFF;
        }
        0x00
    }

    /// Read from OAM address $FE00–$FE9F.
    ///
    /// During Mode 2 (OAM Scan) while LCD is on: returns 0xFF and applies
    /// the OAM read-corruption side effect to the PPU's currently scanned row.
    /// During any other blocked period (Mode 3, or scan 1 4T extension) while LCD is on:
    /// returns 0xFF (no corruption).
    /// When LCD is disabled OAM is always accessible without corruption.
    pub fn read_oam(&mut self, addr: u16) -> u8 {
        if self.registers.lcd_enabled() && self.timing.is_oam_blocked() {
            if self.timing.mode() == PpuMode::OamScan {
                let row = self.current_oam_row();
                if let Some(r) = row {
                    self.apply_oam_read_corruption(r);
                }
            }
            return 0xFF;
        }
        self.oam[(addr - 0xFE00) as usize]
    }

    /// Write to OAM address $FE00–$FE9F.
    ///
    /// During Mode 2 (OAM Scan) while LCD is on: applies the OAM write-corruption
    /// formula to the PPU's currently scanned row; the actual written value is discarded.
    /// During any other write-blocked period (Mode 3, or scan 1/2 write-gate closed) while LCD on:
    /// write silently ignored.
    /// When LCD is disabled OAM is always accessible without corruption.
    pub fn write_oam(&mut self, addr: u16, val: u8) {
        if self.registers.lcd_enabled() && self.timing.is_oam_write_blocked() {
            if self.timing.mode() == PpuMode::OamScan {
                let row = self.current_oam_row();
                if let Some(r) = row {
                    self.apply_oam_write_corruption(r);
                }
            }
            return;
        }
        self.oam[(addr - 0xFE00) as usize] = val;
    }

    /// Read a PPU I/O register ($FF40–$FF4B).
    ///
    /// When LCD is disabled, LY always reads as 0 and STAT mode bits are 0.
    pub fn read_register(&self, addr: u16) -> u8 {
        let stat = self.compose_stat_byte();
        let ly = if self.registers.lcd_enabled() {
            self.timing.ly()
        } else {
            0
        };
        self.registers.read(addr, ly, stat).unwrap_or(0xFF)
    }

    /// Write a PPU I/O register ($FF40–$FF4B).
    ///
    /// Detects LCD 0→1 enable transition (LCDC bit 7) and resets PPU timing
    /// to scanline 0 / Mode 2, as the hardware does.
    /// On LCD 1→0, retains `lyc_eq_ly_frozen` so STAT bit 2 reflects the last
    /// live LYC=LY comparison result.
    pub fn write_register(&mut self, addr: u16, val: u8) {
        // DMG-only: writing to STAT ($FF41) can trigger a spurious STAT interrupt.
        // Evaluate before applying the write so we use the current PPU state.
        if addr == 0xFF41 && !self.cgb_mode && self.registers.lcd_enabled() {
            self.handle_stat_write_spurious_irq();
        }
        if addr == 0xFF47 {
            self.pixel_fifo.record_bgp_write(
                self.registers.bgp,
                val,
                &self.registers,
                self.cgb_mode,
                self.dmg_compat,
                self.cgb_model,
            );
        }
        if addr == 0xFF48 {
            self.pixel_fifo.record_obp0_write(
                self.registers.obp0,
                val,
                self.cgb_mode,
                self.dmg_compat,
                self.cgb_model,
            );
        }
        if addr == 0xFF49 {
            self.pixel_fifo.record_obp1_write(
                self.registers.obp1,
                val,
                self.cgb_mode,
                self.dmg_compat,
                self.cgb_model,
            );
        }
        if addr == 0xFF40 {
            self.pixel_fifo.record_lcdc_write_with_window(
                self.registers.lcdc,
                val,
                self.registers.scx,
                self.cgb_mode,
                self.dmg_compat,
                self.registers.wx,
                self.registers.wy,
            );
        }
        if addr == 0xFF4B {
            self.pixel_fifo.record_wx_write(val, self.registers.wy);
        }
        if addr == 0xFF43 {
            self.pixel_fifo
                .record_scx_write(self.registers.scx, self.timing.dot());
        }
        if addr == 0xFF42 {
            self.pixel_fifo.record_scy_write(
                self.registers.scy,
                val,
                self.timing.dot(),
                self.cgb_mode,
            );
        }
        let was_enabled = self.registers.lcd_enabled();
        self.registers.write(addr, val);
        let now_enabled = self.registers.lcd_enabled();
        if !was_enabled && now_enabled {
            // LCD 0→1: reset timing, immediately compute LYC=LY for LY=0.
            trace_ppu!(1; "lcdc enable y={} dot={} lcdc={:02X}", self.timing.ly(), self.timing.dot(), val);
            let pending_frame = self.timing.is_frame_ready();
            self.timing = Timing::new();
            if pending_frame {
                self.timing.set_frame_ready();
            }
            self.window_line = 0;
            self.pixel_fifo = PixelFifoRenderer::new();
            self.pixel_fifo.set_scy_b_stage_only(self.scy_b_stage_only);
            // Initialise prev_stat_irq_line to the LYC source state that was active
            // while the LCD was off (based on the frozen LYC=LY bit).
            // This prevents a spurious STAT interrupt when LCD re-enables while
            // the LYC=LY condition was already true (no rising edge = no interrupt).
            let lyc_irq_was_active =
                self.lyc_eq_ly_frozen && (self.registers.stat_irq_enables & 0x40 != 0);
            self.prev_stat_irq_line = lyc_irq_was_active;
            self.lyc_eq_ly_frozen = self.registers.lyc == 0;
            // Evaluate the STAT IRQ line synchronously at the point of LCD re-enable.
            // On real hardware, the LYC=LY comparison fires immediately when the LCD
            // is turned on. This means
            // IF is updated before the NEXT instruction's service_interrupts() call.
            self.update_stat_irq();
        } else if was_enabled && !now_enabled {
            trace_ppu!(1; "lcdc disable y={} dot={} lcdc={:02X}", self.timing.ly(), self.timing.dot(), val);
            self.pixel_fifo = PixelFifoRenderer::new();
            self.pixel_fifo.set_scy_b_stage_only(self.scy_b_stage_only);
        }
        // LCD 1→0: lyc_eq_ly_frozen is intentionally NOT cleared here —
        // hardware retains the last LYC=LY state when the LCD is powered off.
    }

    // ── CGB color palette & bank registers ───────────────────────────────────

    /// Read a CGB-specific PPU register.
    ///
    /// Handles `$FF4F` (VBK), `$FF68` (BCPS), `$FF69` (BCPD), `$FF6A` (OCPS),
    /// `$FF6B` (OCPD), `$FF6C` (OPRI).  Returns `None` if the address is not
    /// a CGB register.
    pub fn read_cgb_register(&self, addr: u16) -> Option<u8> {
        match addr {
            // VBK: bit 0 = selected bank; upper bits always read as 1.
            0xFF4F => Some(0xFE | (self.vbk & 0x01)),
            // BCPS: bit 6 always reads as 1 (unused bit pulled high).
            0xFF68 => Some(self.bcps | 0x40),
            0xFF69 => {
                if self.dmg_compat {
                    Some(0xFF)
                } else if self.registers.lcd_enabled() && self.timing.is_palette_blocked() {
                    // BCPD: blocked during Mode 3 when LCD is on (returns 0xFF).
                    Some(0xFF)
                } else {
                    Some(self.bg_palette_ram[(self.bcps & 0x3F) as usize])
                }
            }
            // OCPS: bit 6 always reads as 1 (unused bit pulled high).
            0xFF6A => Some(self.ocps | 0x40),
            0xFF6B => {
                if self.dmg_compat {
                    Some(0xFF)
                } else if self.registers.lcd_enabled() && self.timing.is_palette_blocked() {
                    // OCPD: blocked during Mode 3 when LCD is on (returns 0xFF).
                    Some(0xFF)
                } else {
                    Some(self.obj_palette_ram[(self.ocps & 0x3F) as usize])
                }
            }
            0xFF6C => Some(0xFE | self.opri as u8),
            _ => None,
        }
    }

    /// Write a CGB-specific PPU register.
    ///
    /// Returns `true` if the address was handled.
    pub fn write_cgb_register(&mut self, addr: u16, val: u8) -> bool {
        if !self.cgb_mode {
            return false;
        }
        match addr {
            0xFF4F => {
                self.vbk = val & 0x01;
                true
            }
            0xFF68 => {
                let before = self.bcps;
                self.bcps = val;
                trace_ppu!(3; "bcps write before={:02X} after={:02X}", before, val);
                true
            }
            0xFF69 => {
                // BCPD is CGB-mode-only. In DMG compatibility mode after boot, the data port
                // is locked, so writes neither update palette RAM nor auto-increment BCPS.
                let index = (self.bcps & 0x3F) as usize;
                let addr = self.bcps & 0x3F;
                let blocked =
                    self.registers.lcd_enabled() && self.timing.is_palette_write_blocked();
                if !blocked && !self.dmg_compat {
                    self.bg_palette_ram[index] = val;
                    trace_ppu!(3; "bcpd write addr={:02X} data={:02X}", addr, val);
                } else {
                    trace_ppu!(3; "bcpd write blocked addr={:02X} data={:02X}", addr, val);
                }
                // Auto-increment address after write if bit 7 is set (even when Mode 3-blocked).
                if !self.dmg_compat && self.bcps & 0x80 != 0 {
                    self.bcps = 0x80 | ((self.bcps + 1) & 0x3F);
                    trace_ppu!(3; "bcps auto-increment addr={:02X}", self.bcps & 0x3F);
                }
                true
            }
            0xFF6A => {
                let before = self.ocps;
                self.ocps = val;
                trace_ppu!(3; "ocps write before={:02X} after={:02X}", before, val);
                true
            }
            0xFF6B => {
                // OCPD is CGB-mode-only. In DMG compatibility mode after boot, the data port
                // is locked, so writes neither update palette RAM nor auto-increment OCPS.
                let index = (self.ocps & 0x3F) as usize;
                let addr = self.ocps & 0x3F;
                let blocked =
                    self.registers.lcd_enabled() && self.timing.is_palette_write_blocked();
                if !blocked && !self.dmg_compat {
                    self.obj_palette_ram[index] = val;
                    trace_ppu!(3; "ocpd write addr={:02X} data={:02X}", addr, val);
                } else {
                    trace_ppu!(3; "ocpd write blocked addr={:02X} data={:02X}", addr, val);
                }
                // Auto-increment address after write if bit 7 is set (even when Mode 3-blocked).
                if !self.dmg_compat && self.ocps & 0x80 != 0 {
                    self.ocps = 0x80 | ((self.ocps + 1) & 0x3F);
                    trace_ppu!(3; "ocps auto-increment addr={:02X}", self.ocps & 0x3F);
                }
                true
            }
            0xFF6C => {
                if self.dmg_compat {
                    return true;
                }
                self.opri = val & 0x01 != 0;
                true
            }
            _ => false,
        }
    }

    // ── Interrupt interface ───────────────────────────────────────────────────

    /// Take pending interrupt bits and clear them.
    ///
    /// The caller (DmgBus) ORs these into the IF register ($FF0F).
    pub fn take_pending_interrupts(&mut self) -> u8 {
        let flags = self.pending_interrupts;
        self.pending_interrupts = 0;
        flags
    }

    // ── HDMA interface ────────────────────────────────────────────────────────

    /// Returns `true` if HBlank was entered since the last call, then clears the flag.
    ///
    /// Used by `CgbBus` to synchronize HDMA transfers with Mode 3→Mode 0 transitions.
    pub fn take_hblank_entered(&mut self) -> bool {
        let entered = self.hblank_entered;
        self.hblank_entered = false;
        entered
    }

    /// Returns whether the LCD is enabled (LCDC bit 7).
    ///
    /// Used by CGB bus for HDMA behavior:
    /// - When LCD is off (disabled), mode is effectively 0 (HBlank), so HDMA can start
    ///   an immediate block transfer if requested.
    /// - When LCD is on, HDMA transfers one block per HBlank (mode 0) period.
    pub fn is_lcd_enabled(&self) -> bool {
        self.registers.lcd_enabled()
    }

    /// Returns the current PPU mode.
    /// Used by CGB bus to check PPU state for HDMA activation timing.
    pub fn mode(&self) -> crate::gb::ppu::timing::PpuMode {
        self.timing.mode()
    }

    // ── Frame output ──────────────────────────────────────────────────────────

    pub fn screen_buffer(&self) -> &ScreenBuffer {
        &self.screen_buffer
    }

    pub fn is_frame_ready(&self) -> bool {
        self.timing.is_frame_ready()
    }

    pub fn clear_frame_ready(&mut self) {
        self.timing.clear_frame_ready();
    }

    // ── Timing accessors (for CPU tracing) ───────────────────────────────────

    /// Returns the total number of completed frames since emulation started.
    ///
    /// Used by CPU tracing to correlate events with frame numbers.
    pub fn frame_count(&self) -> u64 {
        self.timing.frame_count()
    }

    /// Returns the current LY register value (scanline number 0-153).
    ///
    /// Used by CPU tracing to correlate events with PPU scanline position.
    pub fn ly(&self) -> u8 {
        self.timing.ly()
    }

    /// Returns the current dot position within the scanline (0-455).
    ///
    /// Used by CPU tracing to correlate events with PPU dot timing.
    pub fn dot(&self) -> u16 {
        self.timing.dot()
    }

    // ── OAM corruption bug helpers ────────────────────────────────────────────

    /// Returns the OAM row currently being scanned by the PPU.
    ///
    /// Returns `Some(row)` during Mode 2 (OAM Scan) when LCD is enabled.
    /// Returns `None` outside Mode 2 or when LCD is disabled.
    ///
    /// The PPU walks the 20 OAM rows from dot 0 of the line, one row per
    /// M-cycle, so the row is `dot / 4`: row 0 = dots 0–3, row 1 = dots 4–7, …,
    /// row 19 = dots 76–79.  This holds on every line, including scan 1 (the
    /// line right after LCD enable), whose leading `[0,4)` HBlank is only a STAT
    /// artefact — [`Timing::is_oam_blocked`] documents the physical OAM scan as
    /// starting at dot 0 there too.  On scan 1 that HBlank hides row 0, which
    /// costs nothing: row 0 is immune to all three corruption patterns.
    ///
    /// Mode 2 never extends past dot 79, so the result is always `<= 19` and the
    /// callers that index OAM by row cannot run off the end.
    pub fn current_oam_row(&self) -> Option<usize> {
        if !self.registers.lcd_enabled() || self.timing.mode() != PpuMode::OamScan {
            return None;
        }
        Some(self.timing.dot() as usize / 4)
    }

    /// Apply OAM write corruption to the given row.
    ///
    /// Row 0 is immune. Formula: `row[0] = ((a^c)&(b^c))^c` where
    /// `a`=row[0], `b`=prev_row[0], `c`=prev_row[2]; words 1–3 copied from prev row.
    pub fn apply_oam_write_corruption(&mut self, row: usize) {
        if row == 0 {
            return;
        }
        let prev = row - 1;
        let a = self.oam_word(row, 0);
        let b = self.oam_word(prev, 0);
        let c = self.oam_word(prev, 2);
        let prev_w1 = self.oam_word(prev, 1);
        let prev_w2 = self.oam_word(prev, 2);
        let prev_w3 = self.oam_word(prev, 3);
        let new_word0 = ((a ^ c) & (b ^ c)) ^ c;
        self.set_oam_word(row, 0, new_word0);
        self.set_oam_word(row, 1, prev_w1);
        self.set_oam_word(row, 2, prev_w2);
        self.set_oam_word(row, 3, prev_w3);
    }

    /// Apply OAM read corruption to the given row.
    ///
    /// Row 0 is immune. Formula: `row[0] = b|(a&c)` where
    /// `a`=row[0], `b`=prev_row[0], `c`=prev_row[2]; words 1–3 copied from prev row.
    pub fn apply_oam_read_corruption(&mut self, row: usize) {
        if row == 0 {
            return;
        }
        let prev = row - 1;
        let a = self.oam_word(row, 0);
        let b = self.oam_word(prev, 0);
        let c = self.oam_word(prev, 2);
        let prev_w1 = self.oam_word(prev, 1);
        let prev_w2 = self.oam_word(prev, 2);
        let prev_w3 = self.oam_word(prev, 3);
        let new_word0 = b | (a & c);
        self.set_oam_word(row, 0, new_word0);
        self.set_oam_word(row, 1, prev_w1);
        self.set_oam_word(row, 2, prev_w2);
        self.set_oam_word(row, 3, prev_w3);
    }

    /// Apply the OAM Read-During-IDU corruption pattern to the given row.
    ///
    /// For rows 4–18: applies the 4-operand complex formula to the preceding row,
    /// copies it up and down two rows, then applies read corruption on the current row.
    /// For rows 1–3 and row 19: skips the complex part; applies only read corruption.
    /// Row 0 is immune.
    pub fn apply_oam_read_idu_corruption(&mut self, row: usize) {
        const LAST_ROW: usize = 19;
        if row == 0 {
            return;
        }
        // Complex part applies only when row is 4–18 (not first four, not last).
        if (4..LAST_ROW).contains(&row) {
            let n = row;
            let prev = n - 1;
            let prev2 = n - 2;
            let a = self.oam_word(prev2, 0);
            let b = self.oam_word(prev, 0);
            let c = self.oam_word(n, 0);
            let d = self.oam_word(prev, 2);
            let new_b = (b & (a | c | d)) | (a & c & d);
            // Step 1: update word 0 of preceding row.
            self.set_oam_word(prev, 0, new_b);
            // Step 2: copy the (updated) preceding row to row n and row n-2.
            let w0 = self.oam_word(prev, 0);
            let w1 = self.oam_word(prev, 1);
            let w2 = self.oam_word(prev, 2);
            let w3 = self.oam_word(prev, 3);
            self.set_oam_word(prev2, 0, w0);
            self.set_oam_word(prev2, 1, w1);
            self.set_oam_word(prev2, 2, w2);
            self.set_oam_word(prev2, 3, w3);
            self.set_oam_word(n, 0, w0);
            self.set_oam_word(n, 1, w1);
            self.set_oam_word(n, 2, w2);
            self.set_oam_word(n, 3, w3);
        }
        // Always apply normal read corruption to the current row.
        self.apply_oam_read_corruption(row);
    }

    // ── OAM word accessors (used by corruption helpers) ───────────────────────

    fn oam_word(&self, row: usize, word: usize) -> u16 {
        let base = row * 8 + word * 2;
        u16::from_le_bytes([self.oam[base], self.oam[base + 1]])
    }

    fn set_oam_word(&mut self, row: usize, word: usize, val: u16) {
        let base = row * 8 + word * 2;
        let bytes = val.to_le_bytes();
        self.oam[base] = bytes[0];
        self.oam[base + 1] = bytes[1];
    }

    // ── Debugger Accessors ─────────────────────────────────────────────────

    /// Capture VRAM bank 0 for debugger rendering (no side effects).
    pub fn vram_snapshot_for_debugger(&self) -> [u8; 0x2000] {
        self.vram
    }

    /// Capture VRAM bank 1 for debugger rendering (CGB only).
    pub fn vram_bank1_snapshot_for_debugger(&self) -> [u8; 0x2000] {
        self.vram_bank1
    }

    /// Capture OAM for debugger sprite viewer (no side effects).
    pub fn oam_snapshot_for_debugger(&self) -> [u8; 0xA0] {
        self.oam
    }

    /// Capture CGB BG palette RAM for debugger (no side effects).
    pub fn bg_palette_ram_snapshot_for_debugger(&self) -> [u8; 64] {
        self.bg_palette_ram
    }

    /// Capture CGB OBJ palette RAM for debugger (no side effects).
    pub fn obj_palette_ram_snapshot_for_debugger(&self) -> [u8; 64] {
        self.obj_palette_ram
    }

    /// Apply DMG compatibility palette colors for CGB running a DMG-only game.
    ///
    /// Writes the given RGB555 colors to BG palette 0 and OBJ palettes 0/1.
    /// Each palette is 4 colors × 2 bytes = 8 bytes.
    ///
    /// This should only be called during CGB initialization for DMG-only cartridges.
    pub fn apply_dmg_compat_palettes(&mut self, bg0: &[u16; 4], obj0: &[u16; 4], obj1: &[u16; 4]) {
        // Write BG palette 0 (bytes 0-7)
        for (i, &color) in bg0.iter().enumerate() {
            self.bg_palette_ram[i * 2] = (color & 0xFF) as u8;
            self.bg_palette_ram[i * 2 + 1] = (color >> 8) as u8;
        }
        // Write OBJ palette 0 (bytes 0-7)
        for (i, &color) in obj0.iter().enumerate() {
            self.obj_palette_ram[i * 2] = (color & 0xFF) as u8;
            self.obj_palette_ram[i * 2 + 1] = (color >> 8) as u8;
        }
        // Write OBJ palette 1 (bytes 8-15)
        for (i, &color) in obj1.iter().enumerate() {
            self.obj_palette_ram[8 + i * 2] = (color & 0xFF) as u8;
            self.obj_palette_ram[8 + i * 2 + 1] = (color >> 8) as u8;
        }
    }

    /// Get current register values for debugger.
    ///
    /// Returns: (lcdc, scx, scy, bgp, obp0, obp1)
    pub fn registers_snapshot_for_debugger(&self) -> (u8, u8, u8, u8, u8, u8) {
        (
            self.registers.lcdc,
            self.registers.scx,
            self.registers.scy,
            self.registers.bgp,
            self.registers.obp0,
            self.registers.obp1,
        )
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

fn line_153_ly_zero_dot(cgb_mode: bool, cgb_model: CgbModel) -> u16 {
    // SameBoy models the line-153 LY=0 comparison window later on CGB-D/E than
    // on DMG and older CGB revisions.
    if cgb_mode && matches!(cgb_model, CgbModel::CgbD | CgbModel::CgbE) {
        12
    } else {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_dots(ppu: &mut Ppu, n: u32) {
        ppu.tick_dots(n);
    }

    fn tick_until_fifo_can_emit_sprite_pixel(ppu: &mut Ppu) {
        tick_dots(ppu, 120);
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
    }

    fn tick_until_pixel_transfer_starts(ppu: &mut Ppu) {
        while ppu.timing.mode() != PpuMode::PixelTransfer {
            tick_dots(ppu, 1);
        }
    }

    fn set_cgb_palette_colour(
        palette_ram: &mut [u8; 64],
        palette: usize,
        slot: usize,
        rgb555: u16,
    ) {
        let base = palette * 8 + slot * 2;
        palette_ram[base] = (rgb555 & 0x00FF) as u8;
        palette_ram[base + 1] = (rgb555 >> 8) as u8;
    }

    #[test]
    fn boot_logo_seeding_decodes_cartridge_header_bytes_without_embedded_logo() {
        let mut ppu = Ppu::new();
        let mut header_logo = [0u8; 48];
        header_logo[0] = 0x80;
        header_logo[1] = 0x01;

        ppu.seed_boot_logo_from_header(&header_logo);

        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR], 0xC0);
        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR + 2], 0xC0);
        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR + 8], 0x00);
        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR + 10], 0x00);
        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR + 12], 0x03);
        assert_eq!(ppu.vram[Ppu::BOOT_LOGO_TILE_DATA_ADDR + 14], 0x03);
        assert_eq!(
            &ppu.vram[Ppu::BOOT_LOGO_TILE_MAP_ADDR..Ppu::BOOT_LOGO_TILE_MAP_ADDR + 12],
            &[
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C
            ]
        );
    }

    /// Number of dots in the first scanline after LCD enable (starts at dot 4).
    const FIRST_SCANLINE_DOTS: u32 = 452;

    /// Advance PPU past the first scanline (which has no Mode 2) to the start
    /// of Mode 2 on scanline 1. The first scanline after LCD enable is 452 dots.
    /// Mode 2 on scan 1 starts at dot 4 (after brief Mode 0 at dots 0-3).
    fn advance_to_mode_2(ppu: &mut Ppu) {
        tick_dots(ppu, FIRST_SCANLINE_DOTS + 4); // dot=4 on scan1 = Mode 2 start
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        assert_eq!(ppu.timing.ly(), 1);
    }

    #[test]
    fn test_native_cgb_bg_attrs_render_before_hblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.registers.lcdc = 0x91;
        ppu.vram[0x1800] = 1;
        ppu.vram_bank1[0x1800] = 0x0A; // palette 2, tile data bank 1
        ppu.vram_bank1[0x0010] = 0x80; // tile 1, row 0: leftmost pixel colour 1
        set_cgb_palette_colour(&mut ppu.bg_palette_ram, 2, 1, 0x7FFF);

        tick_until_fifo_can_emit_sprite_pixel(&mut ppu);

        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    #[test]
    fn test_native_cgb_window_overrides_bg_before_hblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.registers.lcdc = 0xF1; // LCD on, window on, window map $9C00
        ppu.registers.wx = 7;
        ppu.registers.wy = 0;
        ppu.vram[0x1C00] = 1;
        ppu.vram[0x0010] = 0x80; // window tile 1, row 0: colour 1
        ppu.vram_bank1[0x1C00] = 0x03; // BG palette 3
        set_cgb_palette_colour(&mut ppu.bg_palette_ram, 3, 1, 0x7FFF);

        tick_until_fifo_can_emit_sprite_pixel(&mut ppu);

        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    #[test]
    fn test_native_cgb_master_priority_off_obj_wins_before_hblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.registers.lcdc = 0x92; // LCD on, OBJ on, tile data $8000, master priority off
        ppu.vram[0x1800] = 0;
        ppu.vram[0x0000] = 0x80; // BG tile 0, row 0: colour 1
        ppu.vram[0x0010] = 0x80; // OBJ tile 1, row 0: colour 1
        ppu.vram_bank1[0x1800] = 0x80; // BG priority would win if master priority were on
        ppu.oam[0] = 16;
        ppu.oam[1] = 8;
        ppu.oam[2] = 1;
        ppu.oam[3] = 0;
        set_cgb_palette_colour(&mut ppu.obj_palette_ram, 0, 1, 0x7FFF);

        tick_until_fifo_can_emit_sprite_pixel(&mut ppu);

        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    #[test]
    fn test_native_cgb_bg_priority_beats_obj_before_hblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.registers.lcdc = 0x93; // LCD on, OBJ on, tile data $8000, master priority on
        ppu.vram[0x1800] = 0;
        ppu.vram[0x0000] = 0x80; // BG tile 0, row 0: colour 1
        ppu.vram[0x0010] = 0x80; // OBJ tile 1, row 0: colour 1
        ppu.vram_bank1[0x1800] = 0x80;
        ppu.oam[0] = 16;
        ppu.oam[1] = 8;
        ppu.oam[2] = 1;
        ppu.oam[3] = 0;
        set_cgb_palette_colour(&mut ppu.bg_palette_ram, 0, 1, 0x7FFF);

        tick_until_fifo_can_emit_sprite_pixel(&mut ppu);

        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    #[test]
    fn test_cgb_dmg_compat_obj_palette_bit4_renders_before_hblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.set_dmg_compat(true);
        ppu.registers.lcdc = 0x92;
        ppu.registers.obp1 = 0xE4;
        ppu.vram[0x0010] = 0x80; // OBJ tile 1, row 0: colour 1
        ppu.oam[0] = 16;
        ppu.oam[1] = 8;
        ppu.oam[2] = 1;
        ppu.oam[3] = 0x10;
        set_cgb_palette_colour(&mut ppu.obj_palette_ram, 1, 1, 0x7FFF);

        tick_until_fifo_can_emit_sprite_pixel(&mut ppu);

        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    #[test]
    fn test_scx_fine_scroll_delays_first_fifo_pixel() {
        // Given: SCX fine scroll discards 7 pixels before the first visible pixel.
        let mut ppu = Ppu::new();
        ppu.registers.scx = 7;
        ppu.registers.bgp = 0x00;
        ppu.vram[0x1800] = 1;
        ppu.vram[0x0010] = 0x01;

        tick_until_pixel_transfer_starts(&mut ppu);

        // When: the fixed fetcher startup has elapsed but SCX fine scroll has not.
        tick_dots(&mut ppu, 16);

        // Then: no visible pixel has been pushed to the LCD yet.
        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (0, 0, 0));

        // When: the SCX fine-scroll discard dots have elapsed too.
        tick_dots(&mut ppu, 7);

        // Then: the first visible pixel is pushed at the delayed dot.
        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (255, 255, 255));
    }

    // ── VRAM bus-conflict blocking ─────────────────────────────────────────────

    #[test]
    fn test_vram_readable_during_hblank() {
        // Given: a Ppu ticked to Mode 0 (H-Blank, dot 256 on scanline 0)
        let mut ppu = Ppu::new();
        ppu.write_vram(0x8010, 0xAB);
        tick_dots(&mut ppu, 252); // 252 ticks from dot=4 → dot=256 = Mode 0 start
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        // When: read VRAM during H-Blank
        // Then: real value returned
        assert_eq!(ppu.read_vram(0x8010), 0xAB);
    }

    #[test]
    fn test_vram_blocked_during_pixel_transfer_returns_0xff() {
        // Given: a Ppu ticked into Mode 3 (dot 84, scanline 0)
        let mut ppu = Ppu::new();
        ppu.vram[0x0010] = 0xAB; // bypass write_vram to seed value
        tick_dots(&mut ppu, 80); // 80 ticks from dot=4 → dot=84 = Mode 3 start
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: CPU attempts to read VRAM
        assert_eq!(ppu.read_vram(0x8010), 0xFF); // blocked
    }

    #[test]
    fn test_vram_write_blocked_during_pixel_transfer() {
        // Given: a Ppu ticked to Mode 3
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80); // dot=84 = Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: CPU writes VRAM
        ppu.write_vram(0x8000, 0x42);
        // Then: write is ignored; VRAM unchanged
        tick_dots(&mut ppu, 172); // exit Mode 3 (dot=256 = Mode 0)
        assert_eq!(ppu.read_vram(0x8000), 0x00);
    }

    // ── OAM bus-conflict blocking ──────────────────────────────────────────────

    #[test]
    fn test_oam_blocked_during_oam_scan_returns_0xff() {
        // Given: a Ppu advanced to Mode 2 (OAM Scan) on a normal scanline
        let mut ppu = Ppu::new();
        ppu.oam[0] = 0x55;
        advance_to_mode_2(&mut ppu);
        // When: CPU reads OAM during Mode 2
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
    }

    #[test]
    fn test_oam_readable_during_hblank() {
        // Given: a Ppu at H-Blank (dot 256)
        let mut ppu = Ppu::new();
        ppu.oam[0] = 0x77;
        tick_dots(&mut ppu, 252); // 252 ticks from dot=4 → dot=256 = Mode 0 start
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        // When: CPU reads OAM
        assert_eq!(ppu.read_oam(0xFE00), 0x77);
    }

    #[test]
    fn test_oam_write_blocked_during_oam_scan() {
        // Given: Ppu in Mode 2 on scan 1 (second_scanline_after_enable)
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        // When: write OAM
        ppu.write_oam(0xFE00, 0xAA);
        // Then: ignored; tick past scan-1's Mode0 start (dot=256 = physical Mode3 end).
        tick_dots(&mut ppu, 252); // advance from dot=4 to dot=256 → OAM unblocked on scan 1
        assert_eq!(ppu.read_oam(0xFE00), 0x00);
    }

    // ── VBlank interrupt ───────────────────────────────────────────────────────

    #[test]
    fn test_vblank_interrupt_fires_at_scanline_144() {
        // Given: a Ppu ticked to the last dot before scanline 144
        // First scanline is 452 dots, remaining 142 are 456 each
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 142 + 455); // dot 455 of scanline 143
        assert_eq!(ppu.take_pending_interrupts() & 0x01, 0);
        // When: tick 1 more dot → scanline 144, Mode 1 entry
        ppu.tick_dots(1);
        // Then: VBlank interrupt pending (bit 0)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(flags & 0x01, 0x01, "expected VBlank interrupt bit set");
    }

    #[test]
    fn test_vblank_interrupt_fires_only_once_per_frame() {
        // Given: a Ppu already past VBlank entry
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 144); // enter VBlank
        let flags = ppu.take_pending_interrupts();
        assert_eq!(flags & 0x01, 0x01);
        // When: tick more dots within VBlank
        tick_dots(&mut ppu, 456);
        // Then: no second VBlank interrupt
        let flags2 = ppu.take_pending_interrupts();
        assert_eq!(flags2 & 0x01, 0x00);
    }

    #[test]
    fn test_cgb_mode2_stat_interrupt_fires_one_m_cycle_before_vblank() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(0xFF41, 0x20);

        while !(ppu.ly() == 143 && ppu.dot() == 448) {
            ppu.tick_dots(4);
            let _ = ppu.take_pending_interrupts();
        }

        ppu.tick_dots(4);
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "CGB Mode 2 STAT source should fire at LY143 dot 452"
        );
        assert_eq!(flags & 0x01, 0x00, "VBlank should not fire yet");

        ppu.tick_dots(4);
        let flags = ppu.take_pending_interrupts();
        assert_eq!(flags & 0x01, 0x01, "VBlank should fire one M-cycle later");
    }

    // ── STAT interrupt ─────────────────────────────────────────────────────────

    #[test]
    fn test_stat_interrupt_fires_on_lyc_ly_match_when_enabled() {
        // Given: LYC = 5, STAT bit 6 (LYC=LY interrupt) enabled
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 5); // LYC = 5
        ppu.write_register(0xFF41, 0x40); // STAT bit 6 = LYC=LY IRQ enable
        // First scanline is 452 dots; scanlines 1-4 are 456 each
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 3 + 451); // tick to dot 451 of scanline 4
        // Drain any earlier flags
        let _ = ppu.take_pending_interrupts();
        // When: advance to dot 452 (early LY fires — LY becomes 5 = LYC on regular scans)
        ppu.tick_dots(1);
        // Then: STAT interrupt pending (bit 1 of IF)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "expected STAT interrupt from LYC=LY match"
        );
    }

    #[test]
    fn test_stat_interrupt_fires_when_line_153_compares_as_ly_zero() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 0);
        ppu.write_register(0xFF41, 0x40);
        let _ = ppu.take_pending_interrupts();

        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 152 * 456 + 7);
        assert_eq!(ppu.ly(), 153);
        assert_eq!(ppu.dot(), 7);
        let _ = ppu.take_pending_interrupts();

        ppu.tick_dots(1);

        let flags = ppu.take_pending_interrupts();
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.dot(), 8);
        assert_eq!(
            flags & 0x02,
            0x02,
            "expected STAT interrupt from LYC=0 during line 153"
        );
    }

    #[test]
    fn test_cgb_e_stat_interrupt_fires_when_line_153_compares_as_ly_zero() {
        let mut ppu = Ppu::new_cgb();
        ppu.set_cgb_model(CgbModel::CgbE);
        ppu.write_register(0xFF45, 0);
        ppu.write_register(0xFF41, 0x40);
        let _ = ppu.take_pending_interrupts();

        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 152 * 456 + 11);
        assert_eq!(ppu.ly(), 153);
        assert_eq!(ppu.dot(), 11);
        let _ = ppu.take_pending_interrupts();

        ppu.tick_dots(1);

        let flags = ppu.take_pending_interrupts();
        assert_eq!(ppu.ly(), 0);
        assert_eq!(ppu.dot(), 12);
        assert_eq!(
            flags & 0x02,
            0x02,
            "expected CGB-E STAT interrupt from LYC=0 during line 153"
        );
    }

    #[test]
    fn test_stat_interrupt_not_fired_when_lyc_irq_disabled() {
        // Given: LYC = 5, but STAT LYC IRQ NOT enabled
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 5);
        // STAT bit 6 = 0 (default)
        tick_dots(&mut ppu, 456 * 4 + 456 - 1);
        let _ = ppu.take_pending_interrupts();
        ppu.tick_dots(1); // advance to scanline 5
        let flags = ppu.take_pending_interrupts();
        assert_eq!(flags & 0x02, 0x00);
    }

    // ── Frame ready ───────────────────────────────────────────────────────────

    #[test]
    fn test_frame_not_ready_before_full_frame() {
        let mut ppu = Ppu::new();
        let total = FIRST_SCANLINE_DOTS + 456 * 153;
        tick_dots(&mut ppu, total - 1);
        assert!(!ppu.is_frame_ready());
    }

    #[test]
    fn test_frame_ready_after_154_scanlines() {
        let mut ppu = Ppu::new();
        let total = FIRST_SCANLINE_DOTS + 456 * 153;
        tick_dots(&mut ppu, total);
        assert!(ppu.is_frame_ready());
    }

    #[test]
    fn test_clear_frame_ready_resets_flag() {
        let mut ppu = Ppu::new();
        let total = FIRST_SCANLINE_DOTS + 456 * 153;
        tick_dots(&mut ppu, total);
        assert!(ppu.is_frame_ready());
        ppu.clear_frame_ready();
        assert!(!ppu.is_frame_ready());
    }

    #[test]
    fn test_stop_solid_white_display_keeps_frame_timing_running() {
        // Given: STOP display override has forced the DMG LCD to the blank white reference.
        let mut ppu = Ppu::new();
        ppu.screen_buffer.set_pixel(0, 0, 1, 2, 3);
        ppu.enter_stop_display_mode(StopDisplayMode::SolidWhite);
        ppu.clear_frame_ready();

        // When: enough PPU dots elapse for one frame while STOP remains active.
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 153);

        // Then: frame cadence continues, and normal rendering does not overwrite the blank output.
        assert!(ppu.is_frame_ready());
        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn test_stop_display_suppresses_pixels_but_keeps_fifo_advancing() {
        // Given: a STOP display override active before pixel transfer begins.
        let mut ppu = Ppu::new();
        ppu.enter_stop_display_mode(StopDisplayMode::SolidWhite);
        tick_until_pixel_transfer_starts(&mut ppu);
        assert!(ppu.pixel_fifo.is_active());

        // When: the scanline advances beyond the visible FIFO output period.
        tick_dots(&mut ppu, 200);

        // Then: the FIFO completed normally, but the STOP blank output was not overwritten.
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        assert!(!ppu.pixel_fifo.is_active());
        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn test_stop_preserve_current_display_keeps_existing_framebuffer() {
        // Given: CGB Mode 3 STOP preserves the visible screen instead of blanking it.
        let mut ppu = Ppu::new_cgb();
        ppu.screen_buffer.set_pixel(0, 0, 1, 2, 3);
        ppu.enter_stop_display_mode(StopDisplayMode::PreserveCurrent);
        ppu.clear_frame_ready();

        // When: the PPU keeps ticking for a frame.
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 153);

        // Then: frame cadence continues, but the framebuffer is not redrawn.
        assert!(ppu.is_frame_ready());
        assert_eq!(ppu.screen_buffer.get_pixel(0, 0), (1, 2, 3));
    }

    // ── LCD disable/re-enable frame_ready ─────────────────────────────────────

    #[test]
    fn test_lcd_disable_reenable_produces_frame_ready_within_two_frames() {
        // Given: PPU mid-frame (at scanline 50), frame_ready cleared
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 49);
        assert_eq!(ppu.timing.ly(), 50);
        assert!(!ppu.is_frame_ready());

        // When: game disables LCD, does some work, then re-enables LCD
        ppu.write_register(0xFF40, 0x11); // LCD off (bit 7 = 0)
        ppu.tick_dots(456 * 10); // simulate some time passing (no-op while LCD off)
        ppu.write_register(0xFF40, 0x91); // LCD on (bit 7 = 1)

        // Then: within two full frames of ticking, frame_ready must become true.
        // (One frame = 70,224 dots is the worst case after LCD re-enable resets timing.)
        let two_frames = (FIRST_SCANLINE_DOTS + 456 * 153) * 2;
        let chunk: u32 = 456; // tick one scanline at a time for efficiency
        let mut became_ready = false;
        for _ in (0..two_frames).step_by(chunk as usize) {
            ppu.tick_dots(chunk);
            if ppu.is_frame_ready() {
                became_ready = true;
                break;
            }
        }
        assert!(
            became_ready,
            "frame_ready must eventually become true after LCD disable/re-enable"
        );
    }

    #[test]
    fn test_lcd_disable_reenable_when_frame_ready_pending() {
        // Given: PPU has completed a full frame, frame_ready is true
        let mut ppu = Ppu::new();
        let total = FIRST_SCANLINE_DOTS + 456 * 153;
        tick_dots(&mut ppu, total);
        assert!(ppu.is_frame_ready());

        // When: game disables and re-enables LCD without clearing frame_ready
        ppu.write_register(0xFF40, 0x11); // LCD off
        ppu.write_register(0xFF40, 0x91); // LCD on

        // Then: frame_ready should still be true (pending frame signal preserved)
        assert!(
            ppu.is_frame_ready(),
            "LCD re-enable must not discard a pending frame_ready signal"
        );
    }

    // ── Register interface ────────────────────────────────────────────────────

    #[test]
    fn test_lcdc_register_round_trip() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x00);
        assert_eq!(ppu.read_register(0xFF40), 0x00);
    }

    #[test]
    fn test_stat_mode_bits_reflect_current_mode() {
        // At startup: first scanline after LCD enable reports Mode 0 (HBlank)
        let ppu = Ppu::new();
        let stat = ppu.read_register(0xFF41);
        assert_eq!(stat & 0x03, PpuMode::HBlank as u8);
    }

    #[test]
    fn test_ly_register_returns_current_scanline() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 10); // advance 10 scanlines
        assert_eq!(ppu.read_register(0xFF44), 10);
    }

    // ── LCD disabled (LCDC bit 7 = 0) ─────────────────────────────────────────
    //
    // Per Pan Docs: when LCDC bit 7 is cleared the LCD stops, LY is fixed at 0,
    // the STAT mode bits report Mode 0, and VRAM/OAM become freely accessible.

    /// Helper: advance PPU into Mode 3 then disable the LCD.
    fn ppu_in_mode3_then_lcd_off() -> Ppu {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80); // 80 ticks from dot=4 → dot=84 = Mode 3 start
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        ppu.write_register(0xFF40, 0x11); // clear bit 7 (LCD off), keep rest
        ppu
    }

    #[test]
    fn test_vram_write_succeeds_when_lcd_disabled() {
        let mut ppu = ppu_in_mode3_then_lcd_off();
        ppu.write_vram(0x8000, 0x42);
        assert_eq!(
            ppu.vram[0], 0x42,
            "VRAM write should succeed when LCD is off"
        );
    }

    #[test]
    fn test_vram_read_returns_actual_value_when_lcd_disabled() {
        let mut ppu = ppu_in_mode3_then_lcd_off();
        ppu.vram[0x10] = 0xAB;
        let val = ppu.read_vram(0x8010);
        assert_eq!(
            val, 0xAB,
            "VRAM read should return actual value when LCD is off"
        );
    }

    #[test]
    fn test_ppu_does_not_advance_when_lcd_disabled() {
        let mut ppu = ppu_in_mode3_then_lcd_off();
        let ly_before = ppu.read_register(0xFF44);
        ppu.tick_dots(456 * 10);
        let ly_after = ppu.read_register(0xFF44);
        assert_eq!(
            ly_before, ly_after,
            "PPU should not advance when LCD is off"
        );
    }

    #[test]
    fn test_ly_reads_zero_when_lcd_disabled() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 50);
        assert_eq!(ppu.timing.ly(), 50);
        ppu.write_register(0xFF40, 0x11); // LCD off
        let ly = ppu.read_register(0xFF44);
        assert_eq!(ly, 0, "LY must read as 0 when LCD is disabled");
    }

    #[test]
    fn test_ppu_timing_resets_when_lcd_enabled_after_off() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 49);
        ppu.write_register(0xFF40, 0x11); // LCD off
        ppu.tick_dots(456 * 20); // no-op
        ppu.write_register(0xFF40, 0x91); // LCD on
        assert_eq!(
            ppu.timing.ly(),
            0,
            "LY should reset to 0 when LCD is re-enabled"
        );
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::HBlank,
            "PPU should start in HBlank mode when re-enabled (first scanline after LCD enable)"
        );
    }

    #[test]
    fn test_stat_mode_bits_report_mode0_when_lcd_disabled() {
        let ppu = ppu_in_mode3_then_lcd_off();
        let stat = ppu.read_register(0xFF41);
        assert_eq!(
            stat & 0x03,
            0,
            "STAT mode bits must be 0 (Mode 0) when LCD is disabled"
        );
    }

    #[test]
    fn test_oam_write_succeeds_when_lcd_disabled() {
        // Given: PPU advanced to Mode 2 on a normal scanline; LCD turned off.
        // Normally Mode 2 blocks OAM writes; LCD-off lifts that restriction.
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        ppu.write_register(0xFF40, 0x11); // LCD off
        // When: write OAM
        ppu.write_oam(0xFE10, 0xBB);
        // Then: write accepted
        assert_eq!(
            ppu.oam[0x10], 0xBB,
            "OAM write should succeed when LCD is off"
        );
    }

    #[test]
    fn test_oam_read_returns_actual_value_when_lcd_disabled() {
        // Given: PPU in Mode 3 (PixelTransfer), LCD turned off.
        let mut ppu = ppu_in_mode3_then_lcd_off();
        ppu.oam[0x05] = 0xCC;
        // When: read OAM
        let val = ppu.read_oam(0xFE05);
        // Then: actual value returned (not 0xFF)
        assert_eq!(
            val, 0xCC,
            "OAM read should return actual value when LCD is off"
        );
    }

    // ── OAM corruption: current_oam_row ───────────────────────────────────────

    // NOTE (#3104): the three tests below — and the write_oam/read_oam
    // corruption tests further down — previously asserted a row numbering
    // shifted down by one on scan 1, encoding the very defect this issue fixed.
    // The PPU walks OAM from dot 0 of every line, one row per M-cycle, so the
    // row is `dot / 4` on every line; scan 1 is not special.  Under the old
    // `(dot - 4) / 4` the last row (19) was unreachable, because scan 1's Mode 2
    // ends at dot 80.
    #[test]
    fn test_current_oam_row_is_1_at_scan_1_mode_2_start() {
        // Given: Ppu advanced to Mode 2 on scan 1, which begins at dot=4.
        // Row 0 occupies dots [0,4), reported as the "fake" HBlank on scan 1, so
        // the first row observable here is 1.  That costs nothing: row 0 is
        // immune to all three corruption patterns.
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        assert_eq!(ppu.timing.dot(), 4);
        assert_eq!(ppu.current_oam_row(), Some(1));
    }

    #[test]
    fn test_current_oam_row_advances_one_row_per_m_cycle() {
        // Given: Ppu in Mode 2 at dot=4, tick 4 more → dot=8 → row 8/4 = 2
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 4);
        assert_eq!(ppu.timing.dot(), 8);
        assert_eq!(ppu.current_oam_row(), Some(2));
    }

    #[test]
    fn test_current_oam_row_reaches_row_19_before_mode_3() {
        // Mode 2 walks all 20 OAM rows (0-19), one per M-cycle.  Row 19 is the
        // last, at dot=76; Mode 3 starts at dot=80 and the row goes away.
        // blargg's oam_bug 7-timing_effect sweeps this window and depends on
        // row 19 being reachable on the line it samples (scan 1).
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 72);
        assert_eq!(ppu.timing.dot(), 76);
        assert_eq!(ppu.current_oam_row(), Some(19));
        // Also verify dot=80 is Mode3 (None)
        tick_dots(&mut ppu, 4);
        assert_eq!(ppu.timing.dot(), 80);
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        assert_eq!(ppu.current_oam_row(), None);
    }

    #[test]
    fn test_current_oam_row_returns_none_during_mode_3() {
        // Given: ticked to dot=80 → Mode 3 (Pixel Transfer)
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80);
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        assert_eq!(ppu.current_oam_row(), None);
    }

    #[test]
    fn test_current_oam_row_returns_none_when_lcd_disabled() {
        // Given: LCD disabled; mode stays OamScan but LCD is off
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        ppu.write_register(0xFF40, 0x00); // clear LCDC bit 7 → LCD off
        assert_eq!(ppu.current_oam_row(), None);
    }

    // ── OAM corruption: helpers ───────────────────────────────────────────────

    /// Write all four 16-bit words of an OAM row (little-endian).
    fn set_row_words(oam: &mut [u8; 0xA0], row: usize, words: [u16; 4]) {
        let base = row * 8;
        for (i, &w) in words.iter().enumerate() {
            oam[base + i * 2] = w as u8;
            oam[base + i * 2 + 1] = (w >> 8) as u8;
        }
    }

    /// Read all four 16-bit words of an OAM row (little-endian).
    fn get_row_words(oam: &[u8; 0xA0], row: usize) -> [u16; 4] {
        let base = row * 8;
        [0, 1, 2, 3].map(|i| u16::from_le_bytes([oam[base + i * 2], oam[base + i * 2 + 1]]))
    }

    // ── apply_oam_write_corruption ────────────────────────────────────────────

    #[test]
    fn test_write_corruption_applies_formula_to_row() {
        // Given: row 1 (preceding), row 2 (current) with known values.
        // row 1: b=word0=0x0002, word1=0x0003, c=word2=0x0004, word3=0x0005
        // row 2: a=word0=0x0001
        // a^c = 0x0001^0x0004 = 0x0005
        // b^c = 0x0002^0x0004 = 0x0006
        // (a^c)&(b^c) = 0x0005 & 0x0006 = 0x0004
        // result = 0x0004 ^ 0x0004 = 0x0000
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        ppu.apply_oam_write_corruption(2);
        let row2 = get_row_words(&ppu.oam, 2);
        // Then: word 0 = formula result; words 1–3 copied from preceding row 1
        assert_eq!(row2[0], 0x0000, "write corruption: word0 formula mismatch");
        assert_eq!(
            row2[1], 0x0003,
            "write corruption: word1 should come from preceding row"
        );
        assert_eq!(
            row2[2], 0x0004,
            "write corruption: word2 should come from preceding row"
        );
        assert_eq!(
            row2[3], 0x0005,
            "write corruption: word3 should come from preceding row"
        );
    }

    #[test]
    fn test_write_corruption_skips_row_0() {
        // Row 0 (first two objects) must never be corrupted.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 0, [0x1111, 0x2222, 0x3333, 0x4444]);
        let snapshot = ppu.oam;
        ppu.apply_oam_write_corruption(0);
        assert_eq!(ppu.oam, snapshot, "row 0 is immune to write corruption");
    }

    #[test]
    fn test_write_corruption_does_not_modify_preceding_row() {
        // The preceding row is used as source but must not be mutated.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x00BB, 0x00CC, 0x00DD, 0x00EE]);
        set_row_words(&mut ppu.oam, 2, [0x0001, 0x0002, 0x0003, 0x0004]);
        ppu.apply_oam_write_corruption(2);
        assert_eq!(
            get_row_words(&ppu.oam, 1),
            [0x00BB, 0x00CC, 0x00DD, 0x00EE],
            "preceding row must not be modified by write corruption"
        );
    }

    // ── apply_oam_read_corruption ─────────────────────────────────────────────

    #[test]
    fn test_read_corruption_applies_formula_to_row() {
        // Given: row 2 (preceding), row 3 (current).
        // row 2: b=word0=0x0010, word1=0x0020, c=word2=0x0030, word3=0x0040
        // row 3: a=word0=0x00F0
        // a & c = 0x00F0 & 0x0030 = 0x0030
        // b | (a & c) = 0x0010 | 0x0030 = 0x0030
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 2, [0x0010, 0x0020, 0x0030, 0x0040]);
        set_row_words(&mut ppu.oam, 3, [0x00F0, 0x00A0, 0x00B0, 0x00C0]);
        ppu.apply_oam_read_corruption(3);
        let row3 = get_row_words(&ppu.oam, 3);
        assert_eq!(row3[0], 0x0030, "read corruption: word0 formula mismatch");
        assert_eq!(
            row3[1], 0x0020,
            "read corruption: word1 should come from preceding row"
        );
        assert_eq!(
            row3[2], 0x0030,
            "read corruption: word2 should come from preceding row"
        );
        assert_eq!(
            row3[3], 0x0040,
            "read corruption: word3 should come from preceding row"
        );
    }

    #[test]
    fn test_read_corruption_skips_row_0() {
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 0, [0x1111, 0x2222, 0x3333, 0x4444]);
        let snapshot = ppu.oam;
        ppu.apply_oam_read_corruption(0);
        assert_eq!(ppu.oam, snapshot, "row 0 is immune to read corruption");
    }

    // ── apply_oam_read_idu_corruption ─────────────────────────────────────────

    #[test]
    fn test_read_idu_corruption_complex_formula_for_rows_4_to_18() {
        // Given: row 3 (n-2), row 4 (n-1), row 5 (n) with known values.
        // a=row3[word0]=0x00A0, b=row4[word0]=0x0055,
        // c=row5[word0]=0x000F, d=row4[word2]=0x00C0
        // a|c|d = 0x00A0|0x000F|0x00C0 = 0x00EF
        // b&0x00EF = 0x0055&0x00EF = 0x0045
        // a&c&d = 0 → new_b = 0x0045
        // After: row[3] = row[4] = row[5] = [0x0045, 0x0011, 0x00C0, 0x0022]
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 3, [0x00A0, 0x0001, 0x0002, 0x0003]);
        set_row_words(&mut ppu.oam, 4, [0x0055, 0x0011, 0x00C0, 0x0022]);
        set_row_words(&mut ppu.oam, 5, [0x000F, 0x0099, 0x0088, 0x0077]);
        ppu.apply_oam_read_idu_corruption(5);
        let expected = [0x0045u16, 0x0011, 0x00C0, 0x0022];
        assert_eq!(
            get_row_words(&ppu.oam, 3),
            expected,
            "row n-2 should equal corrupted row n-1"
        );
        assert_eq!(
            get_row_words(&ppu.oam, 4),
            expected,
            "row n-1 word0 should be updated by complex formula"
        );
        assert_eq!(
            get_row_words(&ppu.oam, 5),
            expected,
            "row n should be copied from n-1 then read-corrupted"
        );
    }

    #[test]
    fn test_read_idu_corruption_skips_complex_part_for_row_2() {
        // Row 2 is in "first four rows" (0–3) → complex part skipped, read corruption only.
        // row 1: b=0x0010, word1=0x0021, c(word2)=0x0030, word3=0x0041
        // row 2: a=0x00F0
        // read formula: b|(a&c) = 0x0010|(0x00F0&0x0030) = 0x0010|0x0030 = 0x0030
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x0010, 0x0021, 0x0030, 0x0041]);
        set_row_words(&mut ppu.oam, 2, [0x00F0, 0x00AA, 0x00BB, 0x00CC]);
        ppu.apply_oam_read_idu_corruption(2);
        // Row 1 must NOT be modified (complex part skipped)
        assert_eq!(
            get_row_words(&ppu.oam, 1),
            [0x0010, 0x0021, 0x0030, 0x0041],
            "row n-1 must not be modified for rows < 4"
        );
        // Row 2 gets read corruption only
        let row2 = get_row_words(&ppu.oam, 2);
        assert_eq!(
            row2[0], 0x0030,
            "row 2: word0 read corruption formula mismatch"
        );
        assert_eq!(row2[1], 0x0021, "row 2: word1 from preceding row");
        assert_eq!(row2[2], 0x0030, "row 2: word2 from preceding row");
        assert_eq!(row2[3], 0x0041, "row 2: word3 from preceding row");
    }

    #[test]
    fn test_read_idu_corruption_skips_row_0() {
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 0, [0x0001, 0x0002, 0x0003, 0x0004]);
        let snapshot = ppu.oam;
        ppu.apply_oam_read_idu_corruption(0);
        assert_eq!(ppu.oam, snapshot, "row 0 is immune to read+IDU corruption");
    }

    #[test]
    fn test_read_idu_corruption_skips_complex_part_for_last_row() {
        // Row 19 is the last row → complex part skipped, read corruption only.
        // row 18: b=0x0008, word1=0x0009, c(word2)=0x000A, word3=0x000B
        // row 19: a=0x0007
        // read formula: b|(a&c) = 0x0008|(0x0007&0x000A) = 0x0008|0x0002 = 0x000A
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 18, [0x0008, 0x0009, 0x000A, 0x000B]);
        set_row_words(&mut ppu.oam, 19, [0x0007, 0x00FF, 0x00FE, 0x00FD]);
        ppu.apply_oam_read_idu_corruption(19);
        // Row 18 must NOT be modified
        assert_eq!(
            get_row_words(&ppu.oam, 18),
            [0x0008, 0x0009, 0x000A, 0x000B],
            "row 18 must not be modified for last row"
        );
        let row19 = get_row_words(&ppu.oam, 19);
        assert_eq!(row19[0], 0x000A, "last row: word0 read corruption mismatch");
        assert_eq!(row19[1], 0x0009, "last row: word1 from preceding row");
        assert_eq!(row19[2], 0x000A, "last row: word2 from preceding row");
        assert_eq!(row19[3], 0x000B, "last row: word3 from preceding row");
    }

    // ── Phase B: write_oam / read_oam apply corruption during Mode 2 ──────────

    #[test]
    fn test_write_oam_applies_write_corruption_during_mode_2() {
        // Given: OAM data set at row 1 (preceding) and row 2 (current scan row).
        // row 1: b=0x0002, w1=0x0003, c=0x0004, w3=0x0005
        // row 2: a=0x0001
        // write formula: ((a^c)&(b^c))^c = ((0x0001^0x0004)&(0x0002^0x0004))^0x0004
        //   = (0x0005 & 0x0006) ^ 0x0004 = 0x0004 ^ 0x0004 = 0x0000
        // Expected row 2 after corruption: [0x0000, 0x0003, 0x0004, 0x0005]
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        // Advance to Mode 2, then tick to row 2 (dot 8, one row per M-cycle).
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 4); // dot 4 + 4 = dot 8 → current_oam_row() = Some(2): 8/4=2
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        assert_eq!(ppu.current_oam_row(), Some(2));
        // When: CPU writes to any OAM address
        ppu.write_oam(0xFE10, 0xFF);
        // Then: write corruption formula applied to row 2 (not the written value)
        let row2 = get_row_words(&ppu.oam, 2);
        assert_eq!(
            row2[0], 0x0000,
            "write_oam in Mode 2 must apply write corruption to word0"
        );
        assert_eq!(
            row2[1], 0x0003,
            "write_oam in Mode 2: word1 should come from preceding row"
        );
        assert_eq!(
            row2[2], 0x0004,
            "write_oam in Mode 2: word2 should come from preceding row"
        );
        assert_eq!(
            row2[3], 0x0005,
            "write_oam in Mode 2: word3 should come from preceding row"
        );
    }

    #[test]
    fn test_write_oam_does_not_write_actual_value_during_mode_2() {
        // The written value should never appear in OAM — only the corruption formula result.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 4); // dot 8 → row 2
        ppu.write_oam(0xFE10, 0xAB); // written value 0xAB must NOT appear in OAM
        // If 0xAB appears anywhere in row 2, that is wrong
        for i in 0..8 {
            assert_ne!(
                ppu.oam[2 * 8 + i],
                0xAB,
                "written value 0xAB should not appear in OAM after write during Mode 2"
            );
        }
    }

    #[test]
    fn test_read_oam_applies_read_corruption_during_mode_2() {
        // Given: same OAM setup as write test.
        // read formula: b|(a&c) = 0x0002|(0x0001&0x0004) = 0x0002|0x0000 = 0x0002
        // Expected row 2 after corruption: [0x0002, 0x0003, 0x0004, 0x0005]
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 1, [0x0002, 0x0003, 0x0004, 0x0005]);
        set_row_words(&mut ppu.oam, 2, [0x0001, 0x00AA, 0x00BB, 0x00CC]);
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 4); // dot 8 → row 2
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        // When: CPU reads from any OAM address during Mode 2
        let result = ppu.read_oam(0xFE10);
        // Then: returns 0xFF (blocked) AND applies read corruption as side effect
        assert_eq!(result, 0xFF, "read_oam in Mode 2 must return 0xFF");
        let row2 = get_row_words(&ppu.oam, 2);
        assert_eq!(
            row2[0], 0x0002,
            "read_oam in Mode 2 must apply read corruption to word0"
        );
        assert_eq!(
            row2[1], 0x0003,
            "read_oam in Mode 2: word1 from preceding row"
        );
        assert_eq!(
            row2[2], 0x0004,
            "read_oam in Mode 2: word2 from preceding row"
        );
        assert_eq!(
            row2[3], 0x0005,
            "read_oam in Mode 2: word3 from preceding row"
        );
    }

    #[test]
    fn test_write_oam_does_not_corrupt_during_mode_3() {
        // Mode 3 (Pixel Transfer) blocks OAM writes but does NOT apply corruption.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 2, [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD]);
        tick_dots(&mut ppu, 80); // 80 ticks from dot=4 → dot=84 = Mode 3 start
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        let snapshot = ppu.oam;
        ppu.write_oam(0xFE10, 0x42);
        assert_eq!(
            ppu.oam, snapshot,
            "write_oam in Mode 3 must not corrupt OAM"
        );
    }

    #[test]
    fn test_read_oam_does_not_corrupt_during_mode_3() {
        // Mode 3 blocks OAM reads and does NOT apply corruption.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 2, [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD]);
        tick_dots(&mut ppu, 80); // 80 ticks from dot=4 → dot=84 = Mode 3 start
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        let snapshot = ppu.oam;
        ppu.read_oam(0xFE10);
        assert_eq!(ppu.oam, snapshot, "read_oam in Mode 3 must not corrupt OAM");
    }

    #[test]
    fn test_write_corruption_all_rows_1_to_19() {
        // Verify write corruption applies to all rows 1-19 (not just a few).
        for row in 1..=19 {
            let mut ppu = Ppu::new();
            // Set up: preceding row with known values
            set_row_words(&mut ppu.oam, row - 1, [0x00AA, 0x00BB, 0x00CC, 0x00DD]);
            // Set current row to be corrupted with values that will definitely corrupt
            set_row_words(&mut ppu.oam, row, [0x0055, 0x00FF, 0x0011, 0x0022]);

            ppu.apply_oam_write_corruption(row);

            let corrupted_row = get_row_words(&ppu.oam, row);
            // Words 1-3 should always come from preceding row
            assert_eq!(
                corrupted_row[1], 0x00BB,
                "row {}: word1 must come from preceding row",
                row
            );
            assert_eq!(
                corrupted_row[2], 0x00CC,
                "row {}: word2 must come from preceding row",
                row
            );
            assert_eq!(
                corrupted_row[3], 0x00DD,
                "row {}: word3 must come from preceding row",
                row
            );
            // Word 0 should be formula result: ((0x0055 ^ 0x00CC) & (0x00AA ^ 0x00CC)) ^ 0x00CC
            let a = 0x0055u16;
            let b = 0x00AAu16;
            let c = 0x00CCu16;
            let expected = ((a ^ c) & (b ^ c)) ^ c;
            assert_eq!(
                corrupted_row[0], expected,
                "row {}: word0 formula mismatch",
                row
            );
        }
    }

    #[test]
    fn test_read_corruption_all_rows_1_to_19() {
        // Verify read corruption applies to all rows 1-19 (not just a few).
        for row in 1..=19 {
            let mut ppu = Ppu::new();
            // Set up: preceding row with known values
            set_row_words(&mut ppu.oam, row - 1, [0x00AA, 0x00BB, 0x00CC, 0x00DD]);
            // Set current row to be corrupted
            set_row_words(&mut ppu.oam, row, [0x0055, 0x00FF, 0x0011, 0x0022]);

            ppu.apply_oam_read_corruption(row);

            let corrupted_row = get_row_words(&ppu.oam, row);
            // Words 1-3 should always come from preceding row
            assert_eq!(
                corrupted_row[1], 0x00BB,
                "row {}: word1 must come from preceding row",
                row
            );
            assert_eq!(
                corrupted_row[2], 0x00CC,
                "row {}: word2 must come from preceding row",
                row
            );
            assert_eq!(
                corrupted_row[3], 0x00DD,
                "row {}: word3 must come from preceding row",
                row
            );
            // Word 0 should be formula result: b | (a & c)
            let a = 0x0055u16;
            let b = 0x00AAu16;
            let c = 0x00CCu16;
            let expected = b | (a & c);
            assert_eq!(
                corrupted_row[0], expected,
                "row {}: word0 formula mismatch",
                row
            );
        }
    }

    #[test]
    fn test_write_corruption_formula_bitwise_patterns() {
        // Test various bitwise patterns to verify formula ((a^c)&(b^c))^c
        let test_cases = vec![
            // (a, b, c, expected_result)
            (0xFFFF, 0x0000, 0x0000, 0x0000), // ((0xFFFF^0)&(0^0))^0 = (0xFFFF&0)^0 = 0
            (0x0000, 0xFFFF, 0xFFFF, 0xFFFF), // ((0^0xFFFF)&(0xFFFF^0xFFFF))^0xFFFF = (0xFFFF&0)^0xFFFF = 0xFFFF
            (0xAAAA, 0x5555, 0xAAAA, 0xAAAA), // ((0xAAAA^0xAAAA)&(0x5555^0xAAAA))^0xAAAA = (0&0xFFFF)^0xAAAA = 0xAAAA
            (0x5555, 0xAAAA, 0x5555, 0x5555),
            (0xFF00, 0x00FF, 0xFF00, 0xFF00),
            (0x00FF, 0xFF00, 0x00FF, 0x00FF),
        ];

        for (a, b, c, expected) in test_cases {
            let mut ppu = Ppu::new();
            set_row_words(&mut ppu.oam, 1, [b, 0x1111, c, 0x2222]);
            set_row_words(&mut ppu.oam, 2, [a, 0x3333, 0x4444, 0x5555]);

            ppu.apply_oam_write_corruption(2);

            let result = get_row_words(&ppu.oam, 2)[0];
            assert_eq!(
                result, expected,
                "write corruption formula: a={:04X}, b={:04X}, c={:04X}, got {:04X}, expected {:04X}",
                a, b, c, result, expected
            );
        }
    }

    #[test]
    fn test_read_corruption_formula_bitwise_patterns() {
        // Test various bitwise patterns to verify formula b|(a&c)
        let test_cases = vec![
            // (a, b, c, expected_result)
            (0xFFFF, 0x0000, 0x0000, 0x0000), // 0|(0xFFFF&0) = 0
            (0x0000, 0xFFFF, 0xFFFF, 0xFFFF), // 0xFFFF|(0&0xFFFF) = 0xFFFF
            (0xAAAA, 0x5555, 0xAAAA, 0xFFFF), // 0x5555|(0xAAAA&0xAAAA) = 0x5555|0xAAAA = 0xFFFF
            (0x5555, 0xAAAA, 0x5555, 0xFFFF), // 0xAAAA|(0x5555&0x5555) = 0xAAAA|0x5555 = 0xFFFF
            (0xFF00, 0x00FF, 0xF0F0, 0xF0FF), // 0x00FF|(0xFF00&0xF0F0) = 0x00FF|0xF000 = 0xF0FF
            (0x00FF, 0xFF00, 0x0F0F, 0xFF0F), // 0xFF00|(0x00FF&0x0F0F) = 0xFF00|0x000F = 0xFF0F
        ];

        for (a, b, c, expected) in test_cases {
            let mut ppu = Ppu::new();
            set_row_words(&mut ppu.oam, 1, [b, 0x1111, c, 0x2222]);
            set_row_words(&mut ppu.oam, 2, [a, 0x3333, 0x4444, 0x5555]);

            ppu.apply_oam_read_corruption(2);

            let result = get_row_words(&ppu.oam, 2)[0];
            assert_eq!(
                result, expected,
                "read corruption formula: a={:04X}, b={:04X}, c={:04X}, got {:04X}, expected {:04X}",
                a, b, c, result, expected
            );
        }
    }

    #[test]
    fn test_read_oam_does_not_corrupt_outside_mode_2() {
        // Verify that read_oam outside Mode 2 doesn't apply corruption.
        // Mode 1 (VBlank) - should not corrupt
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 2, [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD]);

        // Advance to VBlank (Mode 1)
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143 + 20); // deep into VBlank
        assert_eq!(ppu.timing.mode(), PpuMode::VBlank);

        let snapshot = ppu.oam;
        let _ = ppu.read_oam(0xFE10);

        assert_eq!(ppu.oam, snapshot, "read_oam in VBlank must not corrupt OAM");
    }

    #[test]
    fn test_write_oam_does_not_corrupt_outside_mode_2() {
        // Verify that write_oam outside Mode 2 doesn't apply corruption.
        // Writes ARE allowed outside Mode 2 on real hardware, just without corruption.
        let mut ppu = Ppu::new();
        set_row_words(&mut ppu.oam, 2, [0xAAAAu16, 0xBBBB, 0xCCCC, 0xDDDD]);

        // Advance to VBlank where OAM is definitely accessible
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143 + 20); // deep into VBlank
        assert_eq!(ppu.timing.mode(), PpuMode::VBlank);

        // Write to OAM in VBlank - should write normally without corruption formula
        ppu.write_oam(0xFE10, 0x42);

        // Verify the write was applied (normal behavior, no corruption formula)
        let val = ppu.read_oam(0xFE10);
        assert_eq!(
            val, 0x42,
            "write_oam in VBlank must apply normal write (accessible outside Mode 2)"
        );
    }

    #[test]
    fn test_read_idu_corruption_complex_formula_rows_4_to_18_verify_all_rows() {
        // Verify complex formula applies to all rows 4-18, not just one case.
        for row in 4..=18 {
            let mut ppu = Ppu::new();
            // Set up three consecutive rows with predictable values
            set_row_words(&mut ppu.oam, row - 2, [0x00A0, 0x0001, 0x0002, 0x0003]);
            set_row_words(&mut ppu.oam, row - 1, [0x0055, 0x0011, 0x00C0, 0x0022]);
            set_row_words(&mut ppu.oam, row, [0x000F, 0x0099, 0x0088, 0x0077]);

            ppu.apply_oam_read_idu_corruption(row);

            // After corruption, all three rows should have the same values (from complex formula)
            let expected = get_row_words(&ppu.oam, row - 1);
            assert_eq!(
                get_row_words(&ppu.oam, row - 2),
                expected,
                "row {}: n-2 should equal corrupted n-1 after read-IDU corruption",
                row
            );
            assert_eq!(
                get_row_words(&ppu.oam, row),
                expected,
                "row {}: n should equal corrupted n-1 after read-IDU corruption",
                row
            );
        }
    }

    // ── LYC=LY bit retention when LCD disabled (stat_lyc_onoff) ──────────────

    #[test]
    fn test_lyc_eq_ly_bit_retained_in_stat_when_lcd_disabled() {
        // Given: PPU running in VBlank with LYC=144 (LY=LYC true)
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 144); // LYC = 144
        // Advance to scanline 144 (VBlank) so LY=LYC=144
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        assert_eq!(ppu.timing.ly(), 144);
        // Verify bit 2 is set before LCD disable
        let stat_before = ppu.read_register(0xFF41);
        assert_eq!(
            stat_before & 0x04,
            0x04,
            "LYC=LY bit must be set before LCD disable"
        );
        // When: turn off LCD (LCDC bit 7 = 0)
        ppu.write_register(0xFF40, 0x11); // LCD off
        // Then: STAT bit 2 (LYC=LY) must be retained (not cleared)
        let stat_after = ppu.read_register(0xFF41);
        assert_eq!(
            stat_after & 0x04,
            0x04,
            "LYC=LY bit must be retained in STAT when LCD is disabled"
        );
    }

    #[test]
    fn test_lyc_eq_ly_bit_not_updated_when_lcd_off_and_lyc_changed() {
        // Given: PPU with LYC=144, in VBlank, LCD turned off (LYC=LY bit retained)
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 144); // LYC = 144
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        ppu.write_register(0xFF40, 0x11); // LCD off with LYC=LY bit set
        let stat_before_lyc_change = ppu.read_register(0xFF41);
        assert_eq!(stat_before_lyc_change & 0x04, 0x04);
        // When: change LYC to a different value while LCD is off
        ppu.write_register(0xFF45, 1); // LYC = 1 (no longer matches LY=144)
        // Then: LYC=LY bit must NOT change (comparison clock is stopped)
        let stat_after = ppu.read_register(0xFF41);
        assert_eq!(
            stat_after & 0x04,
            0x04,
            "LYC=LY bit must not change when LCD is off (comparison clock stopped)"
        );
    }

    #[test]
    fn test_lyc_eq_ly_bit_updated_on_lcd_re_enable() {
        // Given: LCD off with LYC=LY bit set (LYC=144, LY was 144)
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 144);
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        ppu.write_register(0xFF40, 0x11); // LCD off
        assert_eq!(ppu.read_register(0xFF41) & 0x04, 0x04);
        // When: re-enable LCD — LY resets to 0, which ≠ LYC=144
        ppu.write_register(0xFF40, 0x91); // LCD on (LCDC reset value)
        // Then: LYC=LY bit must be 0 (LY=0 ≠ LYC=144)
        let stat = ppu.read_register(0xFF41);
        assert_eq!(
            stat & 0x04,
            0x00,
            "LYC=LY bit must be cleared on LCD re-enable when LY=0 != LYC"
        );
    }

    // ── Mode 2 STAT IRQ fires 4 dots before mode bits (intr_2_mode*) ─────────

    #[test]
    fn test_stat_mode2_irq_fires_at_dot_452_not_dot_0() {
        // The Mode 2 STAT IRQ must fire at dot 452 of a scanline
        // (4 T-cycles before mode bits change to Mode 2 on the next scanline).
        //
        // Strategy:
        // 1. Advance to start of Mode 2 on scanline 1 (dot 4).
        // 2. Enable Mode 2 STAT IRQ; drain pending interrupts.
        // 3. Advance 447 more dots to dot 451 — no STAT IRQ expected yet.
        // 4. Tick 1 more dot to dot 452 — STAT IRQ must fire.
        let mut ppu = Ppu::new();
        // Enable Mode 2 STAT interrupt (bit 5 of STAT)
        ppu.write_register(0xFF41, 0x20);
        // Advance to dot 4, scanline 1 (first normal Mode 2 scanline)
        advance_to_mode_2(&mut ppu);
        // Drain any interrupts that fired on entry to Mode 2
        let _ = ppu.take_pending_interrupts();
        // Advance to dot 451 (HBlank, still on scanline 1)
        tick_dots(&mut ppu, 447);
        assert_eq!(ppu.timing.dot(), 451);
        assert_eq!(
            ppu.take_pending_interrupts() & 0x02,
            0x00,
            "STAT IRQ must not fire before dot 452"
        );
        // Tick one more dot → dot 452
        tick_dots(&mut ppu, 1);
        assert_eq!(ppu.timing.dot(), 452);
        assert_eq!(
            ppu.take_pending_interrupts() & 0x02,
            0x02,
            "STAT Mode 2 IRQ must fire at dot 452 (4 dots before Mode 2 mode bits)"
        );
    }

    #[test]
    fn test_stat_no_spurious_irq_on_lcd_reenable_when_lyc_eq_ly_stays_true() {
        // Round 2 scenario from stat_lyc_onoff: LYC=LY was true when LCD turned off,
        // LYC is changed to 0 while off, LCD is re-enabled with LY=0 = new LYC.
        // The comparison flag stays set (no change), so no interrupt should fire.
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF41, 0x40); // enable LYC=LY STAT IRQ
        ppu.write_register(0xFF45, 144); // LYC = 144
        // Advance to scanline 144 (LY=LYC=144)
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        ppu.write_register(0xFF40, 0x11); // LCD off (lyc_eq_ly_frozen = true)
        let _ = ppu.take_pending_interrupts(); // drain any prior interrupts
        // Change LYC to 0 while LCD is off (frozen bit stays true)
        ppu.write_register(0xFF45, 0);
        // Re-enable LCD: LY=0, LYC=0, LYC=LY still true (flag stays set)
        ppu.write_register(0xFF40, 0x91);
        // Tick a few dots to let update_stat_irq run
        tick_dots(&mut ppu, 8);
        // No STAT interrupt should fire (no rising edge — LYC=LY was already true)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x00,
            "No STAT interrupt when LYC=LY condition remains true across LCD off/on"
        );
    }

    #[test]
    fn test_stat_irq_fires_on_lcd_reenable_when_lyc_eq_ly_becomes_true() {
        // Round 4 scenario from stat_lyc_onoff: LYC=LY was FALSE when LCD turned off,
        // LCD is re-enabled and LY=0 = LYC=0 → interrupt SHOULD fire immediately
        // (synchronously at LCDC write time, before the next instruction).
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF41, 0x40); // enable LYC=LY STAT IRQ
        ppu.write_register(0xFF45, 0); // LYC = 0
        // Advance to VBlank (LY=144 ≠ LYC=0, so lyc_eq_ly_frozen = false)
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        ppu.write_register(0xFF40, 0x11); // LCD off (lyc_eq_ly_frozen = false)
        let _ = ppu.take_pending_interrupts();
        // Re-enable LCD: LY=0, LYC=0 → LYC=LY becomes true (rising edge fires immediately)
        ppu.write_register(0xFF40, 0x91);
        // Interrupt must be pending BEFORE any further ticks (synchronous fire)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "STAT interrupt must fire immediately when LYC=LY becomes true on LCD re-enable"
        );
    }

    // ── LCD timing accessors (for CPU tracing) ──────────────────────────────────

    #[test]
    fn test_frame_count_accessor_returns_timing_frame_count() {
        // Given: a fresh Ppu (frame_count = 0)
        let mut ppu = Ppu::new();
        assert_eq!(ppu.frame_count(), 0);

        // When: advance one full frame
        let frame_dots = FIRST_SCANLINE_DOTS + (153 * 456);
        tick_dots(&mut ppu, frame_dots);

        // Then: frame_count accessor returns 1
        assert_eq!(ppu.frame_count(), 1);
    }

    #[test]
    fn test_ly_accessor_returns_current_scanline() {
        // Given: a fresh Ppu (LY = 0)
        let ppu = Ppu::new();
        assert_eq!(ppu.ly(), 0);

        // When: advance to scanline 5
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 4);
        // Then: ly() returns 5
        assert_eq!(ppu.ly(), 5);
    }

    #[test]
    fn test_dot_accessor_returns_current_dot() {
        // Given: a fresh Ppu (dot = 4)
        let ppu = Ppu::new();
        assert_eq!(ppu.dot(), 4);

        // When: advance 10 dots
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 10);
        // Then: dot() returns 14
        assert_eq!(ppu.dot(), 14);
    }

    // ── HBlank-entered flag (HDMA synchronization) ───────────────────────────

    #[test]
    fn test_hblank_entered_flag_set_on_mode3_to_mode0_transition() {
        // Given: PPU at start (first scanline, dot=4, Mode 0 — but no Mode 3→0 yet)
        let mut ppu = Ppu::new();
        // Clear any initial state
        ppu.take_hblank_entered();

        // First scanline after LCD enable: Mode 0 [4,84), Mode 3 [84,256), Mode 0 [256,456)
        // Mode 3→0 transition fires at dot=256, which is 252 dots from dot=4.
        tick_dots(&mut ppu, 252); // dot=4 + 252 = dot=256, Mode 3→0 transition
        // Then: hblank_entered is set
        assert!(
            ppu.take_hblank_entered(),
            "hblank_entered should be set after Mode 3→Mode 0 transition"
        );
    }

    #[test]
    fn test_take_hblank_entered_clears_flag() {
        // Given: PPU has entered HBlank (flag is set)
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 252); // advance to HBlank on first scanline (dot=256)

        // When: take the flag once
        let first = ppu.take_hblank_entered();
        // Then: first take returns true
        assert!(first);

        // When: take again without further ticks
        let second = ppu.take_hblank_entered();
        // Then: second take returns false (flag was cleared)
        assert!(
            !second,
            "take_hblank_entered should clear the flag after first read"
        );
    }

    #[test]
    fn test_hblank_entered_not_set_before_mode3_ends() {
        // Given: PPU in Mode 3 (Pixel Transfer) on first scanline
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80); // dot=84, Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);

        // Then: hblank_entered should NOT be set
        assert!(
            !ppu.take_hblank_entered(),
            "hblank_entered should not be set during Mode 3"
        );
    }

    #[test]
    fn test_hblank_entered_fires_each_visible_scanline() {
        // Given: PPU starting fresh
        let mut ppu = Ppu::new();
        ppu.take_hblank_entered(); // clear initial state

        // When: advance through first scanline (452 dots)
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS);

        // Then: hblank should have fired on the first scanline
        // (We can't be sure it's still set since the scanline wrap might have cleared it,
        //  but let's check a normal scanline)
        let first = ppu.take_hblank_entered();

        // Advance through scanline 1 (456 dots)
        tick_dots(&mut ppu, 456);
        let second = ppu.take_hblank_entered();

        // At least one of these should have been set (if both fire, even better)
        assert!(
            first || second,
            "hblank_entered should fire at least once during visible scanlines"
        );
    }

    #[test]
    fn test_window_penalty_delays_hblank_start_when_window_visible() {
        // Given: scanline 1 with a visible window starting at the left edge.
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB1); // LCD on, BG on, window on, sprites off.
        ppu.write_register(0xFF4A, 1); // WY = current scanline.
        ppu.write_register(0xFF4B, 7); // WX = 7 => window starts at x = 0.
        advance_to_mode_2(&mut ppu);

        // When: advance through the real Mode 2->3 transition and up to the
        // no-penalty HBlank start boundary.
        tick_dots(&mut ppu, 252); // dot 4 -> dot 256, the no-penalty HBlank start.

        // Then: the 6-dot window setup penalty keeps Mode 3 active past dot 256.
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::PixelTransfer,
            "a visible window must delay HBlank start by 6 dots"
        );

        tick_dots(&mut ppu, 6);
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::HBlank,
            "HBlank should begin after the 6-dot window penalty elapses"
        );
    }

    #[test]
    fn test_window_penalty_stacks_with_scx_penalty() {
        // Given: scanline 1 with both SCX fine scroll and a visible window.
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB1); // LCD on, BG on, window on, sprites off.
        ppu.write_register(0xFF43, 0x03); // SCX fine scroll = 3 dots.
        ppu.write_register(0xFF4A, 1); // WY = current scanline.
        ppu.write_register(0xFF4B, 7); // WX = 7 => window starts at x = 0.
        advance_to_mode_2(&mut ppu);

        // When: advance through the real Mode 2->3 transition and up to the
        // base+SCX-only HBlank boundary.
        tick_dots(&mut ppu, 255); // dot 4 -> dot 259 = base + SCX only.

        // Then: Mode 3 must still be active because window penalty stacks with SCX.
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::PixelTransfer,
            "window penalty must stack on top of SCX penalty instead of being ignored"
        );

        tick_dots(&mut ppu, 6); // dot 259 -> dot 265 = base + SCX + window.
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::HBlank,
            "HBlank should begin once both SCX and window penalties have elapsed"
        );
    }

    #[test]
    fn test_window_penalty_not_applied_when_window_starts_offscreen_right() {
        // Given: scanline 1 with the window enabled but starting beyond the visible screen.
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0xB1); // LCD on, BG on, window on, sprites off.
        ppu.write_register(0xFF4A, 1); // WY = current scanline.
        ppu.write_register(0xFF4B, 167); // WX = 167 => window start x = 160, fully off-screen.
        advance_to_mode_2(&mut ppu);

        // When: advance through the real Mode 2->3 transition and up to the
        // no-penalty HBlank start boundary.
        tick_dots(&mut ppu, 252); // dot 4 -> dot 256, the no-penalty HBlank start.

        // Then: no window penalty is applied because no window pixels are visible.
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::HBlank,
            "an off-screen window must not delay HBlank start"
        );
    }

    #[test]
    fn test_cgb_dmg_compat_obj_penalty_applies_when_lcdc_obj_enable_is_clear() {
        // Given: CGB DMG-compat starts Mode 3 with LCDC.1 clear, but has an
        // object selected by OAM scan. The production FIFO still performs the
        // object fetch, so Mode 3 timing must include the same OBJ penalty.
        let mut ppu = Ppu::new_cgb();
        ppu.set_dmg_compat(true);
        ppu.write_register(0xFF40, 0x91); // LCD on, BG on, sprites disabled.
        ppu.oam[0] = 17; // visible on scanline 1
        ppu.oam[1] = 8; // screen x = 0
        ppu.oam[2] = 1;
        ppu.oam[3] = 0;
        advance_to_mode_2(&mut ppu);

        // When: advance to the no-penalty HBlank boundary.
        tick_dots(&mut ppu, 252);

        // Then: the quantized OBJ fetch penalty keeps Mode 3 active.
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::PixelTransfer,
            "CGB DMG-compat OBJ fetch timing must delay HBlank even when LCDC.1 is clear"
        );

        tick_dots(&mut ppu, 8);
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::HBlank,
            "HBlank should begin once the quantized OBJ penalty elapses"
        );
    }

    // ── DMG STAT spurious interrupt quirk (issue #2198) ───────────────────────
    //
    // Pan Docs: "A hardware quirk in the monochrome Game Boy makes the LCD
    // interrupt sometimes trigger when writing to STAT (including writing $00)
    // during OAM scan, HBlank, VBlank, or LY=LYC. It behaves as if $FF were
    // written for one M-cycle, and then the written value were written the
    // next M-cycle."
    //
    // Trigger conditions: write to $FF41 on DMG while in mode 0 (HBlank),
    // mode 1 (VBlank), mode 2 (OAM scan), or while LY=LYC is active.
    // CGB (even in DMG compatibility mode) does NOT have this quirk.

    /// Helper: advance a fresh DMG PPU to HBlank (Mode 0) on scanline 1.
    /// Drains any interrupts accumulated during setup.
    fn ppu_at_hblank() -> Ppu {
        let mut ppu = Ppu::new();
        // Scan 1 layout: HBlank [0,4) → OamScan [4,80) → Mode3 [80,256) → HBlank [256,456)
        // Advance past first scanline (452 dots) + 256 more → dot 256 of scan 1 = HBlank start.
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 256);
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank, "must be in HBlank");
        let _ = ppu.take_pending_interrupts(); // drain setup interrupts
        ppu
    }

    #[test]
    fn test_stat_write_spurious_irq_during_hblank() {
        // Given: DMG PPU in Mode 0 (HBlank), no IRQ sources enabled in STAT
        let mut ppu = ppu_at_hblank();
        // When: write $00 to STAT (Pan Docs explicitly mentions writing $00 triggers it)
        ppu.write_register(0xFF41, 0x00);
        // Then: STAT interrupt fires (bit 1 of IF)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "STAT write during HBlank must trigger spurious STAT interrupt on DMG"
        );
    }

    #[test]
    fn test_stat_write_spurious_irq_during_vblank() {
        // Given: DMG PPU in VBlank (scanline 144)
        let mut ppu = Ppu::new();
        // First scanline = 452 dots, then 143 more at 456 each → scanline 144 = VBlank
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143 + 1);
        assert_eq!(ppu.timing.mode(), PpuMode::VBlank, "must be in VBlank");
        let _ = ppu.take_pending_interrupts();
        // When: write $00 to STAT
        ppu.write_register(0xFF41, 0x00);
        // Then: STAT interrupt fires
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "STAT write during VBlank must trigger spurious STAT interrupt on DMG"
        );
    }

    #[test]
    fn test_stat_write_spurious_irq_during_oam_scan() {
        // Given: DMG PPU in Mode 2 (OAM Scan) on scanline 1
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu); // dot=4 on scan 1 = Mode 2 start
        let _ = ppu.take_pending_interrupts();
        // When: write $00 to STAT
        ppu.write_register(0xFF41, 0x00);
        // Then: STAT interrupt fires
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "STAT write during OAM Scan must trigger spurious STAT interrupt on DMG"
        );
    }

    #[test]
    fn test_stat_write_spurious_irq_when_lyc_eq_ly() {
        // Given: LYC=5, PPU advanced to scanline 5 where LY=LYC is active,
        // but in Mode 3 (PixelTransfer) so no mode condition is true.
        // The LY=LYC condition alone should trigger the spurious interrupt.
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 5); // LYC = 5
        // Scan 2+: OamScan [0,80) → Mode3 [80,252+) → HBlank.
        // After FIRST_SCANLINE_DOTS + 456*4 we're at dot 0 of scan 5 (LY=5).
        // +81 → dot 81 = safely inside Mode 3 on scan 5.
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 4 + 81);
        assert_eq!(ppu.timing.ly(), 5, "must be on scanline 5 (LY=LYC)");
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::PixelTransfer,
            "must be in Mode 3"
        );
        assert!(ppu.lyc_eq_ly_frozen, "LYC=LY must be active");
        let _ = ppu.take_pending_interrupts();
        // When: write $00 to STAT (no IRQ enables set)
        ppu.write_register(0xFF41, 0x00);
        // Then: STAT interrupt fires due to LY=LYC condition
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x02,
            "STAT write when LY=LYC must trigger spurious STAT interrupt on DMG"
        );
    }

    #[test]
    fn test_stat_write_no_spurious_irq_during_pixel_transfer_no_lyc() {
        // Given: DMG PPU in Mode 3 (PixelTransfer) on scanline 1, LYC != LY
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF45, 99); // LYC = 99, will never match during test
        // Scan 1: HBlank [0,4) → OamScan [4,80) → Mode3 [80,252) → HBlank [252,456)
        // Advance to dot 81 of scan 1 = safely inside Mode 3.
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 81);
        assert_eq!(
            ppu.timing.mode(),
            PpuMode::PixelTransfer,
            "must be in Mode 3"
        );
        assert!(!ppu.lyc_eq_ly_frozen, "LYC must not equal LY");
        let _ = ppu.take_pending_interrupts();
        // When: write $00 to STAT
        ppu.write_register(0xFF41, 0x00);
        // Then: NO STAT interrupt (Mode 3 is not a trigger condition, and LY != LYC)
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x00,
            "STAT write during Mode 3 (PixelTransfer) with no LY=LYC must NOT trigger spurious IRQ"
        );
    }

    #[test]
    fn test_stat_write_no_spurious_irq_in_cgb_mode() {
        // Given: CGB PPU in HBlank — the quirk only exists on DMG
        let mut ppu = Ppu::new_cgb();
        // Advance to HBlank on scanline 1 (same dot count as ppu_at_hblank)
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 256);
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank, "must be in HBlank");
        let _ = ppu.take_pending_interrupts();
        // When: write $00 to STAT
        ppu.write_register(0xFF41, 0x00);
        // Then: NO spurious STAT interrupt on CGB
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x00,
            "STAT write on CGB must NOT trigger the DMG spurious interrupt quirk"
        );
    }

    #[test]
    fn test_stat_write_spurious_irq_no_double_fire_if_irq_line_already_high() {
        // Given: DMG PPU in HBlank with Mode 0 IRQ source already enabled,
        // so the STAT IRQ line is already high (prev_stat_irq_line = true).
        // The spurious quirk should not generate an additional interrupt since
        // the IRQ line was already high (no new rising edge).
        let mut ppu = Ppu::new();
        // Enable Mode 0 (HBlank) IRQ so the line goes high when we enter HBlank
        ppu.write_register(0xFF41, 0x08); // bit 3 = Mode 0 IRQ enable
        // Advance to HBlank on scanline 1
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 256);
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        // The STAT IRQ line should already be high now (Mode 0 IRQ fired);
        // drain any accumulated interrupt flags.
        let _ = ppu.take_pending_interrupts();
        // When: write to STAT again (spurious check should NOT re-fire since line already high)
        ppu.write_register(0xFF41, 0x08);
        // Then: no additional STAT interrupt
        let flags = ppu.take_pending_interrupts();
        assert_eq!(
            flags & 0x02,
            0x00,
            "STAT write when IRQ line already high must not double-fire the STAT interrupt"
        );
    }

    // ── CGB palette blocking (Mode 3) ─────────────────────────────────────────
    //
    // Per Pan Docs §Palettes:
    // - BCPD/OCPD ($FF69/$FF6B) cannot be read or written during Mode 3
    // - Auto-increment still happens even when write is blocked
    // - BCPS/OCPS ($FF68/$FF6A) can be accessed anytime

    /// Helper: advance CGB PPU to Mode 0 (HBlank) on scanline 0.
    fn cgb_ppu_at_hblank() -> Ppu {
        let mut ppu = Ppu::new_cgb();
        // Mode 0 starts at dot 256 on scanline 0
        tick_dots(&mut ppu, 252); // 252 ticks from dot=4 → dot=256 = Mode 0
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank, "must be in HBlank");
        ppu
    }

    /// Helper: advance CGB PPU to Mode 1 (VBlank).
    fn cgb_ppu_at_vblank() -> Ppu {
        let mut ppu = Ppu::new_cgb();
        // First scanline is 452 dots; scanlines 1-143 are 456 each
        // VBlank starts at scanline 144
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 456 * 143);
        assert_eq!(ppu.timing.mode(), PpuMode::VBlank, "must be in VBlank");
        ppu
    }

    /// Helper: advance CGB PPU to Mode 2 (OAM Scan) on scanline 1.
    fn cgb_ppu_at_oam_scan() -> Ppu {
        let mut ppu = Ppu::new_cgb();
        // First scanline is 452 dots; Mode 2 on scan 1 starts at dot 4
        tick_dots(&mut ppu, FIRST_SCANLINE_DOTS + 4);
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan, "must be in OamScan");
        ppu
    }

    // ── BGPD ($FF69) read blocking ────────────────────────────────────────────

    #[test]
    fn test_cgb_bgpd_read_blocked_during_mode3() {
        // Given: CGB PPU in Mode 3 with palette data written
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF68, 0x00); // BCPS = index 0, no auto-increment
        ppu.write_cgb_register(0xFF69, 0xAB); // write palette data
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: read BGPD during Mode 3
        let val = ppu.read_cgb_register(0xFF69);
        // Then: blocked read returns 0xFF
        assert_eq!(
            val,
            Some(0xFF),
            "BGPD read during Mode 3 should return 0xFF"
        );
    }

    #[test]
    fn test_cgb_bgpd_read_allowed_during_hblank() {
        // Given: CGB PPU in HBlank with palette data
        let mut ppu = cgb_ppu_at_hblank();
        ppu.write_cgb_register(0xFF68, 0x00); // BCPS = index 0
        ppu.write_cgb_register(0xFF69, 0xCD); // write palette data
        ppu.write_cgb_register(0xFF68, 0x00); // reset index for read
        // When: read BGPD during HBlank
        let val = ppu.read_cgb_register(0xFF69);
        // Then: actual value returned
        assert_eq!(val, Some(0xCD), "BGPD read during HBlank should succeed");
    }

    #[test]
    fn test_cgb_bgpd_read_allowed_during_vblank() {
        // Given: CGB PPU in VBlank with palette data
        let mut ppu = cgb_ppu_at_vblank();
        ppu.write_cgb_register(0xFF68, 0x05); // BCPS = index 5
        ppu.write_cgb_register(0xFF69, 0x77);
        ppu.write_cgb_register(0xFF68, 0x05); // reset index
        // When: read BGPD during VBlank
        let val = ppu.read_cgb_register(0xFF69);
        // Then: actual value returned
        assert_eq!(val, Some(0x77), "BGPD read during VBlank should succeed");
    }

    #[test]
    fn test_cgb_bgpd_read_allowed_during_oam_scan() {
        // Given: CGB PPU in OAM Scan with palette data
        let mut ppu = cgb_ppu_at_oam_scan();
        ppu.write_cgb_register(0xFF68, 0x10); // BCPS = index 16
        ppu.write_cgb_register(0xFF69, 0x33);
        ppu.write_cgb_register(0xFF68, 0x10); // reset index
        // When: read BGPD during OAM Scan
        let val = ppu.read_cgb_register(0xFF69);
        // Then: actual value returned
        assert_eq!(val, Some(0x33), "BGPD read during OAM Scan should succeed");
    }

    // ── BGPD ($FF69) write blocking ───────────────────────────────────────────

    #[test]
    fn test_cgb_bgpd_write_blocked_during_mode3() {
        // Given: CGB PPU with initial palette value
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF68, 0x00); // BCPS = index 0, no auto-increment
        ppu.write_cgb_register(0xFF69, 0x11); // initial value
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: attempt write during Mode 3
        ppu.write_cgb_register(0xFF68, 0x00); // reset index
        ppu.write_cgb_register(0xFF69, 0x99); // attempt overwrite
        // Then: palette RAM unchanged (verify after exiting Mode 3)
        tick_dots(&mut ppu, 172); // exit Mode 3, enter HBlank
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        ppu.write_cgb_register(0xFF68, 0x00); // reset index
        let val = ppu.read_cgb_register(0xFF69);
        assert_eq!(
            val,
            Some(0x11),
            "BGPD write during Mode 3 should be blocked"
        );
    }

    #[test]
    fn test_cgb_bgpd_write_allowed_during_hblank() {
        // Given: CGB PPU in HBlank
        let mut ppu = cgb_ppu_at_hblank();
        ppu.write_cgb_register(0xFF68, 0x00);
        // When: write during HBlank
        ppu.write_cgb_register(0xFF69, 0xEE);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF68, 0x00);
        let val = ppu.read_cgb_register(0xFF69);
        assert_eq!(val, Some(0xEE), "BGPD write during HBlank should succeed");
    }

    #[test]
    fn test_cgb_bgpd_write_allowed_during_vblank() {
        // Given: CGB PPU in VBlank
        let mut ppu = cgb_ppu_at_vblank();
        ppu.write_cgb_register(0xFF68, 0x3F); // max index
        // When: write during VBlank
        ppu.write_cgb_register(0xFF69, 0xDD);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF68, 0x3F);
        let val = ppu.read_cgb_register(0xFF69);
        assert_eq!(val, Some(0xDD), "BGPD write during VBlank should succeed");
    }

    #[test]
    fn test_cgb_bgpd_write_allowed_during_oam_scan() {
        // Given: CGB PPU in OAM Scan
        let mut ppu = cgb_ppu_at_oam_scan();
        ppu.write_cgb_register(0xFF68, 0x20);
        // When: write during OAM Scan
        ppu.write_cgb_register(0xFF69, 0xBB);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF68, 0x20);
        let val = ppu.read_cgb_register(0xFF69);
        assert_eq!(val, Some(0xBB), "BGPD write during OAM Scan should succeed");
    }

    #[test]
    fn test_cgb_bgpd_auto_increment_happens_even_when_write_blocked_in_mode3() {
        // Given: CGB PPU with BCPS auto-increment enabled
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF68, 0x84); // BCPS = index 4, auto-increment ON
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: write to BGPD during Mode 3
        ppu.write_cgb_register(0xFF69, 0xAA);
        // Then: BCPS index incremented (bit 7 preserved, index = 5, bit 6 reads as 1)
        let bcps = ppu.read_cgb_register(0xFF68);
        assert_eq!(
            bcps,
            Some(0xC5),
            "BCPS auto-increment must happen even when write is blocked"
        );
    }

    // ── OCPD ($FF6B) read blocking ────────────────────────────────────────────

    #[test]
    fn test_cgb_ocpd_read_blocked_during_mode3() {
        // Given: CGB PPU in Mode 3 with OBJ palette data
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF6A, 0x00); // OCPS = index 0
        ppu.write_cgb_register(0xFF6B, 0x56); // write palette data
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: read OCPD during Mode 3
        let val = ppu.read_cgb_register(0xFF6B);
        // Then: blocked read returns 0xFF
        assert_eq!(
            val,
            Some(0xFF),
            "OCPD read during Mode 3 should return 0xFF"
        );
    }

    #[test]
    fn test_cgb_ocpd_read_allowed_during_hblank() {
        // Given: CGB PPU in HBlank with OBJ palette data
        let mut ppu = cgb_ppu_at_hblank();
        ppu.write_cgb_register(0xFF6A, 0x08);
        ppu.write_cgb_register(0xFF6B, 0x44);
        ppu.write_cgb_register(0xFF6A, 0x08);
        // When: read OCPD during HBlank
        let val = ppu.read_cgb_register(0xFF6B);
        // Then: actual value returned
        assert_eq!(val, Some(0x44), "OCPD read during HBlank should succeed");
    }

    #[test]
    fn test_cgb_ocpd_read_allowed_during_vblank() {
        // Given: CGB PPU in VBlank with OBJ palette data
        let mut ppu = cgb_ppu_at_vblank();
        ppu.write_cgb_register(0xFF6A, 0x20);
        ppu.write_cgb_register(0xFF6B, 0x88);
        ppu.write_cgb_register(0xFF6A, 0x20);
        // When: read OCPD during VBlank
        let val = ppu.read_cgb_register(0xFF6B);
        // Then: actual value returned
        assert_eq!(val, Some(0x88), "OCPD read during VBlank should succeed");
    }

    #[test]
    fn test_cgb_ocpd_read_allowed_during_oam_scan() {
        // Given: CGB PPU in OAM Scan with OBJ palette data
        let mut ppu = cgb_ppu_at_oam_scan();
        ppu.write_cgb_register(0xFF6A, 0x3C);
        ppu.write_cgb_register(0xFF6B, 0x22);
        ppu.write_cgb_register(0xFF6A, 0x3C);
        // When: read OCPD during OAM Scan
        let val = ppu.read_cgb_register(0xFF6B);
        // Then: actual value returned
        assert_eq!(val, Some(0x22), "OCPD read during OAM Scan should succeed");
    }

    // ── OCPD ($FF6B) write blocking ───────────────────────────────────────────

    #[test]
    fn test_cgb_ocpd_write_blocked_during_mode3() {
        // Given: CGB PPU with initial OBJ palette value
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF6A, 0x00);
        ppu.write_cgb_register(0xFF6B, 0x66); // initial value
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: attempt write during Mode 3
        ppu.write_cgb_register(0xFF6A, 0x00);
        ppu.write_cgb_register(0xFF6B, 0xFF); // attempt overwrite
        // Then: palette RAM unchanged
        tick_dots(&mut ppu, 172); // exit Mode 3
        ppu.write_cgb_register(0xFF6A, 0x00);
        let val = ppu.read_cgb_register(0xFF6B);
        assert_eq!(
            val,
            Some(0x66),
            "OCPD write during Mode 3 should be blocked"
        );
    }

    #[test]
    fn test_cgb_ocpd_write_allowed_during_hblank() {
        // Given: CGB PPU in HBlank
        let mut ppu = cgb_ppu_at_hblank();
        ppu.write_cgb_register(0xFF6A, 0x10);
        // When: write during HBlank
        ppu.write_cgb_register(0xFF6B, 0x77);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF6A, 0x10);
        let val = ppu.read_cgb_register(0xFF6B);
        assert_eq!(val, Some(0x77), "OCPD write during HBlank should succeed");
    }

    #[test]
    fn test_cgb_ocpd_write_allowed_during_vblank() {
        // Given: CGB PPU in VBlank
        let mut ppu = cgb_ppu_at_vblank();
        ppu.write_cgb_register(0xFF6A, 0x2A);
        // When: write during VBlank
        ppu.write_cgb_register(0xFF6B, 0x55);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF6A, 0x2A);
        let val = ppu.read_cgb_register(0xFF6B);
        assert_eq!(val, Some(0x55), "OCPD write during VBlank should succeed");
    }

    #[test]
    fn test_cgb_ocpd_write_allowed_during_oam_scan() {
        // Given: CGB PPU in OAM Scan
        let mut ppu = cgb_ppu_at_oam_scan();
        ppu.write_cgb_register(0xFF6A, 0x38);
        // When: write during OAM Scan
        ppu.write_cgb_register(0xFF6B, 0xCC);
        // Then: write succeeds
        ppu.write_cgb_register(0xFF6A, 0x38);
        let val = ppu.read_cgb_register(0xFF6B);
        assert_eq!(val, Some(0xCC), "OCPD write during OAM Scan should succeed");
    }

    #[test]
    fn test_cgb_ocpd_auto_increment_happens_even_when_write_blocked_in_mode3() {
        // Given: CGB PPU with OCPS auto-increment enabled
        let mut ppu = Ppu::new_cgb();
        ppu.write_cgb_register(0xFF6A, 0x84); // OCPS = index 4, auto-increment ON
        tick_dots(&mut ppu, 80); // advance to Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: write to OCPD during Mode 3
        ppu.write_cgb_register(0xFF6B, 0xBB);
        // Then: OCPS index incremented (bit 6 reads as 1)
        let ocps = ppu.read_cgb_register(0xFF6A);
        assert_eq!(
            ocps,
            Some(0xC5),
            "OCPS auto-increment must happen even when write is blocked"
        );
    }

    #[test]
    fn test_cgb_dmg_compat_palette_data_ports_are_locked_after_boot() {
        let mut ppu = Ppu::new_cgb();
        ppu.bg_palette_ram[0] = 0x12;
        ppu.obj_palette_ram[0] = 0x34;
        ppu.set_dmg_compat(true);

        ppu.write_cgb_register(0xFF68, 0x80);
        ppu.write_cgb_register(0xFF69, 0xAB);
        ppu.write_cgb_register(0xFF6A, 0x80);
        ppu.write_cgb_register(0xFF6B, 0xCD);

        assert_eq!(ppu.bg_palette_ram[0], 0x12);
        assert_eq!(ppu.obj_palette_ram[0], 0x34);
        assert_eq!(ppu.read_cgb_register(0xFF68), Some(0xC0));
        assert_eq!(ppu.read_cgb_register(0xFF69), Some(0xFF));
        assert_eq!(ppu.read_cgb_register(0xFF6A), Some(0xC0));
        assert_eq!(ppu.read_cgb_register(0xFF6B), Some(0xFF));
    }

    #[test]
    fn test_apply_dmg_compat_palettes_writes_correct_bytes() {
        // Given: CGB PPU with default (zeroed) palette RAM
        let mut ppu = Ppu::new_cgb();

        // Define test palettes in RGB555 format
        let bg0: [u16; 4] = [0x7FFF, 0x5294, 0x294A, 0x0000]; // white, light, dark, black
        let obj0: [u16; 4] = [0x001F, 0x03E0, 0x7C00, 0x0000]; // red, green, blue, black
        let obj1: [u16; 4] = [0x7FFF, 0x421F, 0x1CF2, 0x0000]; // white, pink, maroon, black

        // When: apply DMG compatibility palettes
        ppu.apply_dmg_compat_palettes(&bg0, &obj0, &obj1);

        // Then: BG palette 0 (bytes 0-7) contains bg0 colors in little-endian
        assert_eq!(ppu.bg_palette_ram[0], 0xFF); // 0x7FFF low byte
        assert_eq!(ppu.bg_palette_ram[1], 0x7F); // 0x7FFF high byte
        assert_eq!(ppu.bg_palette_ram[2], 0x94); // 0x5294 low byte
        assert_eq!(ppu.bg_palette_ram[3], 0x52); // 0x5294 high byte
        assert_eq!(ppu.bg_palette_ram[4], 0x4A); // 0x294A low byte
        assert_eq!(ppu.bg_palette_ram[5], 0x29); // 0x294A high byte
        assert_eq!(ppu.bg_palette_ram[6], 0x00); // 0x0000 low byte
        assert_eq!(ppu.bg_palette_ram[7], 0x00); // 0x0000 high byte

        // Then: OBJ palette 0 (bytes 0-7) contains obj0 colors
        assert_eq!(ppu.obj_palette_ram[0], 0x1F); // 0x001F low byte
        assert_eq!(ppu.obj_palette_ram[1], 0x00); // 0x001F high byte
        assert_eq!(ppu.obj_palette_ram[2], 0xE0); // 0x03E0 low byte
        assert_eq!(ppu.obj_palette_ram[3], 0x03); // 0x03E0 high byte
        assert_eq!(ppu.obj_palette_ram[4], 0x00); // 0x7C00 low byte
        assert_eq!(ppu.obj_palette_ram[5], 0x7C); // 0x7C00 high byte
        assert_eq!(ppu.obj_palette_ram[6], 0x00); // 0x0000 low byte
        assert_eq!(ppu.obj_palette_ram[7], 0x00); // 0x0000 high byte

        // Then: OBJ palette 1 (bytes 8-15) contains obj1 colors
        assert_eq!(ppu.obj_palette_ram[8], 0xFF); // 0x7FFF low byte
        assert_eq!(ppu.obj_palette_ram[9], 0x7F); // 0x7FFF high byte
        assert_eq!(ppu.obj_palette_ram[10], 0x1F); // 0x421F low byte
        assert_eq!(ppu.obj_palette_ram[11], 0x42); // 0x421F high byte
        assert_eq!(ppu.obj_palette_ram[12], 0xF2); // 0x1CF2 low byte
        assert_eq!(ppu.obj_palette_ram[13], 0x1C); // 0x1CF2 high byte
        assert_eq!(ppu.obj_palette_ram[14], 0x00); // 0x0000 low byte
        assert_eq!(ppu.obj_palette_ram[15], 0x00); // 0x0000 high byte
    }
}
