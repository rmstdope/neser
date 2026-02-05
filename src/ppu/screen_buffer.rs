/// ScreenBuffer holds RGB values for each pixel on the screen.
pub struct ScreenBuffer {
    buffer: Vec<u8>,
}

impl Default for ScreenBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenBuffer {
    const WIDTH: u32 = 256;
    const HEIGHT: u32 = 240;
    const BYTES_PER_PIXEL: usize = 3; // RGB

    /// Creates a new ScreenBuffer with hardcoded NES dimensions (256x240).
    pub fn new() -> Self {
        let buffer_size = (Self::WIDTH * Self::HEIGHT) as usize * Self::BYTES_PER_PIXEL;

        ScreenBuffer {
            buffer: vec![0; buffer_size],
        }
    }

    /// Returns the width of the screen buffer.
    #[cfg(test)]
    pub fn width(&self) -> u32 {
        Self::WIDTH
    }

    /// Returns the height of the screen buffer.
    #[cfg(test)]
    pub fn height(&self) -> u32 {
        Self::HEIGHT
    }

    /// Calculates the buffer offset for a given pixel coordinate.
    fn pixel_offset(&self, x: u32, y: u32) -> usize {
        ((y * Self::WIDTH + x) as usize) * Self::BYTES_PER_PIXEL
    }

    /// Sets the RGB color of a pixel at the specified coordinates.
    ///
    /// # Arguments
    ///
    /// * `x` - The x coordinate (0-255)
    /// * `y` - The y coordinate (0-239)
    /// * `r` - Red component (0-255)
    /// * `g` - Green component (0-255)
    /// * `b` - Blue component (0-255)
    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        let offset = self.pixel_offset(x, y);

        self.buffer[offset] = r;
        self.buffer[offset + 1] = g;
        self.buffer[offset + 2] = b;
    }

    /// Gets the RGB color of a pixel at the specified coordinates.
    ///
    /// # Arguments
    ///
    /// * `x` - The x coordinate (0-255)
    /// * `y` - The y coordinate (0-239)
    ///
    /// # Returns
    ///
    /// A tuple containing the (r, g, b) color components
    pub fn get_pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        let offset = self.pixel_offset(x, y);
        (
            self.buffer[offset],
            self.buffer[offset + 1],
            self.buffer[offset + 2],
        )
    }

    /// Calculates the luminance of a pixel at the specified coordinates.
    /// Uses the Rec. 709 formula for perceptual brightness.
    ///
    /// # Arguments
    ///
    /// * `x` - The x coordinate (0-255)
    /// * `y` - The y coordinate (0-239)
    ///
    /// # Returns
    ///
    /// A luminance value between 0.0 (black) and 255.0 (white)
    pub fn get_luminance(&self, x: u32, y: u32) -> f32 {
        let (r, g, b) = self.get_pixel(x, y);
        // Rec. 709 luma coefficients for perceptual brightness
        0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
    }

    /// Copies the entire buffer to the specified destination buffer.
    ///
    /// # Arguments
    ///
    /// * `dest` - Destination buffer slice to copy to. Must be at least as large as the source buffer.
    pub fn copy_buffer(&self, dest: &mut [u8]) {
        assert!(
            dest.len() >= self.buffer.len(),
            "Destination buffer is too small: need {}, got {}",
            self.buffer.len(),
            dest.len()
        );

        // let len = self.buffer.len();
        // dest[..len].copy_from_slice(&self.buffer);

        dest[..self.buffer.len()].copy_from_slice(&self.buffer);

        // // Display debug pixels to help count and pinpoint positions
        // for x in 11usize..=12 {
        //     const Y_LINE: usize = 1;
        //     let offset = ((Y_LINE * Self::WIDTH as usize) + x) * Self::BYTES_PER_PIXEL;
        //     dest[offset] = if x.is_multiple_of(2) { 0xFF } else { 0x00 };
        //     dest[offset + 1] = 0xFF;
        //     dest[offset + 2] = if x.is_multiple_of(2) { 0x00 } else { 0xFF };
        // }
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    pub fn crc32(&self) -> u32 {
        crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(&self.buffer)
    }

    pub fn restore_from_snapshot(&mut self, data: &[u8]) {
        let len = data.len().min(self.buffer.len());
        self.buffer[..len].copy_from_slice(&data[..len]);
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenBufferDebugState {
    pub buffer: Vec<u8>,
}

#[cfg(test)]
impl ScreenBuffer {
    pub fn debug_state(&self) -> ScreenBufferDebugState {
        ScreenBufferDebugState {
            buffer: self.buffer.clone(),
        }
    }

    pub fn set_debug_state(&mut self, state: ScreenBufferDebugState) {
        self.buffer = state.buffer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_screen_buffer() {
        let screen_buffer = ScreenBuffer::new();

        // Verify dimensions are NES screen size (256x240)
        assert_eq!(screen_buffer.width(), 256);
        assert_eq!(screen_buffer.height(), 240);
    }

    #[test]
    fn test_set_and_get_pixel() {
        let mut screen_buffer = ScreenBuffer::new();

        // Test setting and getting different pixels at various locations
        // Top-left corner
        screen_buffer.set_pixel(0, 0, 255, 0, 0);
        let (r, g, b) = screen_buffer.get_pixel(0, 0);
        assert_eq!((r, g, b), (255, 0, 0));

        // Top-right corner
        screen_buffer.set_pixel(255, 0, 0, 255, 0);
        let (r, g, b) = screen_buffer.get_pixel(255, 0);
        assert_eq!((r, g, b), (0, 255, 0));

        // Bottom-left corner
        screen_buffer.set_pixel(0, 239, 0, 0, 255);
        let (r, g, b) = screen_buffer.get_pixel(0, 239);
        assert_eq!((r, g, b), (0, 0, 255));

        // Bottom-right corner
        screen_buffer.set_pixel(255, 239, 128, 64, 32);
        let (r, g, b) = screen_buffer.get_pixel(255, 239);
        assert_eq!((r, g, b), (128, 64, 32));

        // Middle of screen
        screen_buffer.set_pixel(128, 120, 200, 100, 50);
        let (r, g, b) = screen_buffer.get_pixel(128, 120);
        assert_eq!((r, g, b), (200, 100, 50));

        // Verify that setting one pixel doesn't affect another
        let (r, g, b) = screen_buffer.get_pixel(0, 0);
        assert_eq!((r, g, b), (255, 0, 0)); // Should still be red
    }

    #[test]
    fn test_initial_pixels_are_black() {
        let screen_buffer = ScreenBuffer::new();

        // Test various positions to ensure they're initialized to black (0, 0, 0)
        let (r, g, b) = screen_buffer.get_pixel(0, 0);
        assert_eq!((r, g, b), (0, 0, 0));

        let (r, g, b) = screen_buffer.get_pixel(100, 100);
        assert_eq!((r, g, b), (0, 0, 0));

        let (r, g, b) = screen_buffer.get_pixel(255, 239);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn test_copy_buffer() {
        let mut source = ScreenBuffer::new();

        // Set some pixels in source buffer
        source.set_pixel(0, 0, 255, 0, 0);
        source.set_pixel(10, 10, 0, 255, 0);
        source.set_pixel(100, 100, 0, 0, 255);
        source.set_pixel(255, 239, 128, 64, 32);

        // Create destination buffer
        let mut dest_buffer = vec![0u8; 256 * 240 * 3];

        // Copy the buffer
        source.copy_buffer(&mut dest_buffer);

        // Verify pixels were copied correctly
        // Pixel at (0, 0) - offset 0
        assert_eq!(dest_buffer[0], 255);
        assert_eq!(dest_buffer[1], 0);
        assert_eq!(dest_buffer[2], 0);

        // Pixel at (10, 10) - offset (10 * 256 + 10) * 3 = 7710
        let offset_10_10 = (10 * 256 + 10) * 3;
        assert_eq!(dest_buffer[offset_10_10], 0);
        assert_eq!(dest_buffer[offset_10_10 + 1], 255);
        assert_eq!(dest_buffer[offset_10_10 + 2], 0);

        // Pixel at (100, 100) - offset (100 * 256 + 100) * 3 = 76900
        let offset_100_100 = (100 * 256 + 100) * 3;
        assert_eq!(dest_buffer[offset_100_100], 0);
        assert_eq!(dest_buffer[offset_100_100 + 1], 0);
        assert_eq!(dest_buffer[offset_100_100 + 2], 255);

        // Pixel at (255, 239) - last pixel
        let offset_255_239 = (239 * 256 + 255) * 3;
        assert_eq!(dest_buffer[offset_255_239], 128);
        assert_eq!(dest_buffer[offset_255_239 + 1], 64);
        assert_eq!(dest_buffer[offset_255_239 + 2], 32);
    }

    #[test]
    fn test_copy_buffer_does_not_modify_source() {
        let mut source = ScreenBuffer::new();

        // Pick a pixel in the region that should be copied verbatim.
        // This test also guards against accidental debug drawing inside `copy_buffer()`.
        source.set_pixel(0, 148, 1, 2, 3);
        let before = source.get_pixel(0, 148);

        let mut dest_buffer = vec![0u8; 256 * 240 * 3];
        source.copy_buffer(&mut dest_buffer);

        let after = source.get_pixel(0, 148);
        assert_eq!(
            after, before,
            "copy_buffer must not mutate the source buffer"
        );
    }

    #[test]
    fn test_crc32_for_blank_frame() {
        let screen_buffer = ScreenBuffer::new();
        let crc = screen_buffer.crc32();
        assert_eq!(crc, 0xB77D_18AB);
    }

    #[test]
    fn test_get_luminance_for_black() {
        let screen_buffer = ScreenBuffer::new();
        let luminance = screen_buffer.get_luminance(0, 0);
        assert_eq!(luminance, 0.0);
    }

    #[test]
    fn test_get_luminance_for_white() {
        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(10, 10, 255, 255, 255);
        let luminance = screen_buffer.get_luminance(10, 10);
        assert_eq!(luminance, 255.0);
    }

    #[test]
    fn test_get_luminance_for_red() {
        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(20, 20, 255, 0, 0);
        let luminance = screen_buffer.get_luminance(20, 20);
        // Red contributes 0.2126 * 255 = 54.213
        assert!((luminance - 54.213).abs() < 0.01);
    }

    #[test]
    fn test_get_luminance_for_green() {
        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(30, 30, 0, 255, 0);
        let luminance = screen_buffer.get_luminance(30, 30);
        // Green contributes 0.7152 * 255 = 182.376
        assert!((luminance - 182.376).abs() < 0.01);
    }

    #[test]
    fn test_get_luminance_for_blue() {
        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(40, 40, 0, 0, 255);
        let luminance = screen_buffer.get_luminance(40, 40);
        // Blue contributes 0.0722 * 255 = 18.411
        assert!((luminance - 18.411).abs() < 0.01);
    }

    #[test]
    fn test_get_luminance_for_mixed_color() {
        let mut screen_buffer = ScreenBuffer::new();
        screen_buffer.set_pixel(50, 50, 128, 200, 64);
        let luminance = screen_buffer.get_luminance(50, 50);
        // 0.2126 * 128 + 0.7152 * 200 + 0.0722 * 64 = 27.2128 + 143.04 + 4.6208 = 174.8736
        assert!((luminance - 174.8736).abs() < 0.01);
    }
}
