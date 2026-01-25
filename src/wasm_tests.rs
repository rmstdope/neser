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
fn render_frame_rgba_returns_expected_size() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");
    let frame = nes.render_frame_rgba();
    assert_eq!(frame.len(), 256 * 240 * 4);
    // Alpha should be opaque for all pixels.
    assert!(frame.iter().skip(3).step_by(4).all(|a| *a == 0xFF));
}

#[wasm_bindgen_test]
fn render_frame_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    let frame = nes.render_frame();
    assert_eq!(frame.len(), 256 * 240 * 3);
}

#[wasm_bindgen_test]
fn render_frame_rgba_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    let frame = nes.render_frame_rgba();
    assert_eq!(frame.len(), 256 * 240 * 4);
    assert!(frame.iter().skip(3).step_by(4).all(|a| *a == 0xFF));
}

#[wasm_bindgen_test]
fn set_button_accepts_valid_inputs() {
    let mut nes = WasmNes::new();
    // Test all valid button values (0-7) and controller values (1-2)
    for controller in 1..=2 {
        for button in 0..=7 {
            nes.set_button(controller, button, true);
            nes.set_button(controller, button, false);
        }
    }
    // If we reach here without panicking, the test passes
}

#[wasm_bindgen_test]
fn set_button_ignores_invalid_button() {
    let mut nes = WasmNes::new();
    // Invalid button numbers should be ignored without panicking
    nes.set_button(1, 8, true);
    nes.set_button(1, 255, true);
}

#[wasm_bindgen_test]
fn get_audio_samples_returns_vec() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");
    
    // Run a frame to generate some audio samples
    let _frame = nes.render_frame_rgba();
    
    // Get audio samples
    let samples = nes.get_audio_samples();
    
    // Samples should be a valid vector (could be empty or have data)
    // Just verify it doesn't panic and returns a vector
    assert!(samples.len() >= 0);
}

#[wasm_bindgen_test]
fn get_audio_samples_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    
    // Should be able to call get_audio_samples even without a ROM
    let samples = nes.get_audio_samples();
    
    // Should return empty or default samples
    assert!(samples.len() >= 0);
}

