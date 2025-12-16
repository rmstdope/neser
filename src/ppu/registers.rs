/// PPU Control Register ($2000) bit constants
const GENERATE_NMI: u8 = 0b1000_0000;
const SPRITE_SIZE: u8 = 0b0010_0000;
const BG_PATTERN_TABLE_ADDR: u8 = 0b0001_0000;
const SPRITE_PATTERN_TABLE_ADDR: u8 = 0b0000_1000;
const VRAM_ADDR_INCREMENT: u8 = 0b0000_0100;
const BASE_NAMETABLE_ADDR: u8 = 0b0000_0011;

/// PPU Mask Register ($2001) bit constants
const SHOW_BACKGROUND: u8 = 0b0000_1000;
const SHOW_SPRITES: u8 = 0b0001_0000;
const SHOW_BACKGROUND_LEFT: u8 = 0b0000_0010;
const SHOW_SPRITES_LEFT: u8 = 0b0000_0100;
const GRAYSCALE: u8 = 0b0000_0001;
// const EMPHASIZE_GREEN: u8 = 0b0100_0000;
// const EMPHASIZE_BLUE: u8 = 0b1000_0000;

/// Manages PPU registers including PPUCTRL, PPUMASK, and Loopy scroll registers
pub struct Registers {
    /// Control register value ($2000)
    control_register: u8,
    /// Mask register value ($2001)
    mask_register: u8,
    /// OAM address register ($2003)
    pub oam_address: u8,
    /// PPU data read buffer
    data_buffer: u8,
    /// PPU I/O bus latch - holds last value written or read from PPU registers
    /// This is separate from the CPU data bus!
    io_bus: u8,
    /// v: Current VRAM address (15 bits)
    v: u16,
    /// t: Temporary VRAM address (15 bits)
    t: u16,
    /// x: Fine X scroll (3 bits)
    x: u8,
    /// w: Write toggle (1 bit) - false=first write, true=second write
    w: bool,
}

impl Registers {
    /// Create a new Registers instance
    pub fn new() -> Self {
        Self {
            control_register: 0,
            mask_register: 0,
            oam_address: 0,
            data_buffer: 0,
            io_bus: 0,
            v: 0,
            t: 0,
            x: 0,
            w: false,
        }
    }

    /// Reset registers to initial state
    pub fn reset(&mut self) {
        self.control_register = 0;
        self.mask_register = 0;
        self.oam_address = 0;
        self.data_buffer = 0;
        self.io_bus = 0;
        self.v = 0;
        self.t = 0;
        self.x = 0;
        self.w = false;
    }

    /// Write to control register ($2000)
    pub fn write_control(&mut self, value: u8) {
        self.control_register = value;
        // t: ...GH.. ........ <- d: ......GH
        // (Update nametable select bits in t register)
        let nametable_bits = (value & BASE_NAMETABLE_ADDR) as u16;
        self.t = (self.t & 0xF3FF) | (nametable_bits << 10);
    }

    /// Write to mask register ($2001)
    pub fn write_mask(&mut self, value: u8) {
        self.mask_register = value;
    }

    /// Get the current PPU I/O bus latch value
    pub fn io_bus(&self) -> u8 {
        self.io_bus
    }

    /// Update the PPU I/O bus latch (called on PPU register reads/writes)
    pub fn set_io_bus(&mut self, value: u8) {
        self.io_bus = value;
    }

    /// Write to scroll register ($2005)
    pub fn write_scroll(&mut self, value: u8, is_dummy_write: bool) {
        // Dummy writes from RMW instructions DO toggle w and modify registers
        // They just write the unmodified value (which was already there)
        // So the behavior is the same as a normal write - no special handling needed
        let _ = is_dummy_write; // Dummy writes behave identically to normal writes
        
        if !self.w {
            // First write: X scroll
            // t: ....... ...ABCDE <- d: ABCDE...
            // x:              FGH <- d: .....FGH
            // w:                  <- 1
            self.t = (self.t & 0xFFE0) | ((value as u16) >> 3);
            self.x = value & 0x07;
            self.w = true;
        } else {
            // Second write: Y scroll
            // t: FGH..AB CDE..... <- d: ABCDEFGH
            // w:                  <- 0
            self.t = (self.t & 0x8FFF) | (((value as u16) & 0x07) << 12);
            self.t = (self.t & 0xFC1F) | (((value as u16) & 0xF8) << 2);
            self.w = false;
        }
    }

    /// Write to address register ($2006)
    pub fn write_address(&mut self, value: u8, is_dummy_write: bool) {
        // Dummy writes from RMW instructions DO toggle w and modify registers
        // They just write the unmodified value (which was already there)
        // So the behavior is the same as a normal write - no special handling needed
        let _ = is_dummy_write; // Dummy writes behave identically to normal writes
        
        if !self.w {
            // First write: high byte
            // t: .CDEFGH ........ <- d: ..CDEFGH
            //        <-- d: AB......
            // t: Z...... ........ <- 0 (bit 14 is cleared)
            // w:                  <- 1
            self.t = (self.t & 0x80FF) | (((value & 0x3F) as u16) << 8);
            self.w = true;
        } else {
            // Second write: low byte
            // t: ....... ABCDEFGH <- d: ABCDEFGH
            // v: <--t
            // w:                  <- 0
            self.t = (self.t & 0xFF00) | (value as u16);
            self.v = self.t;
            self.w = false;
        }
    }

    /// Read data buffer
    pub fn data_buffer(&self) -> u8 {
        self.data_buffer
    }

    /// Set data buffer
    pub fn set_data_buffer(&mut self, value: u8) {
        self.data_buffer = value;
    }

    /// Increment VRAM address by the amount specified in control register
    pub fn increment_vram_address(&mut self) {
        let increment = if (self.control_register & VRAM_ADDR_INCREMENT) != 0 {
            32
        } else {
            1
        };
        self.v = self.v.wrapping_add(increment) & 0x3FFF;
    }

    /// Increment coarse X (used during rendering)
    pub fn increment_coarse_x(&mut self) {
        if (self.v & 0x001F) == 31 {
            // Coarse X = 0, switch horizontal nametable
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            // Increment coarse X
            self.v += 1;
        }
    }

    /// Increment fine Y (used during rendering)
    pub fn increment_fine_y(&mut self) {
        if (self.v & 0x7000) != 0x7000 {
            // Fine Y < 7, just increment
            self.v += 0x1000;
        } else {
            // Fine Y = 7, reset and increment coarse Y
            self.v &= !0x7000;
            let mut y = (self.v & 0x03E0) >> 5;
            if y == 29 {
                // Coarse Y = 29, reset and switch vertical nametable
                y = 0;
                self.v ^= 0x0800;
            } else if y == 31 {
                // Coarse Y = 31, reset (no nametable switch)
                y = 0;
            } else {
                // Normal increment
                y += 1;
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }

    /// Copy horizontal bits from t to v (used during rendering)
    pub fn copy_horizontal_bits(&mut self) {
        // v: ....A.. ...BCDEF <- t: ....A.. ...BCDEF
        self.v = (self.v & 0xFBE0) | (self.t & 0x041F);
    }

    /// Copy vertical bits from t to v (used during rendering)
    pub fn copy_vertical_bits(&mut self) {
        // v: GHIA.BC DEF..... <- t: GHIA.BC DEF.....
        self.v = (self.v & 0x841F) | (self.t & 0x7BE0);
    }

    /// Increment v register using the rendering glitch pattern
    /// During rendering, PPUDATA access increments both coarse X and fine Y
    /// This is a hardware quirk that some games rely on
    pub fn inc_address_with_rendering_glitch(&mut self) {
        self.increment_coarse_x();
        self.increment_fine_y();
    }

    /// Get current VRAM address
    pub fn v(&self) -> u16 {
        self.v
    }

    /// Get temporary VRAM address
    pub fn t(&self) -> u16 {
        self.t
    }

    /// Get fine X scroll
    pub fn x(&self) -> u8 {
        self.x
    }

    /// Get write toggle
    pub fn w(&self) -> bool {
        self.w
    }

    /// Clear write toggle (used when reading status)
    pub fn clear_w(&mut self) {
        self.w = false;
    }

    /// Check if NMI should be generated
    pub fn should_generate_nmi(&self) -> bool {
        (self.control_register & GENERATE_NMI) != 0
    }

    /// Check if background rendering is enabled
    pub fn is_background_enabled(&self) -> bool {
        (self.mask_register & SHOW_BACKGROUND) != 0
    }

    /// Check if sprite rendering is enabled
    pub fn is_sprite_enabled(&self) -> bool {
        (self.mask_register & SHOW_SPRITES) != 0
    }

    /// Check if rendering is enabled (background or sprites)
    pub fn is_rendering_enabled(&self) -> bool {
        self.is_background_enabled() || self.is_sprite_enabled()
    }

    /// Check if background should be shown in leftmost 8 pixels
    pub fn show_background_left(&self) -> bool {
        (self.mask_register & SHOW_BACKGROUND_LEFT) != 0
    }

    /// Check if sprites should be shown in leftmost 8 pixels
    pub fn show_sprites_left(&self) -> bool {
        (self.mask_register & SHOW_SPRITES_LEFT) != 0
    }

    /// Check if grayscale mode is enabled
    pub fn is_grayscale(&self) -> bool {
        (self.mask_register & GRAYSCALE) != 0
    }

    /// Get color emphasis bits
    pub fn color_emphasis(&self) -> u8 {
        (self.mask_register >> 5) & 0x07
    }

    /// Get sprite size (0=8x8, 1=8x16)
    pub fn sprite_height(&self) -> u8 {
        if (self.control_register & SPRITE_SIZE) != 0 {
            16
        } else {
            8
        }
    }

    /// Get background pattern table address
    pub fn bg_pattern_table_addr(&self) -> u16 {
        if (self.control_register & BG_PATTERN_TABLE_ADDR) != 0 {
            0x1000
        } else {
            0x0000
        }
    }

    /// Get sprite pattern table address (ignored in 8x16 mode)
    pub fn sprite_pattern_table_addr(&self) -> u16 {
        if (self.control_register & SPRITE_PATTERN_TABLE_ADDR) != 0 {
            0x1000
        } else {
            0x0000
        }
    }

    /// Get control register value
    pub fn control(&self) -> u8 {
        self.control_register
    }

    /// Get base nametable address from control register bits 0-1
    /// Returns: 0x2000, 0x2400, 0x2800, or 0x2C00
    pub fn base_nametable_addr(&self) -> u16 {
        let nametable_select = (self.control_register & BASE_NAMETABLE_ADDR) as u16;
        0x2000 | (nametable_select << 10)
    }

    /// Get mask register value
    pub fn mask(&self) -> u8 {
        self.mask_register
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registers_new() {
        let regs = Registers::new();
        assert_eq!(regs.control(), 0);
        assert_eq!(regs.mask(), 0);
        assert_eq!(regs.v(), 0);
        assert_eq!(regs.t(), 0);
    }

    #[test]
    fn test_write_control() {
        let mut regs = Registers::new();
        regs.write_control(0b0000_0011);
        assert_eq!(regs.t() & 0x0C00, 0x0C00);
    }

    #[test]
    fn test_write_scroll_first() {
        let mut regs = Registers::new();
        regs.write_scroll(0b11111111, false);
        assert_eq!(regs.t() & 0x1F, 0b11111);
        assert_eq!(regs.x(), 0b111);
        assert!(regs.w());
    }

    #[test]
    fn test_write_scroll_second() {
        let mut regs = Registers::new();
        regs.write_scroll(0, false);
        regs.write_scroll(0b11111111, false);
        assert!(!regs.w());
    }

    #[test]
    fn test_write_address() {
        let mut regs = Registers::new();
        regs.write_address(0x3F, false);
        regs.write_address(0x00, false);
        assert_eq!(regs.v(), 0x3F00);
    }

    #[test]
    fn test_increment_coarse_x() {
        let mut regs = Registers::new();
        regs.v = 0;
        regs.increment_coarse_x();
        assert_eq!(regs.v() & 0x1F, 1);
    }

    #[test]
    fn test_increment_coarse_x_wrap() {
        let mut regs = Registers::new();
        regs.v = 31;
        regs.increment_coarse_x();
        assert_eq!(regs.v() & 0x1F, 0);
        assert_eq!(regs.v() & 0x0400, 0x0400);
    }

    #[test]
    fn test_increment_fine_y() {
        let mut regs = Registers::new();
        regs.v = 0;
        regs.increment_fine_y();
        assert_eq!(regs.v() & 0x7000, 0x1000);
    }

    #[test]
    fn test_copy_horizontal_bits() {
        let mut regs = Registers::new();
        regs.t = 0x041F;
        regs.v = 0;
        regs.copy_horizontal_bits();
        assert_eq!(regs.v() & 0x041F, 0x041F);
    }

    #[test]
    fn test_copy_vertical_bits() {
        let mut regs = Registers::new();
        regs.t = 0x7BE0;
        regs.v = 0;
        regs.copy_vertical_bits();
        assert_eq!(regs.v() & 0x7BE0, 0x7BE0);
    }

    #[test]
    fn test_sprite_height_8x8() {
        let mut regs = Registers::new();
        regs.write_control(0);
        assert_eq!(regs.sprite_height(), 8);
    }

    #[test]
    fn test_sprite_height_8x16() {
        let mut regs = Registers::new();
        regs.write_control(SPRITE_SIZE);
        assert_eq!(regs.sprite_height(), 16);
    }

    #[test]
    fn test_is_grayscale() {
        let mut regs = Registers::new();
        regs.write_mask(GRAYSCALE);
        assert!(regs.is_grayscale());
    }

    const EMPHASIZE_RED: u8 = 0b0010_0000;

    #[test]
    fn test_color_emphasis() {
        let mut regs = Registers::new();
        regs.write_mask(EMPHASIZE_RED);
        assert_eq!(regs.color_emphasis() & 0x01, 0x01);
    }
}
