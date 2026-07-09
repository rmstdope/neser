//! SA-1 Super MMC ROM banking (`$2220-$2223`) and BW-RAM mapping/write-protection
//! (`$2224-$2228`).
//!
//! Scope (issue #2959): configurable ROM banking for both CPU sides, the mappable 8KB BW-RAM
//! window (banks `$00-$3F`/`$80-$BF`:`$6000-$7FFF`) with independent per-side block selection,
//! direct linear BW-RAM access (banks `$40-$4F`), and the shared write-protection rule. Register
//! bit layouts, reset values, and the ROM-banking semantics are sourced from fullsnes ("SNES
//! Cart SA-1 Memory Control" section); fullsnes's wording on the LoROM-range default/bit-7
//! behavior is ambiguous on its own, so it's cross-checked against bsnes
//! (`bsnes/sfc/coprocessor/sa1/rom.cpp` `readCPU`, `bwram.cpp` `writeCPU`/`writeLinear`), per the
//! `snes-hardware-research` skill's escalation path.
//!
//! Deliberately out of scope: BW-RAM "bitmap" mode (`$223F` BBF, banks `$60-$6F`, 2bpp/4bpp pixel
//! packing) -- not exercised by either absindx conformance ROM's RAM-protection focus. `$2225`
//! BMAP's bit 7 (mode select) is stored but otherwise ignored; SA-1-side BW-RAM access always
//! uses the linear (non-bitmap) 8KB-window interpretation of BMAP's low 7 bits.

/// `$2220-$2228`: Super MMC ROM banking and BW-RAM mapping/write-protection registers.
pub struct Sa1MemoryControl {
    /// `$2220` CXB (SNES-writable): banks `$C0-$CF`/`$00-$1F` ROM banking.
    cxb: u8,
    /// `$2221` DXB (SNES-writable): banks `$D0-$DF`/`$20-$3F` ROM banking.
    dxb: u8,
    /// `$2222` EXB (SNES-writable): banks `$E0-$EF`/`$80-$9F` ROM banking.
    exb: u8,
    /// `$2223` FXB (SNES-writable): banks `$F0-$FF`/`$A0-$BF` ROM banking.
    fxb: u8,
    /// `$2224` BMAPS (SNES-writable): SNES-side BW-RAM block select for the `$6000-$7FFF` window.
    bmaps: u8,
    /// `$2225` BMAP (SA-1-writable): SA-1-side BW-RAM block select for the `$6000-$7FFF` window
    /// (bits 0-6) plus the bitmap-mode select (bit 7, stored but not acted on -- see module doc).
    bmap: u8,
    /// `$2226` SBWE (SNES-writable): SNES-side BW-RAM write enable (bit 7).
    sbwe: u8,
    /// `$2227` CBWE (SA-1-writable): SA-1-side BW-RAM write enable (bit 7).
    cbwe: u8,
    /// `$2228` BWPA (SNES-writable): BW-RAM write-protected area size code (bits 0-3).
    bwpa: u8,
}

impl Sa1MemoryControl {
    /// Hardware reset values (fullsnes "Reset" table): `$2220-$2223`=`$00,$01,$02,$03` (ROM
    /// slots 0-3 in order, bit 7 clear); `$2228`=`$FF` (protected-area code `$F`, i.e. 8MB --
    /// larger than any real BW-RAM, so effectively "protect everything"); everything else `$00`
    /// (BW-RAM write-disabled from both sides, combined with the `$FF` BWPA this fully
    /// protects BW-RAM at power-on, mirroring I-RAM's own protect-by-default convention).
    pub fn new() -> Self {
        Self {
            cxb: 0x00,
            dxb: 0x01,
            exb: 0x02,
            fxb: 0x03,
            bmaps: 0x00,
            bmap: 0x00,
            sbwe: 0x00,
            cbwe: 0x00,
            bwpa: 0xFF,
        }
    }

    /// Dispatches a write to the raw `$2220-$2228` MMIO offset.
    pub fn write(&mut self, port: u16, value: u8) {
        match port {
            0x2220 => self.cxb = value,
            0x2221 => self.dxb = value,
            0x2222 => self.exb = value,
            0x2223 => self.fxb = value,
            0x2224 => self.bmaps = value,
            0x2225 => self.bmap = value,
            0x2226 => self.sbwe = value,
            0x2227 => self.cbwe = value,
            0x2228 => self.bwpa = value,
            _ => {}
        }
    }

    pub(crate) fn cxb(&self) -> u8 {
        self.cxb
    }
    pub(crate) fn dxb(&self) -> u8 {
        self.dxb
    }
    pub(crate) fn exb(&self) -> u8 {
        self.exb
    }
    pub(crate) fn fxb(&self) -> u8 {
        self.fxb
    }
    pub(crate) fn bmaps(&self) -> u8 {
        self.bmaps
    }
    pub(crate) fn bmap(&self) -> u8 {
        self.bmap
    }
    pub(crate) fn sbwe(&self) -> u8 {
        self.sbwe
    }
    pub(crate) fn cbwe(&self) -> u8 {
        self.cbwe
    }
    pub(crate) fn bwpa(&self) -> u8 {
        self.bwpa
    }

    /// Restores every register to an exact byte value, for save-state loading.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn restore_raw(
        &mut self,
        cxb: u8,
        dxb: u8,
        exb: u8,
        fxb: u8,
        bmaps: u8,
        bmap: u8,
        sbwe: u8,
        cbwe: u8,
        bwpa: u8,
    ) {
        self.cxb = cxb;
        self.dxb = dxb;
        self.exb = exb;
        self.fxb = fxb;
        self.bmaps = bmaps;
        self.bmap = bmap;
        self.sbwe = sbwe;
        self.cbwe = cbwe;
        self.bwpa = bwpa;
    }

    /// `$2224` BMAPS bits 0-4: SNES-side 8KB BW-RAM block select (0-31).
    pub fn snes_bwram_block(&self) -> usize {
        (self.bmaps & 0x1F) as usize
    }

    /// `$2225` BMAP bits 0-6: SA-1-side 8KB BW-RAM block select (0-127, linear mode).
    pub fn sa1_bwram_block(&self) -> usize {
        (self.bmap & 0x7F) as usize
    }

    /// `$2228` BWPA: size in bytes of the write-protected region, based at BW-RAM offset 0
    /// (fullsnes: "Select size of Write-Protected Area (`256 SHL N` bytes)").
    pub fn protected_bytes(&self) -> usize {
        256usize << (self.bwpa & 0x0F)
    }

    /// Whether a write to the given *linear* BW-RAM offset is blocked.
    ///
    /// bsnes (`SA1::BWRAM::writeCPU`/`writeLinear`), confirming the absindx
    /// `SA1RamProtectionTest` README's own note ("BW-RAM Protection will not be reflected unless
    /// protection is enabled on both SNES and SA-1"): protection engages **only** when *both*
    /// `$2226` SBWE and `$2227` CBWE have their write-enable bit clear; if either side has
    /// enabled writes, `$2228` BWPA has no effect and the write always succeeds. This is a
    /// single, shared rule -- not independent per-side protection like I-RAM's SIWP/CIWP.
    pub fn is_bwram_write_protected(&self, linear_offset: usize) -> bool {
        let snes_write_enabled = self.sbwe & 0x80 != 0;
        let sa1_write_enabled = self.cbwe & 0x80 != 0;
        !snes_write_enabled && !sa1_write_enabled && linear_offset < self.protected_bytes()
    }
}

impl Default for Sa1MemoryControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Decodes a cartridge ROM address into a byte offset per SA-1's Super MMC banking.
///
/// fullsnes: "The registers do affect both SNES and SA-1 mapping" -- this single decode is
/// shared by both CPU sides. HiROM banks (`$C0-$FF`) always honor their quarter's bank-select
/// field; LoROM banks (`$00-$3F`/`$80-$BF`:`$8000-$FFFF`) only do so if that quarter's bit 7 is
/// set, otherwise they show a fixed, un-remapped 1MB slot (0/1/2/3 respectively) -- confirmed
/// against bsnes's `SA1::ROM::readCPU` since fullsnes's own prose here is ambiguous in
/// isolation. Each 1MB slot is `256 SHL 12` bytes; up to 8 slots (3-bit field) address up to 8MB.
pub fn decode_rom_index(addr: u32, control: &Sa1MemoryControl) -> Option<usize> {
    let addr = addr & 0xFF_FFFF;
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;

    if let Some((register, bank_in_quarter)) = match bank {
        0xC0..=0xCF => Some((control.cxb(), bank & 0x0F)),
        0xD0..=0xDF => Some((control.dxb(), bank & 0x0F)),
        0xE0..=0xEF => Some((control.exb(), bank & 0x0F)),
        0xF0..=0xFF => Some((control.fxb(), bank & 0x0F)),
        _ => None,
    } {
        let slot = (register & 0x07) as usize;
        return Some(slot * 0x10_0000 + bank_in_quarter as usize * 0x10000 + offset as usize);
    }

    if offset < 0x8000 {
        return None;
    }
    let (register, fixed_slot, bank_in_quarter) = match bank {
        0x00..=0x1F => (control.cxb(), 0u8, bank & 0x1F),
        0x20..=0x3F => (control.dxb(), 1u8, bank & 0x1F),
        0x80..=0x9F => (control.exb(), 2u8, bank & 0x1F),
        0xA0..=0xBF => (control.fxb(), 3u8, bank & 0x1F),
        _ => return None,
    };
    let remaps_lorom = register & 0x80 != 0;
    let slot = if remaps_lorom {
        (register & 0x07) as usize
    } else {
        fixed_slot as usize
    };
    Some(slot * 0x10_0000 + bank_in_quarter as usize * 0x8000 + (offset as usize - 0x8000))
}

/// Decodes an address into a BW-RAM window offset (`0..0x2000`) if it falls within the mappable
/// `$6000-$7FFF` window (banks `$00-$3F`/`$80-$BF`). The 8KB block selected within BW-RAM is a
/// per-side concern (`$2224` BMAPS or `$2225` BMAP) applied by the caller.
pub fn decode_windowed_offset(addr: u32) -> Option<usize> {
    let addr = addr & 0xFF_FFFF;
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;
    if matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && (0x6000..=0x7FFF).contains(&offset) {
        Some((offset - 0x6000) as usize)
    } else {
        None
    }
}

/// Decodes an address into a linear BW-RAM offset if it falls within the direct-access banks
/// `$40-$4F` (fullsnes: "Entire 256Kbyte BW-RAM (mirrors in 44h-4Fh)" -- the mirroring itself is
/// handled by the caller modulo-ing against the actual BW-RAM size).
pub fn decode_direct_offset(addr: u32) -> Option<usize> {
    let addr = addr & 0xFF_FFFF;
    let bank = ((addr >> 16) & 0xFF) as u8;
    let offset = (addr & 0xFFFF) as u16;
    if (0x40..=0x4F).contains(&bank) {
        Some((bank - 0x40) as usize * 0x10000 + offset as usize)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_resets_to_hardware_defaults() {
        let control = Sa1MemoryControl::new();
        assert_eq!(control.cxb(), 0x00);
        assert_eq!(control.dxb(), 0x01);
        assert_eq!(control.exb(), 0x02);
        assert_eq!(control.fxb(), 0x03);
        assert_eq!(control.bwpa(), 0xFF);
        assert_eq!(control.protected_bytes(), 256 << 0xF);
    }

    #[test]
    fn hirom_banks_always_honor_the_bank_select_field_regardless_of_bit7() {
        let mut control = Sa1MemoryControl::new();
        control.write(0x2220, 0x05); // CXB: slot 5, bit7 clear
        assert_eq!(decode_rom_index(0x00C0_0000, &control), Some(5 * 0x10_0000));
        assert_eq!(
            decode_rom_index(0x00CF_FFFF, &control),
            Some(5 * 0x10_0000 + 0x0F_FFFF)
        );
    }

    #[test]
    fn lorom_banks_show_a_fixed_slot_when_bit7_is_clear() {
        let control = Sa1MemoryControl::new(); // cxb=$00, bit7 clear
        // Bank $00 offset $8000 must show fixed slot 0, ignoring cxb's bank-select bits.
        assert_eq!(decode_rom_index(0x00_8000, &control), Some(0));
        // Bank $20 (DXB's quarter) offset $8000 must show fixed slot 1.
        assert_eq!(decode_rom_index(0x20_8000, &control), Some(0x10_0000));
    }

    #[test]
    fn lorom_banks_honor_the_bank_select_field_once_bit7_is_set() {
        let mut control = Sa1MemoryControl::new();
        control.write(0x2220, 0x87); // CXB: slot 7, bit7 set (remap LoROM too)
        assert_eq!(decode_rom_index(0x00_8000, &control), Some(7 * 0x10_0000));
    }

    #[test]
    fn lorom_offsets_below_8000_are_unmapped() {
        let control = Sa1MemoryControl::new();
        assert_eq!(decode_rom_index(0x00_0000, &control), None);
    }

    #[test]
    fn unrelated_banks_are_unmapped() {
        let control = Sa1MemoryControl::new();
        assert_eq!(decode_rom_index(0x40_8000, &control), None);
    }

    #[test]
    fn decode_windowed_offset_covers_6000_7fff_in_system_banks() {
        assert_eq!(decode_windowed_offset(0x00_6000), Some(0x0000));
        assert_eq!(decode_windowed_offset(0x80_7FFF), Some(0x1FFF));
        assert_eq!(decode_windowed_offset(0x00_5FFF), None);
        assert_eq!(decode_windowed_offset(0x40_6000), None);
    }

    #[test]
    fn decode_direct_offset_covers_40_4f_banks() {
        assert_eq!(decode_direct_offset(0x40_0000), Some(0));
        assert_eq!(decode_direct_offset(0x4F_FFFF), Some(0x0F_FFFF));
        assert_eq!(decode_direct_offset(0x3F_0000), None);
        assert_eq!(decode_direct_offset(0x50_0000), None);
    }

    #[test]
    fn write_protection_engages_only_when_both_sides_have_writes_disabled() {
        let mut control = Sa1MemoryControl::new();
        control.write(0x2228, 0x00); // protect only the first 256 bytes
        assert!(control.is_bwram_write_protected(0));
        assert!(!control.is_bwram_write_protected(256));

        control.write(0x2226, 0x80); // SNES side enables writes
        assert!(
            !control.is_bwram_write_protected(0),
            "either side enabling writes lifts BWPA"
        );
    }

    #[test]
    fn bwram_block_selects_are_independent_per_side() {
        let mut control = Sa1MemoryControl::new();
        control.write(0x2224, 0x05); // BMAPS
        control.write(0x2225, 0x7F); // BMAP
        assert_eq!(control.snes_bwram_block(), 5);
        assert_eq!(control.sa1_bwram_block(), 0x7F);
    }
}
