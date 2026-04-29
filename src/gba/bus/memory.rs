//! Backing stores for the GBA memory map regions.
//!
//! Each region is a flat byte buffer sized per GBATek's memory map, with
//! mirrored access (addresses are taken modulo the region size). Reads and
//! writes here are byte/halfword/word aware and little-endian, matching the
//! ARM7TDMI on the GBA.

/// 16 KB BIOS ROM at `0x00000000`.
pub const BIOS_SIZE: usize = 16 * 1024;
/// 256 KB on-board work RAM (EWRAM) at `0x02000000`.
pub const EWRAM_SIZE: usize = 256 * 1024;
/// 32 KB on-chip work RAM (IWRAM) at `0x03000000`.
pub const IWRAM_SIZE: usize = 32 * 1024;
/// 1 KB Palette RAM (PRAM) at `0x05000000`.
pub const PRAM_SIZE: usize = 1024;
/// 96 KB VRAM at `0x06000000`. Mirroring is non-uniform — see `MemoryRegions`.
pub const VRAM_SIZE: usize = 96 * 1024;
/// 1 KB OAM at `0x07000000`.
pub const OAM_SIZE: usize = 1024;
/// 64 KB cartridge SRAM at `0x0E000000`.
pub const SRAM_SIZE: usize = 64 * 1024;

/// Maximum cartridge ROM size addressable on the GBA (32 MB).
pub const ROM_MAX_SIZE: usize = 32 * 1024 * 1024;

/// Read a little-endian 16-bit halfword from a byte slice with mirrored
/// access.
#[inline]
pub fn read_le_u16(buf: &[u8], offset: usize) -> u16 {
    let len = buf.len();
    let i = offset % len;
    u16::from_le_bytes([buf[i], buf[(i + 1) % len]])
}

/// Read a little-endian 32-bit word from a byte slice with mirrored access.
#[inline]
pub fn read_le_u32(buf: &[u8], offset: usize) -> u32 {
    let len = buf.len();
    let i = offset % len;
    u32::from_le_bytes([
        buf[i],
        buf[(i + 1) % len],
        buf[(i + 2) % len],
        buf[(i + 3) % len],
    ])
}

/// Write a little-endian 16-bit halfword into a byte slice with mirrored
/// access.
#[inline]
pub fn write_le_u16(buf: &mut [u8], offset: usize, value: u16) {
    let len = buf.len();
    let i = offset % len;
    let bytes = value.to_le_bytes();
    buf[i] = bytes[0];
    buf[(i + 1) % len] = bytes[1];
}

/// Write a little-endian 32-bit word into a byte slice with mirrored access.
#[inline]
pub fn write_le_u32(buf: &mut [u8], offset: usize, value: u32) {
    let len = buf.len();
    let i = offset % len;
    let bytes = value.to_le_bytes();
    for (k, b) in bytes.iter().enumerate() {
        buf[(i + k) % len] = *b;
    }
}
