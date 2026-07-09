//! SA-1 I-RAM (2KB on-chip RAM) and its per-256-byte-chunk write protection.
//!
//! Scope (issue #2958): the 2KB I-RAM buffer itself, addressable from the SA-1 side directly at
//! `$000000-$0007FF` *and* mirrored at `$003000-$0037FF` (fullsnes "Memory Map (SA-1 Side)"), and
//! from the SNES side only via the `$003000-$0037FF` mirror (banks `$00-$3F`/`$80-$BF`). Register
//! bit layouts and reset values are sourced from fullsnes ("SNES Cart SA-1 I/O Map" / "Memory
//! Control" sections), per the `snes-hardware-research` skill's source priority.

/// Size of SA-1's on-chip I-RAM in bytes (fullsnes: "2Kbytes internal I-RAM").
pub const IRAM_SIZE: usize = 0x800;

/// The 2KB I-RAM buffer plus its two independent write-protection registers.
///
/// fullsnes ("SNES Cart SA-1 Memory Control", `$2229`/`$222A`): "Write enable flags for eight
/// 256-byte chunks (0=Protect, 1=Write Enable). Bit0 for I-RAM `3000h..30FFh`, bit1 for
/// `3100h..31FFh`, etc, bit7 for `3700h..37FFh`." Both registers reset to `$00`, so I-RAM is
/// fully write-protected from both CPU sides at power-on until software enables it -- reads are
/// never protected, from either side.
pub struct Sa1IRam {
    data: [u8; IRAM_SIZE],
    /// `$2229` SIWP: gates writes arriving from the SNES side.
    snes_write_protect: u8,
    /// `$222A` CIWP: gates writes arriving from the SA-1 side.
    sa1_write_protect: u8,
}

impl Sa1IRam {
    pub fn new() -> Self {
        Self {
            data: [0; IRAM_SIZE],
            snes_write_protect: 0x00,
            sa1_write_protect: 0x00,
        }
    }

    fn chunk_bit(offset: usize) -> u8 {
        1 << ((offset & (IRAM_SIZE - 1)) >> 8)
    }

    /// Reads are never protected, from either CPU side.
    pub fn read(&self, offset: usize) -> u8 {
        self.data[offset & (IRAM_SIZE - 1)]
    }

    /// Writes a byte from the SNES side, honoring `$2229` SIWP.
    pub fn write_from_snes(&mut self, offset: usize, value: u8) {
        if self.snes_write_protect & Self::chunk_bit(offset) != 0 {
            self.data[offset & (IRAM_SIZE - 1)] = value;
        }
    }

    /// Writes a byte from the SA-1 side, honoring `$222A` CIWP.
    pub fn write_from_sa1(&mut self, offset: usize, value: u8) {
        if self.sa1_write_protect & Self::chunk_bit(offset) != 0 {
            self.data[offset & (IRAM_SIZE - 1)] = value;
        }
    }

    /// `$2229` SIWP write.
    pub fn set_snes_write_protect(&mut self, value: u8) {
        self.snes_write_protect = value;
    }

    /// `$222A` CIWP write.
    pub fn set_sa1_write_protect(&mut self, value: u8) {
        self.sa1_write_protect = value;
    }

    /// Raw I-RAM bytes, for save-state capture.
    pub(crate) fn data(&self) -> &[u8; IRAM_SIZE] {
        &self.data
    }

    pub(crate) fn snes_write_protect_raw(&self) -> u8 {
        self.snes_write_protect
    }

    pub(crate) fn sa1_write_protect_raw(&self) -> u8 {
        self.sa1_write_protect
    }

    /// Restores I-RAM bytes and both protection registers exactly, for save-state loading.
    /// `data` shorter than [`IRAM_SIZE`] leaves the remaining tail zeroed; longer is truncated.
    pub(crate) fn restore_raw(
        &mut self,
        data: &[u8],
        snes_write_protect: u8,
        sa1_write_protect: u8,
    ) {
        self.data = [0; IRAM_SIZE];
        let len = data.len().min(IRAM_SIZE);
        self.data[..len].copy_from_slice(&data[..len]);
        self.snes_write_protect = snes_write_protect;
        self.sa1_write_protect = sa1_write_protect;
    }
}

impl Default for Sa1IRam {
    fn default() -> Self {
        Self::new()
    }
}

/// Decodes an address into an I-RAM byte offset (`0..IRAM_SIZE`) if it falls within the
/// `$003000-$0037FF` mirror window (banks `$00-$3F`/`$80-$BF`), used by both the SNES-side bus
/// and the SA-1-side bus (SA-1 also sees this same mirror per fullsnes).
pub fn decode_mirror_offset(addr: u32) -> Option<usize> {
    let addr = addr & 0xFF_FFFF;
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;
    if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && (0x3000..=0x37FF).contains(&offset) {
        Some((offset - 0x3000) as usize)
    } else {
        None
    }
}

/// Decodes an address into an I-RAM byte offset if it falls within the SA-1-side-only direct
/// window `$000000-$0007FF` (banks `$00-$3F`/`$80-$BF`; fullsnes: "I-RAM (at both `0000h-07FFh`
/// and `3000h-37FFh`)").
pub fn decode_direct_offset(addr: u32) -> Option<usize> {
    let addr = addr & 0xFF_FFFF;
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;
    if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset <= 0x07FF {
        Some(offset as usize)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_never_protected() {
        let mut iram = Sa1IRam::new();
        iram.data[0] = 0xAB;
        assert_eq!(iram.read(0), 0xAB);
    }

    #[test]
    fn snes_write_is_blocked_by_default() {
        let mut iram = Sa1IRam::new();
        iram.write_from_snes(0, 0x11);
        assert_eq!(iram.read(0), 0x00);
    }

    #[test]
    fn sa1_write_is_blocked_by_default() {
        let mut iram = Sa1IRam::new();
        iram.write_from_sa1(0, 0x11);
        assert_eq!(iram.read(0), 0x00);
    }

    #[test]
    fn snes_write_succeeds_once_its_chunk_is_enabled() {
        let mut iram = Sa1IRam::new();
        iram.set_snes_write_protect(0b0000_0001); // chunk 0 ($3000-$30FF) writable
        iram.write_from_snes(0x0000, 0x22);
        assert_eq!(iram.read(0x0000), 0x22);
    }

    #[test]
    fn snes_write_is_blocked_outside_its_enabled_chunk() {
        let mut iram = Sa1IRam::new();
        iram.set_snes_write_protect(0b0000_0001); // only chunk 0 writable
        iram.write_from_snes(0x0100, 0x22); // chunk 1
        assert_eq!(iram.read(0x0100), 0x00);
    }

    #[test]
    fn sa1_write_succeeds_once_its_chunk_is_enabled() {
        let mut iram = Sa1IRam::new();
        iram.set_sa1_write_protect(0b1000_0000); // chunk 7 ($3700-$37FF) writable
        iram.write_from_sa1(0x07FF, 0x33);
        assert_eq!(iram.read(0x07FF), 0x33);
    }

    #[test]
    fn snes_and_sa1_write_protection_are_independent() {
        let mut iram = Sa1IRam::new();
        iram.set_snes_write_protect(0xFF); // all chunks writable from SNES
        // CIWP stays at its $00 reset value: SA-1-side writes remain blocked.
        iram.write_from_sa1(0x0000, 0x44);
        assert_eq!(iram.read(0x0000), 0x00);
        iram.write_from_snes(0x0000, 0x55);
        assert_eq!(iram.read(0x0000), 0x55);
    }

    #[test]
    fn decode_mirror_offset_covers_the_3000_37ff_window_in_system_banks() {
        assert_eq!(decode_mirror_offset(0x00_3000), Some(0x000));
        assert_eq!(decode_mirror_offset(0x00_37FF), Some(0x7FF));
        assert_eq!(decode_mirror_offset(0x80_3100), Some(0x100));
        assert_eq!(decode_mirror_offset(0x00_2FFF), None);
        assert_eq!(decode_mirror_offset(0x00_3800), None);
        assert_eq!(decode_mirror_offset(0x40_3000), None); // bank $40 is outside 00-3F/80-BF
    }

    #[test]
    fn decode_direct_offset_covers_the_0000_07ff_window_in_system_banks() {
        assert_eq!(decode_direct_offset(0x00_0000), Some(0x000));
        assert_eq!(decode_direct_offset(0x00_07FF), Some(0x7FF));
        assert_eq!(decode_direct_offset(0x80_0100), Some(0x100));
        assert_eq!(decode_direct_offset(0x00_0800), None);
        assert_eq!(decode_direct_offset(0x40_0000), None);
    }

    #[test]
    fn restore_raw_reinstates_bytes_and_both_protection_registers() {
        let mut iram = Sa1IRam::new();
        iram.restore_raw(&[0xAA, 0xBB, 0xCC], 0x01, 0x80);
        assert_eq!(iram.read(0), 0xAA);
        assert_eq!(iram.read(1), 0xBB);
        assert_eq!(iram.read(2), 0xCC);
        assert_eq!(iram.snes_write_protect_raw(), 0x01);
        assert_eq!(iram.sa1_write_protect_raw(), 0x80);
    }

    #[test]
    fn restore_raw_zero_fills_the_tail_when_given_fewer_bytes_than_iram_size() {
        let mut iram = Sa1IRam::new();
        iram.set_snes_write_protect(0xFF);
        iram.write_from_snes(IRAM_SIZE - 1, 0x77);
        iram.restore_raw(&[0x01], 0x00, 0x00);
        assert_eq!(iram.read(IRAM_SIZE - 1), 0x00);
    }
}
