/// Manages background rendering including shift registers, tile fetching, and pixel composition
pub struct Background {
    /// Background pattern shift register - low bit plane (16 bits)
    bg_pattern_shift_lo: u16,
    /// Background pattern shift register - high bit plane (16 bits)
    bg_pattern_shift_hi: u16,
    /// Background attribute shift register - low bit (16 bits)
    bg_attribute_shift_lo: u16,
    /// Background attribute shift register - high bit (16 bits)
    bg_attribute_shift_hi: u16,
    /// Nametable byte latch (tile index)
    nametable_latch: u8,
    /// Attribute table byte latch (palette selection)
    attribute_latch: u8,
    /// Pattern table low byte latch
    pattern_lo_latch: u8,
    /// Pattern table high byte latch
    pattern_hi_latch: u8,
}

impl Background {
    /// Create a new Background instance
    pub fn new() -> Self {
        Self {
            bg_pattern_shift_lo: 0,
            bg_pattern_shift_hi: 0,
            bg_attribute_shift_lo: 0,
            bg_attribute_shift_hi: 0,
            nametable_latch: 0,
            attribute_latch: 0,
            pattern_lo_latch: 0,
            pattern_hi_latch: 0,
        }
    }

    /// Reset background state
    pub fn reset(&mut self) {
        self.bg_pattern_shift_lo = 0;
        self.bg_pattern_shift_hi = 0;
        self.bg_attribute_shift_lo = 0;
        self.bg_attribute_shift_hi = 0;
        self.nametable_latch = 0;
        self.attribute_latch = 0;
        self.pattern_lo_latch = 0;
        self.pattern_hi_latch = 0;
    }

    /// Fetch nametable byte from memory
    pub fn fetch_nametable<F>(&mut self, v: u16, read_nametable: F)
    where
        F: Fn(u16) -> u8,
    {
        let addr = 0x2000 | (v & 0x0FFF);
        self.nametable_latch = read_nametable(addr);
    }

    /// Fetch attribute byte from memory
    pub fn fetch_attribute<F>(&mut self, v: u16, read_nametable: F)
    where
        F: Fn(u16) -> u8,
    {
        let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
        self.attribute_latch = read_nametable(addr);
    }

    /// Fetch pattern table low byte from CHR ROM
    pub fn fetch_pattern_lo<F>(&mut self, pattern_table_base: u16, v: u16, read_chr: F)
    where
        F: Fn(u16) -> u8,
    {
        let tile_index = self.nametable_latch as u16;
        let fine_y = (v >> 12) & 0x07;
        let addr = pattern_table_base | (tile_index << 4) | fine_y;
        self.pattern_lo_latch = read_chr(addr);
    }

    /// Fetch pattern table high byte from CHR ROM
    pub fn fetch_pattern_hi<F>(&mut self, pattern_table_base: u16, v: u16, read_chr: F)
    where
        F: Fn(u16) -> u8,
    {
        let tile_index = self.nametable_latch as u16;
        let fine_y = (v >> 12) & 0x07;
        let addr = pattern_table_base | (tile_index << 4) | fine_y | 0x08;
        self.pattern_hi_latch = read_chr(addr);
    }

    /// Load shift registers from latches
    pub fn load_shift_registers(&mut self, v: u16) {
        // Load pattern data into low 8 bits of 16-bit shift registers
        self.bg_pattern_shift_lo = (self.bg_pattern_shift_lo & 0xFF00) | (self.pattern_lo_latch as u16);
        self.bg_pattern_shift_hi = (self.bg_pattern_shift_hi & 0xFF00) | (self.pattern_hi_latch as u16);

        // Extract the correct 2-bit palette from the attribute byte
        let coarse_x = v & 0x1F;
        let coarse_y = (v >> 5) & 0x1F;
        let shift = ((coarse_y & 0x02) << 1) | (coarse_x & 0x02);
        let palette = (self.attribute_latch >> shift) & 0x03;

        // Load attribute data into low 8 bits
        let palette_lo_bits = if (palette & 0x01) != 0 { 0xFF } else { 0x00 };
        let palette_hi_bits = if (palette & 0x02) != 0 { 0xFF } else { 0x00 };

        self.bg_attribute_shift_lo = (self.bg_attribute_shift_lo & 0xFF00) | (palette_lo_bits as u16);
        self.bg_attribute_shift_hi = (self.bg_attribute_shift_hi & 0xFF00) | (palette_hi_bits as u16);
    }

    /// Shift all background rendering shift registers left by 1
    pub fn shift_registers(&mut self) {
        self.bg_pattern_shift_lo <<= 1;
        self.bg_pattern_shift_hi <<= 1;
        self.bg_attribute_shift_lo <<= 1;
        self.bg_attribute_shift_hi <<= 1;
    }

    /// Get the current background pixel value
    /// Returns a palette index (0-15) where 0 means transparent
    pub fn get_pixel(&self, fine_x: u8) -> u8 {
        let bit_position = 15 - fine_x;

        // Extract pattern bits
        let pattern_lo_bit = ((self.bg_pattern_shift_lo >> bit_position) & 0x01) as u8;
        let pattern_hi_bit = ((self.bg_pattern_shift_hi >> bit_position) & 0x01) as u8;
        let pattern = (pattern_hi_bit << 1) | pattern_lo_bit;

        // If pattern is 0, pixel is transparent
        if pattern == 0 {
            return 0;
        }

        // Extract attribute bits
        let attr_lo_bit = ((self.bg_attribute_shift_lo >> bit_position) & 0x01) as u8;
        let attr_hi_bit = ((self.bg_attribute_shift_hi >> bit_position) & 0x01) as u8;
        let palette = (attr_hi_bit << 1) | attr_lo_bit;

        // Combine: palette * 4 + pattern
        palette * 4 + pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_new() {
        let bg = Background::new();
        assert_eq!(bg.get_pixel(0), 0);
    }

    #[test]
    fn test_background_reset() {
        let mut bg = Background::new();
        bg.nametable_latch = 42;
        bg.reset();
        assert_eq!(bg.nametable_latch, 0);
    }

    #[test]
    fn test_fetch_nametable() {
        let mut bg = Background::new();
        bg.fetch_nametable(0x2000, |_| 0x42);
        assert_eq!(bg.nametable_latch, 0x42);
    }

    #[test]
    fn test_shift_registers() {
        let mut bg = Background::new();
        bg.bg_pattern_shift_lo = 0x8000;
        bg.shift_registers();
        assert_eq!(bg.bg_pattern_shift_lo, 0);
    }

    #[test]
    fn test_load_shift_registers() {
        let mut bg = Background::new();
        bg.pattern_lo_latch = 0xFF;
        bg.pattern_hi_latch = 0xFF;
        bg.attribute_latch = 0x03;
        bg.load_shift_registers(0);
        assert_eq!(bg.bg_pattern_shift_lo & 0xFF, 0xFF);
        assert_eq!(bg.bg_pattern_shift_hi & 0xFF, 0xFF);
    }

    #[test]
    fn test_get_pixel_transparent() {
        let bg = Background::new();
        assert_eq!(bg.get_pixel(0), 0);
    }

    #[test]
    fn test_get_pixel_with_pattern() {
        let mut bg = Background::new();
        bg.bg_pattern_shift_lo = 0x8000;
        bg.bg_pattern_shift_hi = 0x8000;
        let pixel = bg.get_pixel(0);
        assert_eq!(pixel, 3); // Pattern 3, palette 0
    }
}
