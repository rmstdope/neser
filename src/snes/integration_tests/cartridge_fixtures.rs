//! Minimal, in-code SNES cartridge image builder for the base-cartridge
//! verification fixtures (issue #2885). Emits byte-array ROM images with a
//! valid internal header at the mapping's offset (LoROM `0x7FC0` / HiROM
//! `0xFFC0` / ExHiROM `0x40FFC0`), authored from the header spec (fullsnes /
//! nesdev) rather than from the loader implementation.
//!
//! These images are *data* fixtures: the header, region metadata, SRAM
//! size/battery flags and address-translation sentinels are set directly so a
//! test can load the image and assert the decoded cartridge properties. For an
//! *executable* battery-SRAM fixture that a CPU program drives, see
//! [`super::fixture_rom::FixtureRom::with_battery_sram`].

use crate::snes::cartridge::Mapping;

/// Header title placed in every fixture (21 bytes, space-padded).
const TITLE: &[u8; 21] = b"NESER CART FIXTURE   ";

/// A builder for a single minimal cartridge image of a given `mapping`.
pub(crate) struct CartFixture {
    mapping: Mapping,
    chipset: u8,
    ram_size_field: u8,
    country: u8,
    copier_header: bool,
    /// `(offset_into_stripped_image, byte)` sentinels for address-translation
    /// checks. Offsets are relative to the ROM body, *after* any copier header.
    sentinels: Vec<(usize, u8)>,
}

impl CartFixture {
    pub(crate) fn new(mapping: Mapping) -> Self {
        Self {
            mapping,
            chipset: 0x00,
            ram_size_field: 0x00,
            country: 0x00,
            copier_header: false,
            sentinels: Vec::new(),
        }
    }

    pub(crate) fn chipset(mut self, chipset: u8) -> Self {
        self.chipset = chipset;
        self
    }

    pub(crate) fn ram_size_field(mut self, ram_size_field: u8) -> Self {
        self.ram_size_field = ram_size_field;
        self
    }

    pub(crate) fn country(mut self, country: u8) -> Self {
        self.country = country;
        self
    }

    /// Prepends a 512-byte copier header, making the emitted image
    /// `0x200`-longer so its length satisfies `len % 0x400 == 0x200`.
    pub(crate) fn with_copier_header(mut self) -> Self {
        self.copier_header = true;
        self
    }

    /// Places `byte` at `offset` in the ROM body (before any copier header is
    /// prepended), for reading back through the mapped CPU address.
    pub(crate) fn sentinel(mut self, offset: usize, byte: u8) -> Self {
        self.sentinels.push((offset, byte));
        self
    }

    /// The internal-header offset for this mapping.
    fn header_offset(&self) -> usize {
        match self.mapping {
            Mapping::LoRom => 0x7FC0,
            Mapping::HiRom => 0xFFC0,
            Mapping::ExHiRom => 0x40FFC0,
        }
    }

    fn map_mode(&self) -> u8 {
        match self.mapping {
            Mapping::LoRom => 0x20,
            Mapping::HiRom => 0x21,
            Mapping::ExHiRom => 0x25,
        }
    }

    fn image_size(&self) -> usize {
        match self.mapping {
            Mapping::LoRom => 0x10000,     // 64 KiB (two LoROM banks)
            Mapping::HiRom => 0x20000,     // 128 KiB (two HiROM banks)
            Mapping::ExHiRom => 0x41_0000, // header at 0x40FFC0 needs > 4 MiB
        }
    }

    fn rom_size_field(&self) -> u8 {
        match self.mapping {
            Mapping::LoRom | Mapping::HiRom => 0x07,
            Mapping::ExHiRom => 0x0C,
        }
    }

    /// Builds the ROM image bytes. Sets a spec-valid header (checksum +
    /// complement summing to `0xFFFF`, reset vector `$8000`) so `detect_mapping`
    /// scores the intended mapping, applies the sentinels, then optionally
    /// prepends the copier header.
    pub(crate) fn build(&self) -> Vec<u8> {
        let base = self.header_offset();
        let mut rom = vec![0u8; self.image_size()];

        for &(offset, byte) in &self.sentinels {
            debug_assert!(
                offset < base || offset >= base + 0x40,
                "sentinel at {offset:#X} overlaps the header window \
                 [{base:#X}, {:#X}) and would be clobbered",
                base + 0x40
            );
            rom[offset] = byte;
        }

        rom[base..base + TITLE.len()].copy_from_slice(TITLE);
        rom[base + 0x15] = self.map_mode();
        rom[base + 0x16] = self.chipset;
        rom[base + 0x17] = self.rom_size_field();
        rom[base + 0x18] = self.ram_size_field;
        rom[base + 0x19] = self.country;
        rom[base + 0x1C] = 0x34; // Complement + checksum sum to 0xFFFF.
        rom[base + 0x1D] = 0x12;
        rom[base + 0x1E] = 0xCB;
        rom[base + 0x1F] = 0xED;
        rom[base + 0x3C] = 0x00; // Reset vector -> $8000.
        rom[base + 0x3D] = 0x80;

        if self.copier_header {
            let mut with_copier = vec![0u8; 0x200];
            with_copier.extend_from_slice(&rom);
            with_copier
        } else {
            rom
        }
    }
}
