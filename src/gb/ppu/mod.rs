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

/// DMG PPU.
///
/// Owns VRAM ($8000–$9FFF) and OAM ($FE00–$FE9F) buffers, all I/O registers,
/// timing state, and the rendered screen buffer.
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
        }
    }

    // ── Dot-level tick ────────────────────────────────────────────────────────

    /// Advance the PPU by `n` dots (T-cycles).
    ///
    /// Call from `DmgBus::tick(m_cycles)` with `n = m_cycles * 4`.
    pub fn tick_dots(&mut self, n: u32) {
        for _ in 0..n {
            self.tick_one_dot();
        }
    }

    fn tick_one_dot(&mut self) {
        // LCD off (LCDC bit 7 = 0): PPU is completely frozen.
        if self.registers.lcdc & 0x80 == 0 {
            return;
        }
        let lyc = self.registers.lyc;
        let events = self.timing.tick_dot(lyc);

        // Render the current visible scanline when Mode 3→Mode 0 transition fires.
        if events.render_scanline {
            let scanline = self.timing.ly();
            rendering::render_scanline(
                scanline,
                &self.vram,
                &self.oam,
                &self.registers,
                &mut self.window_line,
                &mut self.screen_buffer,
            );
        }

        // V-Blank interrupt (IF bit 0).
        if events.vblank_start {
            self.pending_interrupts |= 0x01;
        }

        // Reset window-line counter at the start of a new frame.
        if events.new_frame {
            self.window_line = 0;
        }

        // STAT interrupt — edge-triggered on the STAT IRQ source line.
        self.update_stat_irq();
    }

    // ── STAT IRQ line ─────────────────────────────────────────────────────────

    fn compose_stat_byte(&self) -> u8 {
        let mode_bits = self.timing.mode() as u8;
        let lyc_bit = if self.timing.ly() == self.registers.lyc {
            0x04
        } else {
            0x00
        };
        (self.registers.stat_irq_enables & 0x78) | lyc_bit | mode_bits
    }

    /// Evaluate the STAT IRQ source line and fire a STAT interrupt on 0→1 edge.
    fn update_stat_irq(&mut self) {
        let mode = self.timing.mode();
        let ly = self.timing.ly();
        let lyc = self.registers.lyc;
        let en = self.registers.stat_irq_enables;

        let irq_line = (en & 0x40 != 0 && ly == lyc)
            || (en & 0x20 != 0 && mode == PpuMode::OamScan)
            || (en & 0x10 != 0 && mode == PpuMode::VBlank)
            || (en & 0x08 != 0 && mode == PpuMode::HBlank);

        if irq_line && !self.prev_stat_irq_line {
            self.pending_interrupts |= 0x02;
        }
        self.prev_stat_irq_line = irq_line;
    }

    // ── Memory-mapped I/O ─────────────────────────────────────────────────────

    /// Read from VRAM address $8000–$9FFF.
    ///
    /// Returns 0xFF if the CPU is blocked (Mode 3 — Pixel Transfer).
    pub fn read_vram(&self, addr: u16) -> u8 {
        // LCD off: unrestricted access.
        if self.registers.lcdc & 0x80 != 0 && self.timing.mode() == PpuMode::PixelTransfer {
            return 0xFF;
        }
        self.vram[(addr - 0x8000) as usize]
    }

    /// Write to VRAM address $8000–$9FFF.
    ///
    /// Silently ignored if the CPU is blocked (Mode 3 — Pixel Transfer).
    pub fn write_vram(&mut self, addr: u16, val: u8) {
        // LCD off: unrestricted access.
        if self.registers.lcdc & 0x80 != 0 && self.timing.mode() == PpuMode::PixelTransfer {
            return;
        }
        self.vram[(addr - 0x8000) as usize] = val;
    }

    /// Read from OAM address $FE00–$FE9F.
    ///
    /// Returns 0xFF if the CPU is blocked (Mode 2 or 3).
    pub fn read_oam(&self, addr: u16) -> u8 {
        // LCD off: unrestricted access.
        if self.registers.lcdc & 0x80 != 0
            && matches!(
                self.timing.mode(),
                PpuMode::OamScan | PpuMode::PixelTransfer
            )
        {
            return 0xFF;
        }
        self.oam[(addr - 0xFE00) as usize]
    }

    /// Write to OAM address $FE00–$FE9F.
    ///
    /// Silently ignored if the CPU is blocked (Mode 2 or 3).
    pub fn write_oam(&mut self, addr: u16, val: u8) {
        // LCD off: unrestricted access.
        if self.registers.lcdc & 0x80 != 0
            && matches!(
                self.timing.mode(),
                PpuMode::OamScan | PpuMode::PixelTransfer
            )
        {
            return;
        }
        self.oam[(addr - 0xFE00) as usize] = val;
    }

    /// Read a PPU I/O register ($FF40–$FF4B).
    pub fn read_register(&self, addr: u16) -> u8 {
        let lcd_off = self.registers.lcdc & 0x80 == 0;
        // LY reads as 0 and mode bits read as 0 (HBlank) when LCD is off.
        let stat = if lcd_off {
            0x80
        } else {
            self.compose_stat_byte()
        };
        let ly = if lcd_off { 0 } else { self.timing.ly() };
        self.registers.read(addr, ly, stat).unwrap_or(0xFF)
    }

    /// Write a PPU I/O register ($FF40–$FF4B).
    pub fn write_register(&mut self, addr: u16, val: u8) {
        // Reset PPU timing when LCD transitions from off → on.
        if addr == 0xFF40 {
            let was_off = self.registers.lcdc & 0x80 == 0;
            let turning_on = val & 0x80 != 0;
            if was_off && turning_on {
                self.timing = Timing::new();
            }
        }
        self.registers.write(addr, val);
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

    // ── VRAM bus-conflict blocking ─────────────────────────────────────────────

    #[test]
    fn test_vram_readable_during_hblank() {
        // Given: a Ppu ticked to Mode 0 (H-Blank, dot 252 on scanline 0)
        let mut ppu = Ppu::new();
        ppu.write_vram(0x8010, 0xAB);
        tick_dots(&mut ppu, 252); // dot 252 → Mode 0
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        // When: read VRAM during H-Blank
        // Then: real value returned
        assert_eq!(ppu.read_vram(0x8010), 0xAB);
    }

    #[test]
    fn test_vram_blocked_during_pixel_transfer_returns_0xff() {
        // Given: a Ppu ticked into Mode 3 (dot 80, scanline 0)
        let mut ppu = Ppu::new();
        ppu.vram[0x0010] = 0xAB; // bypass write_vram to seed value
        tick_dots(&mut ppu, 80); // dot 80 → Mode 3
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: CPU attempts to read VRAM
        assert_eq!(ppu.read_vram(0x8010), 0xFF); // blocked
    }

    #[test]
    fn test_vram_write_blocked_during_pixel_transfer() {
        // Given: a Ppu ticked to Mode 3
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80);
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // When: CPU writes VRAM
        ppu.write_vram(0x8000, 0x42);
        // Then: write is ignored; VRAM unchanged
        tick_dots(&mut ppu, 172); // exit Mode 3
        assert_eq!(ppu.read_vram(0x8000), 0x00);
    }

    // ── OAM bus-conflict blocking ──────────────────────────────────────────────

    #[test]
    fn test_oam_blocked_during_oam_scan_returns_0xff() {
        // Given: a fresh Ppu in Mode 2 (OAM Scan) at dot 0
        let mut ppu = Ppu::new();
        ppu.oam[0] = 0x55;
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        // When: CPU reads OAM during Mode 2
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
    }

    #[test]
    fn test_oam_readable_during_hblank() {
        // Given: a Ppu at H-Blank (dot 252)
        let mut ppu = Ppu::new();
        ppu.oam[0] = 0x77;
        tick_dots(&mut ppu, 252);
        assert_eq!(ppu.timing.mode(), PpuMode::HBlank);
        // When: CPU reads OAM
        assert_eq!(ppu.read_oam(0xFE00), 0x77);
    }

    #[test]
    fn test_oam_write_blocked_during_oam_scan() {
        // Given: fresh Ppu in Mode 2
        let mut ppu = Ppu::new();
        assert_eq!(ppu.timing.mode(), PpuMode::OamScan);
        // When: write OAM
        ppu.write_oam(0xFE00, 0xAA);
        // Then: ignored
        tick_dots(&mut ppu, 252); // advance to H-Blank where reads are unblocked
        assert_eq!(ppu.read_oam(0xFE00), 0x00);
    }

    // ── VBlank interrupt ───────────────────────────────────────────────────────

    #[test]
    fn test_vblank_interrupt_fires_at_scanline_144() {
        // Given: a Ppu ticked to the last dot before scanline 144
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 143 + 455); // dot 455 of scanline 143
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
        tick_dots(&mut ppu, 456 * 4 + 456 - 1); // tick to dot 455 of scanline 4
        // Drain any earlier flags
        let _ = ppu.take_pending_interrupts();
        // When: advance to scanline 5 (LY becomes 5 = LYC)
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
        tick_dots(&mut ppu, 456 * 154 - 1);
        assert!(!ppu.is_frame_ready());
    }

    #[test]
    fn test_frame_ready_after_154_scanlines() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 154);
        assert!(ppu.is_frame_ready());
    }

    #[test]
    fn test_clear_frame_ready_resets_flag() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 154);
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
        // At startup: Mode 2 (OAM Scan)
        let ppu = Ppu::new();
        let stat = ppu.read_register(0xFF41);
        assert_eq!(stat & 0x03, PpuMode::OamScan as u8);
    }

    #[test]
    fn test_ly_register_returns_current_scanline() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 10); // advance 10 scanlines
        assert_eq!(ppu.read_register(0xFF44), 10);
    }

    // ── LCD-off behaviour ─────────────────────────────────────────────────────

    /// When the LCD is off (LCDC bit 7 = 0) the PPU timing must freeze: no
    /// VBlank interrupt fires even after a full frame's worth of dots.
    #[test]
    fn test_ppu_does_not_advance_when_lcd_is_off() {
        let mut ppu = Ppu::new();
        ppu.write_register(0xFF40, 0x11); // LCD off (bit 7 = 0)
        tick_dots(&mut ppu, 456 * 154); // full frame worth of dots
        assert_eq!(
            ppu.take_pending_interrupts() & 0x01,
            0,
            "no VBlank interrupt when LCD is off"
        );
    }

    /// LY ($FF44) must read as 0 while the LCD is off.
    #[test]
    fn test_ly_returns_0_when_lcd_is_off() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 10); // advance to scanline 10
        assert_eq!(ppu.read_register(0xFF44), 10);
        ppu.write_register(0xFF40, 0x11); // LCD off
        assert_eq!(
            ppu.read_register(0xFF44),
            0,
            "LY must read 0 when LCD is off"
        );
    }

    /// VRAM writes must succeed even when Mode 3 is active, as long as the LCD is off.
    /// This is the core bug: games turn off the LCD to safely fill VRAM.
    #[test]
    fn test_vram_write_accepted_when_lcd_is_off() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 80); // → PixelTransfer (Mode 3)
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        // Turn LCD off while in Mode 3
        ppu.write_register(0xFF40, 0x11); // LCDC bit 7 = 0
        // Write to VRAM — should NOT be dropped
        ppu.write_vram(0x8050, 0xAB);
        // Verify the write was accepted
        assert_eq!(
            ppu.vram[0x0050], 0xAB,
            "VRAM write must succeed when LCD is off"
        );
    }

    /// VRAM reads must return real data (not 0xFF) when the LCD is off.
    #[test]
    fn test_vram_read_returns_real_value_when_lcd_is_off() {
        let mut ppu = Ppu::new();
        ppu.vram[0x0010] = 0xAB; // seed value directly
        tick_dots(&mut ppu, 80); // → PixelTransfer
        assert_eq!(ppu.timing.mode(), PpuMode::PixelTransfer);
        ppu.write_register(0xFF40, 0x11); // LCD off
        assert_eq!(
            ppu.read_vram(0x8010),
            0xAB,
            "VRAM read must return real value when LCD is off"
        );
    }

    /// PPU timing must restart from dot 0 / scanline 0 when the LCD is re-enabled.
    #[test]
    fn test_ppu_timing_resets_when_lcd_is_re_enabled() {
        let mut ppu = Ppu::new();
        tick_dots(&mut ppu, 456 * 50); // advance to scanline 50
        ppu.write_register(0xFF40, 0x11); // LCD off
        tick_dots(&mut ppu, 100); // should be a no-op
        // Re-enable LCD
        ppu.write_register(0xFF40, 0x91); // LCD on
        assert_eq!(
            ppu.read_register(0xFF44),
            0,
            "LY must be 0 after LCD re-enable"
        );
    }
}
