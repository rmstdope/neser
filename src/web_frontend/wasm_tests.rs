#![cfg(all(test, feature = "wasm", target_arch = "wasm32"))]

use crate::console::{ArkanoidState, ControllerStateWrapper, SaveState};
use crate::wasm::WasmNes;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

// A minimal valid iNES header for NROM-128 with zeroed ROM data.
fn minimal_nrom() -> Vec<u8> {
    let mut data = vec![0u8; 16 + 16384 + 8192];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1; // 1 * 16KB PRG
    data[5] = 1; // 1 * 8KB CHR
    data
}

fn read_save_state(nes: &WasmNes) -> SaveState {
    SaveState::from_bytes(&nes.save_state_bytes()).expect("save state should decode")
}

fn port1_arkanoid_state(state: &SaveState) -> ArkanoidState {
    match &state.bus.port1_controller {
        ControllerStateWrapper::Arkanoid(arkanoid) => arkanoid.clone(),
        ControllerStateWrapper::Joypad(_) => panic!("expected Arkanoid controller on port 1"),
    }
}

fn enable_arkanoid_on_port1(nes: &mut WasmNes) {
    nes.set_controller_type(1, "arkanoid")
        .expect("should set controller type");
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
    assert!(
        err.as_string()
            .unwrap_or_default()
            .to_lowercase()
            .contains("invalid"),
        "unexpected err: {:?}",
        err.as_string()
    );
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

    // Get audio samples - should return a valid vector
    // Note: May be empty or have data depending on timing
    let _samples = nes.get_audio_samples();

    // Just verify it doesn't panic - actual sample count depends on emulator timing
}

#[wasm_bindgen_test]
fn audio_samples_unmuted_returns_samples() {
    let mut nes = WasmNes::new();
    nes.push_audio_sample_for_test(0.5);
    nes.set_audio_muted(false);
    let samples = nes.get_audio_samples();
    assert_eq!(samples.len(), 1);
    assert!((samples[0] - 0.5).abs() < f32::EPSILON);
}

#[wasm_bindgen_test]
fn audio_samples_muted_drops_samples() {
    let mut nes = WasmNes::new();
    nes.push_audio_sample_for_test(0.5);
    nes.set_audio_muted(true);
    let samples = nes.get_audio_samples();
    assert!(samples.is_empty());

    nes.set_audio_muted(false);
    let samples_after_unmute = nes.get_audio_samples();
    assert!(samples_after_unmute.is_empty());
}

#[wasm_bindgen_test]
fn get_audio_samples_without_rom_succeeds() {
    let mut nes = WasmNes::new();

    // Should be able to call get_audio_samples even without a ROM
    let _samples = nes.get_audio_samples();

    // Should not panic
}

#[wasm_bindgen_test]
fn save_state_roundtrip_returns_same_bytes() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");

    let state1 = nes.save_state_bytes();
    assert!(!state1.is_empty());

    nes.load_state_bytes(&state1).expect("state should load");
    let state2 = nes.save_state_bytes();

    assert_eq!(state1, state2);
}

#[wasm_bindgen_test]
fn reset_restores_initial_state() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom).expect("valid rom should load");

    let initial = nes.save_state_bytes();
    assert!(!initial.is_empty());

    let _frame = nes.render_frame();
    let modified = nes.save_state_bytes();
    assert_ne!(initial, modified);

    nes.reset();
    let after_reset = nes.save_state_bytes();
    assert!(!after_reset.is_empty());
    assert_ne!(modified, after_reset);

    let _state = SaveState::from_bytes(&after_reset).expect("save state should decode");
}

#[wasm_bindgen_test]
fn set_mouse_x_position_updates_save_state() {
    let mut nes = WasmNes::new();
    enable_arkanoid_on_port1(&mut nes);
    nes.set_mouse_x_position(0x80);

    let state = read_save_state(&nes);
    let arkanoid = port1_arkanoid_state(&state);
    assert_eq!(arkanoid.position, 0x80);
}

#[wasm_bindgen_test]
fn set_mouse_x_position_clamps_to_valid_range() {
    let mut nes = WasmNes::new();
    enable_arkanoid_on_port1(&mut nes);
    nes.set_mouse_x_position(0x20);

    let state = read_save_state(&nes);
    let arkanoid = port1_arkanoid_state(&state);
    assert_eq!(arkanoid.position, 0x62);

    nes.set_mouse_x_position(0xFF);
    let state = read_save_state(&nes);
    let arkanoid = port1_arkanoid_state(&state);
    assert_eq!(arkanoid.position, 0xF2);
}

#[wasm_bindgen_test]
fn set_mouse_left_button_updates_save_state() {
    let mut nes = WasmNes::new();
    enable_arkanoid_on_port1(&mut nes);
    nes.set_mouse_left_button(true);

    let state = read_save_state(&nes);
    let arkanoid = port1_arkanoid_state(&state);
    assert!(arkanoid.trigger);

    nes.set_mouse_left_button(false);
    let state = read_save_state(&nes);
    let arkanoid = port1_arkanoid_state(&state);
    assert!(!arkanoid.trigger);
}
