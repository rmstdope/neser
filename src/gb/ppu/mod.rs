pub mod background;
pub mod registers;
pub mod rendering;
pub mod screen_buffer;
pub mod sprites;
pub mod timing;
pub mod window;

use registers::Registers;
use screen_buffer::ScreenBuffer;
use timing::{PpuMode, Timing};

/// Dots per CPU M-cycle — the granularity at which Mode 3 length is observable.
const DOTS_PER_M_CYCLE: u16 = 4;

/// DMG/CGB PPU.
///
/// Owns VRAM ($8000–$9FFF) and OAM ($FE00–$FE9F) buffers, all I/O registers,
/// timing state, and the rendered screen buffer.
///
/// In CGB mode (`cgb_mode = true`) the PPU additionally owns VRAM bank 1,
/// CGB color palette RAM, and handles the CGB-specific registers
/// (`$FF4F` VBK, `$FF68/$FF69` BCPS/BCPD, `$FF6A/$FF6B` OCPS/OCPD, `$FF6C` OPRI).
pub struct Ppu {
    pub vram: [u8; 0x2000],
    pub oam: [u8; 0xA0],
    timing: Timing,
    registers: Registers,
    screen_buffer: ScreenBuffer,
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
    pub vram_bank1: [u8; 0x2000],
    /// `$FF4F` VBK — VRAM bank select (bit 0: 0=bank0, 1=bank1; upper bits read as 1).
    pub vbk: u8,
    /// CGB BG color palette RAM — 8 palettes × 4 colors × 2 bytes (5-5-5 little-endian).
    pub bg_palette_ram: [u8; 64],
    /// CGB OBJ color palette RAM — 8 palettes × 4 colors × 2 bytes (5-5-5 little-endian).
    pub obj_palette_ram: [u8; 64],
    /// `$FF68` BCPS — BG Color Palette Specification (index + auto-increment flag).
    pub bcps: u8,
    /// `$FF6A` OCPS — OBJ Color Palette Specification (index + auto-increment flag).
    pub ocps: u8,
    /// `$FF6C` OPRI bit 0 — Object priority mode: `false`=OAM order (CGB), `true`=X-coord (DMG).
    pub opri: bool,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: [0u8; 0x2000],
            oam: [0u8; 0xA0],
            timing: Timing::new(),
            registers: Registers::new(),
            screen_buffer: ScreenBuffer::new(),
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
        }
    }

    /// Create a new PPU initialised for CGB (Game Boy Color) mode.
    pub fn new_cgb() -> Self {
        Self {
            cgb_mode: true,
            ..Self::new()
        }
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
            self.lyc_eq_ly_frozen = new_lyc_eq;
        }

        // At Mode 2→Mode 3 transition, extend Mode 3 with OBJ penalty.
        if events.mode_changed && self.timing.mode() == PpuMode::PixelTransfer {
            self.apply_obj_penalty();
        }

        // Render the current visible scanline when Mode 3→Mode 0 transition fires.
        if events.render_scanline {
            let scanline = self.timing.ly();
            if self.cgb_mode {
                rendering::render_scanline_cgb(
                    scanline,
                    &self.vram,
                    &self.vram_bank1,
                    &self.oam,
                    &self.registers,
                    &self.bg_palette_ram,
                    &self.obj_palette_ram,
                    &mut self.window_line,
                    self.opri,
                    &mut self.screen_buffer,
                );
            } else {
                rendering::render_scanline(
                    scanline,
                    &self.vram,
                    &self.oam,
                    &self.registers,
                    &mut self.window_line,
                    &mut self.screen_buffer,
                );
            }
        }

        // V-Blank interrupt (IF bit 0).
        if events.vblank_start {
            self.pending_interrupts |= 0x01;
        }

        // On DMG, the Mode 2 STAT source fires simultaneously with VBlank at line 144.
        // Fire Mode 2 STAT directly (edge-triggered) without changing mode_for_irq,
        // which would cause spurious fires during VBlank if Mode2IE stays enabled.
        if events.vblank_start
            && (self.registers.stat_irq_enables & 0x20 != 0)
            && !self.prev_stat_irq_line
        {
            self.pending_interrupts |= 0x02;
        }

        // Reset window-line counter at the start of a new frame.
        if events.new_frame {
            self.window_line = 0;
        }

        // STAT interrupt — edge-triggered on the STAT IRQ source line.
        self.update_stat_irq();
    }

    // ── OBJ penalty ───────────────────────────────────────────────────────────

    /// Apply OBJ and SCX fine-scroll penalties to Mode 3 at the Mode 2→3 transition.
    ///
    /// SCX penalty: raw SCX mod 8 dots (unquantized), applied on all visible scanlines.
    /// OBJ penalty: dot-accurate, then floor-quantised to M-cycle boundaries (÷4×4).
    ///   DMG only applies the OBJ penalty when sprites are enabled (LCDC bit 1).
    /// Combined: `extra_dots = scx_raw + floor(obj / 4) * 4`.
    fn apply_obj_penalty(&mut self) {
        let scx_penalty = (self.registers.scx & 0x07) as u16;
        let sprites_enabled = self.registers.lcdc & 0x02 != 0;
        let obj_penalty = if sprites_enabled {
            let scanline = self.timing.ly();
            let sprite_indices = sprites::scan_oam_line(scanline, &self.oam, self.registers.lcdc);
            sprites::calculate_obj_penalty(&sprite_indices, &self.oam, self.registers.scx)
        } else {
            0
        };
        let extra_dots = scx_penalty + (obj_penalty / DOTS_PER_M_CYCLE) * DOTS_PER_M_CYCLE;
        self.timing.set_mode3_extra_dots(extra_dots);
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
    /// Uses `timing.mode_for_irq()` for Mode 2 only (fires 4 dots early).
    /// Mode 0 and Mode 1 remain level-based (raw mode bits) since they are
    /// active throughout the entire HBlank / VBlank period.
    /// Mode IRQs are suppressed when `mode_for_irq == -1` (first scanline after LCD enable).
    fn update_stat_irq(&mut self) {
        let mode = self.timing.mode();
        let mode_for_irq = self.timing.mode_for_irq();
        let en = self.registers.stat_irq_enables;

        // mode_for_irq < 0 signals that mode-based IRQs are suppressed
        // (first scanline after LCD enable, or during Mode 3).
        let suppress_mode_irqs = mode_for_irq < 0;

        // Use the same delayed lyc_eq_ly_frozen latch that drives STAT bit 2,
        // ensuring the IRQ and the readable bit are always consistent.
        let irq_line = (en & 0x40 != 0 && self.lyc_eq_ly_frozen)
            || (mode_for_irq == 2 && en & 0x20 != 0)
            || (!suppress_mode_irqs && en & 0x10 != 0 && mode == PpuMode::VBlank)
            || (mode_for_irq == 0 && en & 0x08 != 0);

        if irq_line && !self.prev_stat_irq_line {
            self.pending_interrupts |= 0x02;
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
        let was_enabled = self.registers.lcd_enabled();
        self.registers.write(addr, val);
        let now_enabled = self.registers.lcd_enabled();
        if !was_enabled && now_enabled {
            // LCD 0→1: reset timing, immediately compute LYC=LY for LY=0.
            self.timing = Timing::new();
            self.window_line = 0;
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
            // is turned on (SameBoy: GB_STAT_update at lcd-enable time). This means
            // IF is updated before the NEXT instruction's service_interrupts() call.
            self.update_stat_irq();
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
            0xFF68 => Some(self.bcps),
            0xFF69 => Some(self.bg_palette_ram[(self.bcps & 0x3F) as usize]),
            0xFF6A => Some(self.ocps),
            0xFF6B => Some(self.obj_palette_ram[(self.ocps & 0x3F) as usize]),
            0xFF6C => Some(0xFE | self.opri as u8),
            _ => None,
        }
    }

    /// Write a CGB-specific PPU register.
    ///
    /// Returns `true` if the address was handled.
    pub fn write_cgb_register(&mut self, addr: u16, val: u8) -> bool {
        match addr {
            0xFF4F => {
                self.vbk = val & 0x01;
                true
            }
            0xFF68 => {
                self.bcps = val;
                true
            }
            0xFF69 => {
                let index = (self.bcps & 0x3F) as usize;
                self.bg_palette_ram[index] = val;
                // Auto-increment address after write if bit 7 is set.
                if self.bcps & 0x80 != 0 {
                    self.bcps = 0x80 | ((self.bcps + 1) & 0x3F);
                }
                true
            }
            0xFF6A => {
                self.ocps = val;
                true
            }
            0xFF6B => {
                let index = (self.ocps & 0x3F) as usize;
                self.obj_palette_ram[index] = val;
                if self.ocps & 0x80 != 0 {
                    self.ocps = 0x80 | ((self.ocps + 1) & 0x3F);
                }
                true
            }
            0xFF6C => {
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

    // ── OAM corruption bug helpers ────────────────────────────────────────────

    /// Returns the OAM row currently being scanned by the PPU.
    ///
    /// Returns `Some(row)` during Mode 2 (OAM Scan) when LCD is enabled.
    /// Returns `None` outside Mode 2 or when LCD is disabled.
    ///
    /// Scan 1 (second_scanline_after_enable): OamScan starts at dot=4.
    ///   Row 0 = dots 4–7, row 1 = dots 8–11, …, row 19 = dots 80–83.
    /// Scan 2+ (regular): OamScan starts at dot=0.
    ///   Row 0 = dots 0–3, row 1 = dots 4–7, …, row 19 = dots 76–79.
    pub fn current_oam_row(&self) -> Option<usize> {
        if !self.registers.lcd_enabled() || self.timing.mode() != PpuMode::OamScan {
            return None;
        }
        let oam_scan_offset = if self.timing.is_second_scanline_after_enable() {
            4 // scan 1: OamScan starts at dot=4
        } else {
            0 // scan 2+: OamScan starts at dot=0
        };
        Some((self.timing.dot() as usize).saturating_sub(oam_scan_offset) / 4)
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

    // ── Save-state accessors ──────────────────────────────────────────────────

    pub(crate) fn registers_snapshot(&self) -> [u8; 10] {
        [
            self.registers.lcdc,
            self.registers.stat_irq_enables,
            self.registers.scy,
            self.registers.scx,
            self.registers.lyc,
            self.registers.bgp,
            self.registers.obp0,
            self.registers.obp1,
            self.registers.wy,
            self.registers.wx,
        ]
    }

    pub(crate) fn restore_registers(&mut self, regs: &[u8; 10]) {
        self.registers.lcdc = regs[0];
        self.registers.stat_irq_enables = regs[1];
        self.registers.scy = regs[2];
        self.registers.scx = regs[3];
        self.registers.lyc = regs[4];
        self.registers.bgp = regs[5];
        self.registers.obp0 = regs[6];
        self.registers.obp1 = regs[7];
        self.registers.wy = regs[8];
        self.registers.wx = regs[9];
    }

    pub(crate) fn restore_screen_buffer(&mut self, data: &[u8]) -> Result<(), String> {
        self.screen_buffer.restore_from_bytes(data)
    }

    pub(crate) fn timing_dot(&self) -> u16 {
        self.timing.dot()
    }
    pub(crate) fn timing_scanline(&self) -> u8 {
        self.timing.scanline()
    }
    pub(crate) fn timing_mode(&self) -> u8 {
        self.timing.mode() as u8
    }
    pub(crate) fn timing_stat_mode(&self) -> u8 {
        self.timing.stat_mode_raw()
    }
    pub(crate) fn timing_first_scanline(&self) -> bool {
        self.timing.is_first_scanline_after_enable()
    }
    pub(crate) fn timing_second_scanline(&self) -> bool {
        self.timing.is_second_scanline_after_enable()
    }
    pub(crate) fn timing_third_scanline(&self) -> bool {
        self.timing.is_third_scanline_after_enable()
    }
    pub(crate) fn timing_mode_for_irq(&self) -> i8 {
        self.timing.mode_for_irq_raw()
    }
    pub(crate) fn timing_mode3_extra_dots(&self) -> u16 {
        self.timing.mode3_extra_dots_raw()
    }
    pub(crate) fn timing_ly(&self) -> u8 {
        self.timing.ly()
    }
    pub(crate) fn pending_interrupts_raw(&self) -> u8 {
        self.pending_interrupts
    }
    pub(crate) fn set_pending_interrupts(&mut self, val: u8) {
        self.pending_interrupts = val;
    }
    pub(crate) fn window_line_raw(&self) -> u8 {
        self.window_line
    }
    pub(crate) fn set_window_line(&mut self, val: u8) {
        self.window_line = val;
    }
    pub(crate) fn prev_stat_irq_line_raw(&self) -> bool {
        self.prev_stat_irq_line
    }
    pub(crate) fn set_prev_stat_irq_line(&mut self, val: bool) {
        self.prev_stat_irq_line = val;
    }
    pub(crate) fn lyc_eq_ly_frozen_raw(&self) -> bool {
        self.lyc_eq_ly_frozen
    }
    pub(crate) fn set_lyc_eq_ly_frozen(&mut self, val: bool) {
        self.lyc_eq_ly_frozen = val;
    }

    pub(crate) fn restore_timing(
        &mut self,
        snap: &crate::gb::console::save_state::PpuSnapshot,
    ) -> Result<(), String> {
        self.timing
            .restore(&crate::gb::console::save_state::TimingRestoreArgs {
                dot: snap.dot,
                scanline: snap.scanline,
                mode: snap.mode,
                stat_mode: snap.stat_mode,
                frame_ready: snap.frame_ready,
                first_scanline: snap.first_scanline_after_enable,
                second_scanline: snap.second_scanline_after_enable,
                third_scanline: snap.third_scanline_after_enable,
                mode_for_irq: snap.mode_for_irq,
                mode3_extra_dots: snap.mode3_extra_dots,
                ly: snap.ly,
            })
    }
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_dots(ppu: &mut Ppu, n: u32) {
        ppu.tick_dots(n);
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

    #[test]
    fn test_current_oam_row_returns_row_0_at_mode_2_start() {
        // Given: Ppu advanced to Mode 2 on scanline 1, dot=0 → row 0/4 = 0
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        assert_eq!(ppu.current_oam_row(), Some(0));
    }

    #[test]
    fn test_current_oam_row_returns_row_1_after_4_dot_ticks() {
        // Given: Ppu in Mode 2 at dot=4, tick 4 more → dot=8 → row (8-4)/4 = 1
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 4);
        assert_eq!(ppu.timing.dot(), 8);
        assert_eq!(ppu.current_oam_row(), Some(1));
    }

    #[test]
    fn test_current_oam_row_returns_row_18_at_dot_76() {
        // On scan 1, OamScan runs [4,80): 19 rows (0-18). Row 18 is the last.
        // dot=76 → row (76-4)/4 = 18. At dot=80: Mode3 starts, row=None.
        let mut ppu = Ppu::new();
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 72);
        assert_eq!(ppu.timing.dot(), 76);
        assert_eq!(ppu.current_oam_row(), Some(18));
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
        // Advance to Mode 2 on a normal scanline, then tick to row 2
        advance_to_mode_2(&mut ppu);
        tick_dots(&mut ppu, 8); // dot 4 + 8 = dot 12 → current_oam_row() = Some(2): (12-4)/4=2
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
        tick_dots(&mut ppu, 8);
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
        tick_dots(&mut ppu, 8);
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
}
