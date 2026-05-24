use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenBuffer {
    buffer: Vec<u8>,
}

impl Default for ScreenBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenBuffer {
    pub const WIDTH: u32 = 160;
    pub const HEIGHT: u32 = 144;
    const BYTES_PER_PIXEL: usize = 3;

    pub fn new() -> Self {
        let size = (Self::WIDTH * Self::HEIGHT) as usize * Self::BYTES_PER_PIXEL;
        Self {
            buffer: vec![0; size],
        }
    }

    fn pixel_offset(x: u32, y: u32) -> usize {
        (y * Self::WIDTH + x) as usize * Self::BYTES_PER_PIXEL
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        let offset = Self::pixel_offset(x, y);
        self.buffer[offset] = r;
        self.buffer[offset + 1] = g;
        self.buffer[offset + 2] = b;
    }

    pub fn fill_rgb(&mut self, r: u8, g: u8, b: u8) {
        for pixel in self.buffer.chunks_exact_mut(Self::BYTES_PER_PIXEL) {
            pixel[0] = r;
            pixel[1] = g;
            pixel[2] = b;
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> (u8, u8, u8) {
        let offset = Self::pixel_offset(x, y);
        (
            self.buffer[offset],
            self.buffer[offset + 1],
            self.buffer[offset + 2],
        )
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    pub fn crc32(&self) -> u32 {
        crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(&self.buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_has_correct_size() {
        // Given: a new ScreenBuffer
        let buf = ScreenBuffer::new();
        // When: we take a snapshot
        let snap = buf.snapshot();
        // Then: size is 160×144×3 bytes
        assert_eq!(snap.len(), 160 * 144 * 3);
    }

    #[test]
    fn test_set_pixel_then_get_pixel_roundtrip() {
        // Given: a new ScreenBuffer
        let mut buf = ScreenBuffer::new();
        // When: we set pixel (10, 20) to (1, 2, 3)
        buf.set_pixel(10, 20, 1, 2, 3);
        // Then: get_pixel returns the same values
        assert_eq!(buf.get_pixel(10, 20), (1, 2, 3));
    }

    #[test]
    fn test_set_pixel_does_not_affect_adjacent_pixel() {
        // Given: a new ScreenBuffer
        let mut buf = ScreenBuffer::new();
        // When: we set pixel (0, 0) to (0xFF, 0xFF, 0xFF)
        buf.set_pixel(0, 0, 0xFF, 0xFF, 0xFF);
        // Then: pixel (1, 0) is still (0, 0, 0)
        assert_eq!(buf.get_pixel(1, 0), (0, 0, 0));
    }

    #[test]
    fn test_snapshot_reflects_written_pixel() {
        // Given: a buffer with one pixel set
        let mut buf = ScreenBuffer::new();
        buf.set_pixel(0, 0, 0xAA, 0xBB, 0xCC);
        // When: snapshot is taken
        let snap = buf.snapshot();
        // Then: first three bytes are the pixel RGB value
        assert_eq!(snap[0], 0xAA);
        assert_eq!(snap[1], 0xBB);
        assert_eq!(snap[2], 0xCC);
    }

    #[test]
    fn test_fill_rgb_sets_entire_buffer() {
        // Given: a buffer with different pixels
        let mut buf = ScreenBuffer::new();
        buf.set_pixel(0, 0, 1, 2, 3);
        buf.set_pixel(ScreenBuffer::WIDTH - 1, ScreenBuffer::HEIGHT - 1, 4, 5, 6);

        // When: the buffer is filled
        buf.fill_rgb(0xAA, 0xBB, 0xCC);

        // Then: every pixel has the fill colour
        assert!(
            buf.snapshot()
                .chunks_exact(ScreenBuffer::BYTES_PER_PIXEL)
                .all(|pixel| pixel == [0xAA, 0xBB, 0xCC])
        );
    }

    #[test]
    fn test_crc32_is_deterministic() {
        // Given: two identical ScreenBuffers with the same pixel written
        let mut buf1 = ScreenBuffer::new();
        let mut buf2 = ScreenBuffer::new();
        buf1.set_pixel(5, 5, 10, 20, 30);
        buf2.set_pixel(5, 5, 10, 20, 30);
        // When: crc32 is computed
        // Then: both CRCs are equal
        assert_eq!(buf1.crc32(), buf2.crc32());
    }

    #[test]
    fn test_crc32_differs_for_different_content() {
        // Given: two ScreenBuffers with different pixels
        let mut buf1 = ScreenBuffer::new();
        let mut buf2 = ScreenBuffer::new();
        buf1.set_pixel(0, 0, 1, 2, 3);
        buf2.set_pixel(0, 0, 4, 5, 6);
        // When: crc32 is computed
        // Then: CRCs differ
        assert_ne!(buf1.crc32(), buf2.crc32());
    }
}
