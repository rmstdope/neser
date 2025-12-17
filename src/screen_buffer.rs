/// ScreenBuffer holds RGB values for each pixel on the screen.
pub struct ScreenBuffer {
    buffer: Vec<u8>,
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
    
        // Debug: Track writes to offset 21 (which is x=7, y=0)
        if offset == 21 {
            static mut OFFSET_21_WRITES: u32 = 0;
            unsafe {
                let write_count = OFFSET_21_WRITES + 1;
                OFFSET_21_WRITES = write_count;
                if write_count <= 10 || (r == 0 && g == 0 && b == 0) {
                    println!("Writing to offset 21: x={}, y={}, rgb=({},{},{}), write #{}", x, y, r, g, b, write_count);
                }
            }
        }
        
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
        let result = (
            self.buffer[offset],
            self.buffer[offset + 1],
            self.buffer[offset + 2],
        );
        // Debug: Log first few reads on scanline 0
        if y == 0 && x >= 7 && x < 12 {
            static mut GET_LOG_COUNT: u32 = 0;
            unsafe {
                if GET_LOG_COUNT < 5 {
                    println!("get_pixel: x={}, y={}, offset={}, rgb={:?}, buffer[offset]={:02X} {:02X} {:02X}",
                        x, y, offset, result,
                        self.buffer[offset], self.buffer[offset+1], self.buffer[offset+2]);
                    GET_LOG_COUNT += 1;
                }
            }
        }
        result
    }

    /// Copies the entire buffer to the specified destination buffer.
    ///
    /// # Arguments
    ///
    /// * `dest` - Destination buffer slice to copy to. Must be at least as large as the source buffer.
    pub fn copy_buffer(&mut self, dest: &mut [u8]) {
        // // Set pixels at y=10, x=[0..7] to red for testing
        // for y in 148..150 {
        //     for x in 0..8 {
        //         self.set_pixel(x, y, 255, 0, 0);
        //     }
        //     for x in 8..14 {
        //         self.set_pixel(x, y, 0, 255, 0);
        //     }
        // }
        dest[..self.buffer.len()].copy_from_slice(&self.buffer);
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
}
