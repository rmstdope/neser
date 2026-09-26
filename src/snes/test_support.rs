//! Shared construction helpers for SNES tests.
//!
//! Every SNES test that builds a console goes through here so the power-on RAM
//! pattern is pinned in exactly one place.

use crate::platform::app_context::AppContext;
use crate::platform::config::{Config, RamInitMode};

/// A [`Config`] with the power-on RAM pattern pinned to
/// [`RamInitMode::Zero`], for tests that need a deterministic console.
///
/// `Config::default()` resolves `ram_init_mode` to [`RamInitMode::Random`] on
/// native targets (it is [`RamInitMode::Zero`] only on wasm), and since #3128
/// the SNES core honours that setting for WRAM, VRAM, CGRAM, OAM, ARAM and SA-1
/// I-RAM. A test built straight from `Config::default()` would therefore see a
/// different machine on every run, and every committed screen CRC would be
/// measuring the RNG. Tests that deliberately exercise a non-zero mode set the
/// field themselves after calling this.
pub(crate) fn snes_test_config() -> Config {
    let mut config = Config::default();
    config.frontend.ram_init_mode = RamInitMode::Zero;
    config
}

/// [`snes_test_config`] wrapped in an [`AppContext`], for the common case of
/// `Snes::new(...)` with no other configuration.
pub(crate) fn snes_test_app_context() -> AppContext {
    AppContext::new_with_config(snes_test_config())
}

/// A 64 KiB ROM image with a valid internal header: `title` (padded with spaces), the given
/// map mode and chipset, and a `NOP` at the reset vector's target `$00:8000`.
fn header_rom(title: &[u8], hirom: bool, chipset: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x10000];
    let header = if hirom { 0xFFC0 } else { 0x7FC0 };
    let mut padded = [b' '; 21];
    padded[..title.len()].copy_from_slice(title);
    rom[header..header + 21].copy_from_slice(&padded);
    rom[header + 0x3C] = 0x00;
    rom[header + 0x3D] = 0x80;
    rom[header + 0x15] = if hirom { 0x21 } else { 0x20 };
    rom[header + 0x16] = chipset;
    rom[header + 0x17] = 0x06; // 64 KiB
    rom[header + 0x1C] = 0x34;
    rom[header + 0x1D] = 0x12;
    rom[header + 0x1E] = 0xCB;
    rom[header + 0x1F] = 0xED;
    rom[if hirom { 0x8000 } else { 0x0000 }] = 0xEA; // NOP at $00:8000
    rom
}

/// A DSP cartridge (chipset `$03`, "ROM+DSP") with the given header title, LoROM or HiROM.
pub(crate) fn dsp_rom(title: &[u8], hirom: bool) -> Vec<u8> {
    header_rom(title, hirom, 0x03)
}

/// A plain LoROM cartridge with no coprocessor.
pub(crate) fn minimal_lorom(title: &[u8]) -> Vec<u8> {
    header_rom(title, false, 0x00)
}
