#![cfg(all(test, feature = "wasm", target_arch = "wasm32"))]

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use crate::wasm::WasmNes;

wasm_bindgen_test_configure!(run_in_browser);

// A minimal valid iNES header for NROM-128 with zeroed ROM data.
fn minimal_nrom() -> Vec<u8> {
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1; // 1 * 16KB PRG
    data[5] = 1; // 1 * 8KB CHR
    data
}

#[wasm_bindgen_test]
fn wasm_nes_constructs() {
    let _nes = WasmNes::new();
}

#[wasm_bindgen_test]
fn load_rom_accepts_valid_data() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");
}

#[wasm_bindgen_test]
fn load_rom_rejects_invalid_header() {
    let mut nes = WasmNes::new();
    let mut rom = minimal_nrom();
    rom[0] = 0; // break magic
    let err = nes.load_rom(&rom).expect_err("invalid rom should error");
    assert!(err.as_string().unwrap_or_default().to_lowercase().contains("invalid"), "unexpected err: {:?}", err.as_string());
}

#[wasm_bindgen_test]
fn render_frame_returns_expected_size() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");
    let frame = nes.render_frame();
    assert_eq!(frame.len(), 256 * 240 * 3);
}

#[wasm_bindgen_test]
fn render_frame_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    let frame = nes.render_frame();
    assert_eq!(frame.len(), 256 * 240 * 3);
}
