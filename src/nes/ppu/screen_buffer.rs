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
    #[cfg(test)]
    pub fn copy_buffer(&self, dest: &mut [u8]) {
        assert!(
            dest.len() >= self.buffer.len(),
            "Destination buffer is too small: need {}, got {}",
            self.buffer.len(),
            dest.len()
        );

        dest[..self.buffer.len()].copy_from_slice(&self.buffer);
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    /// Returns a cropped snapshot with the given overscan removed from all edges.
    ///
    /// `h_overscan` pixels are removed from the left and right edges.
    /// `v_overscan` pixels are removed from the top and bottom edges.
    /// The returned buffer has dimensions `(256 - 2*h_overscan) × (240 - 2*v_overscan)` in RGB888.
    pub fn cropped_snapshot(&self, h_overscan: u32, v_overscan: u32) -> Vec<u8> {
        let src_w = Self::WIDTH;
        let dst_w = src_w - 2 * h_overscan;
        let dst_h = Self::HEIGHT - 2 * v_overscan;
        let mut out = Vec::with_capacity((dst_w * dst_h) as usize * Self::BYTES_PER_PIXEL);
        for row in v_overscan..v_overscan + dst_h {
            let row_start = (row * src_w + h_overscan) as usize * Self::BYTES_PER_PIXEL;
            let row_end = row_start + dst_w as usize * Self::BYTES_PER_PIXEL;
            out.extend_from_slice(&self.buffer[row_start..row_end]);
        }
        out
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

    #[test]
    fn test_cropped_snapshot_no_overscan_returns_full_frame() {
        let mut buf = ScreenBuffer::new();
        buf.set_pixel(0, 0, 10, 20, 30);
        buf.set_pixel(255, 239, 40, 50, 60);
        let cropped = buf.cropped_snapshot(0, 0);
        assert_eq!(cropped.len(), 256 * 240 * 3);
        assert_eq!(&cropped[0..3], &[10, 20, 30]);
    }

    #[test]
    fn test_cropped_snapshot_default_overscan_produces_240x224_frame() {
        let buf = ScreenBuffer::new();
        let h: u32 = 8;
        let v: u32 = 8;
        let cropped = buf.cropped_snapshot(h, v);
        let expected_w = 256 - 2 * h; // 240
        let expected_h = 240 - 2 * v; // 224
        assert_eq!(cropped.len() as u32, expected_w * expected_h * 3);
    }

    #[test]
    fn test_cropped_snapshot_first_visible_pixel_is_at_overscan_offset() {
        let mut buf = ScreenBuffer::new();
        // Mark the first pixel inside the overscan boundary
        buf.set_pixel(8, 8, 1, 2, 3);
        // Mark a pixel inside the left overscan (should not appear in cropped output)
        buf.set_pixel(0, 8, 255, 0, 0);
        let cropped = buf.cropped_snapshot(8, 8);
        assert_eq!(&cropped[0..3], &[1, 2, 3]);
    }

    #[test]
    fn test_cropped_snapshot_last_visible_pixel_is_before_overscan_boundary() {
        let mut buf = ScreenBuffer::new();
        let h: u32 = 8;
        let v: u32 = 8;
        // Last visible pixel: (255 - h, 239 - v) = (247, 231)
        buf.set_pixel(247, 231, 7, 8, 9);
        let cropped = buf.cropped_snapshot(h, v);
        let expected_w = (256 - 2 * h) as usize;
        let expected_h = (240 - 2 * v) as usize;
        let last_offset = (expected_h - 1) * expected_w * 3 + (expected_w - 1) * 3;
        assert_eq!(&cropped[last_offset..last_offset + 3], &[7, 8, 9]);
    }

    #[test]
    fn test_cropped_snapshot_right_overscan_pixel_excluded() {
        let mut buf = ScreenBuffer::new();
        // Pixel at x=248 is in the right overscan region (h=8 → right starts at 256-8=248)
        buf.set_pixel(248, 8, 99, 0, 0);
        let cropped = buf.cropped_snapshot(8, 8);
        let expected_w = (256 - 2 * 8) as usize; // 240
        // Row 0 of cropped (row 8 of original) has 240 pixels; none should be 99
        for x in 0..expected_w {
            assert_ne!(
                cropped[x * 3],
                99,
                "overscan pixel should not appear at x={x}"
            );
        }
    }

    #[test]
    fn test_cropped_snapshot_max_horizontal_overscan() {
        let buf = ScreenBuffer::new();
        let cropped = buf.cropped_snapshot(8, 0);
        assert_eq!(cropped.len(), (256 - 16) * 240 * 3); // 240 * 240 * 3
    }

    #[test]
    fn test_cropped_snapshot_max_vertical_overscan() {
        let buf = ScreenBuffer::new();
        let cropped = buf.cropped_snapshot(0, 16);
        assert_eq!(cropped.len(), 256 * (240 - 32) * 3); // 256 * 208 * 3
    }
}
