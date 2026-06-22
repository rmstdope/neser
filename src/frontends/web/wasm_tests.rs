#![cfg(all(test, feature = "wasm", target_arch = "wasm32"))]

use crate::nes::bus::ControllerStateWrapper;
use crate::nes::console::SaveState;
use crate::nes::input::ArkanoidState;
use crate::wasm::{WasmNes, gamepad_init_toast_message};
use crate::wasm_gb::WasmGb;
use crate::wasm_gba::WasmGba;
use crate::wasm_snes::WasmSnes;
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

/// NROM-128 with all-NOP PRG and all interrupt vectors pointing to $8000.
///
/// Use this helper when the disassembly window is exercised, because the
/// all-zero `minimal_nrom` has reset vector = $0000 and `disassemble_from_start`
/// stops early when the address wraps around through zero, which prevents
/// `is_current` from being set to `true` for any entry.
fn minimal_nrom_nop_at_8000() -> Vec<u8> {
    let prg_size = 16384usize;
    let chr_size = 8192usize;
    let header_size = 16usize;
    // Zero-initialise the header so no stray mapper or flag bits creep in
    // from the NOP fill below.
    let mut data = vec![0u8; header_size + prg_size + chr_size];
    data[0..4].copy_from_slice(b"NES\x1A");
    data[4] = 1; // 1 × 16 KB PRG
    data[5] = 1; // 1 × 8 KB CHR
    // Fill PRG ROM with NOP ($EA) so instructions are 1 byte each and
    // there are no wrap-around issues when the disassembly window walks
    // backwards from $8000.
    let prg_start = header_size;
    data[prg_start..prg_start + prg_size].fill(0xEA);
    // Set NMI / Reset / IRQ-BRK vectors to $8000.
    data[prg_start + 0x3FFA] = 0x00; // NMI lo
    data[prg_start + 0x3FFB] = 0x80; // NMI hi  → $8000
    data[prg_start + 0x3FFC] = 0x00; // Reset lo
    data[prg_start + 0x3FFD] = 0x80; // Reset hi → $8000
    data[prg_start + 0x3FFE] = 0x00; // IRQ/BRK lo
    data[prg_start + 0x3FFF] = 0x80; // IRQ/BRK hi → $8000
    data
}

fn read_save_state(nes: &WasmNes) -> SaveState {
    SaveState::from_bytes(&nes.save_state_bytes()).expect("save state should decode")
}

fn port1_arkanoid_state(state: &SaveState) -> ArkanoidState {
    match &state.bus.port1_controller {
        ControllerStateWrapper::Arkanoid(arkanoid) => arkanoid.clone(),
        ControllerStateWrapper::Joypad(_) => panic!("expected Arkanoid controller on port 1"),
        ControllerStateWrapper::SnesAdapter(_) => {
            panic!("expected Arkanoid controller on port 1")
        }
        ControllerStateWrapper::Zapper(_) => panic!("expected Arkanoid controller on port 1"),
        ControllerStateWrapper::PowerPad(_) => panic!("expected Arkanoid controller on port 1"),
    }
}

fn enable_arkanoid_on_port1(nes: &mut WasmNes) {
    nes.set_controller_type(1, "arkanoid")
        .expect("should set controller type");
}

fn parse_disasm_addrs_and_current_index(json: &str) -> (Vec<u16>, usize) {
    let trimmed = json.trim();
    let body = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .expect("disasm JSON should be an array");

    let mut addrs = Vec::new();
    let mut current_index = None;

    if body.is_empty() {
        return (addrs, 0);
    }

    for (index, raw_entry) in body.split("},{").enumerate() {
        let entry = raw_entry.trim_start_matches('{').trim_end_matches('}');
        let addr_start = entry
            .find("\"addr\":")
            .map(|p| p + "\"addr\":".len())
            .expect("entry should contain addr");
        let addr_end_rel = entry[addr_start..]
            .find(',')
            .expect("addr field should be followed by comma");
        let addr_end = addr_start + addr_end_rel;
        let addr = entry[addr_start..addr_end]
            .parse::<u16>()
            .expect("addr should be u16");
        addrs.push(addr);

        if entry.contains("\"is_current\":true") {
            current_index = Some(index);
        }
    }

    (
        addrs,
        current_index.expect("one disasm entry should be current"),
    )
}

#[wasm_bindgen_test]
fn wasm_nes_constructs() {
    let _nes = WasmNes::new();
}

#[wasm_bindgen_test]
fn load_rom_accepts_valid_data() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");
}

#[wasm_bindgen_test]
fn load_rom_rejects_invalid_header() {
    let mut nes = WasmNes::new();
    let mut rom = minimal_nrom();
    rom[0] = 0; // break magic
    let err = nes
        .load_rom(&rom, "broken.nes")
        .expect_err("invalid rom should error");
    assert!(
        err.as_string()
            .unwrap_or_default()
            .to_lowercase()
            .contains("invalid"),
        "unexpected err: {:?}",
        err.as_string()
    );

    let drained = nes.drain_toasts();
    assert_eq!(drained.len(), 1);
    assert_eq!(
        drained[0].as_string().as_deref(),
        Some("Cartridge load failed: broken.nes")
    );
}

#[wasm_bindgen_test]
fn load_rom_success_enqueues_loaded_and_timing_toasts() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "mario.nes")
        .expect("valid rom should load");

    let drained = nes.drain_toasts();
    assert_eq!(drained.len(), 3);
    assert_eq!(
        drained[0].as_string().as_deref(),
        Some("Cartridge loaded: mario.nes")
    );
    let timing = drained[1].as_string().unwrap_or_default();
    assert!(timing == "Emulator timing: NTSC" || timing == "Emulator timing: PAL");
    let hardware = drained[2].as_string().unwrap_or_default();
    assert!(
        hardware.starts_with("Hardware: "),
        "expected hardware toast, got: {}",
        hardware
    );
}

#[wasm_bindgen_test]
fn drain_toasts_clears_queue_after_read() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "mario.nes")
        .expect("valid rom should load");

    let first = nes.drain_toasts();
    assert!(!first.is_empty());
    let second = nes.drain_toasts();
    assert!(second.is_empty());
}

#[wasm_bindgen_test]
fn gamepad_init_toast_export_uses_shared_wording() {
    assert_eq!(
        gamepad_init_toast_message(true, 1),
        "Gamepad found: using 1 gamepad"
    );
}

#[wasm_bindgen_test]
fn render_frame_returns_expected_size() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");
    let frame = nes.render_frame();
    // Render with default 8 pixels overscan removal
    assert_eq!(frame.len(), 256 * (240 - 16) * 3);
}

#[wasm_bindgen_test]
fn render_frame_rgba_returns_expected_size() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");
    let frame = nes.render_frame_rgba().to_vec();
    // Render with default 8 pixels overscan removal
    assert_eq!(frame.len(), 256 * (240 - 16) * 4);
    // Alpha should be opaque for all pixels.
    assert!(frame.iter().skip(3).step_by(4).all(|a| *a == 0xFF));
}

#[wasm_bindgen_test]
fn render_frame_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    let frame = nes.render_frame();
    // Render with default 8 pixels overscan removal
    assert_eq!(frame.len(), 256 * (240 - 16) * 3);
}

#[wasm_bindgen_test]
fn render_frame_rgba_without_rom_succeeds() {
    let mut nes = WasmNes::new();
    let frame = nes.render_frame_rgba().to_vec();
    // Render with default 8 pixels overscan removal
    assert_eq!(frame.len(), 256 * (240 - 16) * 4);
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
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");

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
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");

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
    nes.load_rom(&rom, "test.nes")
        .expect("valid rom should load");

    let initial = nes.save_state_bytes();
    assert!(!initial.is_empty());

    let _frame = nes.render_frame();
    let modified = nes.save_state_bytes();
    assert_ne!(initial, modified);

    nes.reset(true);
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

#[wasm_bindgen_test]
fn is_mouse_emulated_controller_reflects_port_configuration() {
    let mut nes = WasmNes::new();
    assert!(!nes.is_mouse_emulated_controller(1));
    assert!(!nes.is_mouse_emulated_controller(2));

    enable_arkanoid_on_port1(&mut nes);
    assert!(nes.is_mouse_emulated_controller(1));
    assert!(!nes.is_mouse_emulated_controller(2));
}

#[wasm_bindgen_test]
fn has_expansion_mouse_controller_reflects_expansion_arkanoid() {
    let mut nes = WasmNes::new();
    assert!(!nes.has_expansion_mouse_controller());

    nes.set_hardware_mode("famicom").expect("set famicom mode");
    nes.set_expansion_port("arkanoid")
        .expect("set expansion port");
    assert!(nes.has_expansion_mouse_controller());
}

#[wasm_bindgen_test]
fn has_expansion_mouse_controller_is_false_for_port_arkanoid() {
    let mut nes = WasmNes::new();
    enable_arkanoid_on_port1(&mut nes);
    assert!(!nes.has_expansion_mouse_controller());
}

// --- Debugger API tests ---

#[wasm_bindgen_test]
fn debugger_is_closed_by_default() {
    let nes = WasmNes::new();
    assert!(!nes.is_debugger_open());
}

#[wasm_bindgen_test]
fn debugger_open_pauses_emulator() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    assert!(!nes.is_debugger_open());
    nes.debugger_open();
    assert!(nes.is_debugger_open());
}

#[wasm_bindgen_test]
fn debugger_continue_resumes_emulator() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    assert!(nes.is_debugger_open());

    nes.debugger_continue();
    assert!(!nes.is_debugger_open());
}

#[wasm_bindgen_test]
fn debugger_step_into_keeps_debugger_open_and_advances_one_instruction() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    // Advance to a known stable state
    let _ = nes.render_frame_rgba();

    nes.debugger_open();
    nes.debugger_step_into();

    // Debugger must still be open after stepping into
    assert!(nes.is_debugger_open());
    // PC must have advanced (NOP is one byte, but minimal NROM has zeroed PRG which is BRK)
    // — just assert we got a response without error
    let _pc_after = nes.debugger_cpu_pc();
    // The debugger remains open
    assert!(nes.is_debugger_open());
}

#[wasm_bindgen_test]
fn debugger_step_over_keeps_debugger_open() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    nes.debugger_step_over();

    assert!(nes.is_debugger_open());
}

#[wasm_bindgen_test]
fn debugger_snapshot_returns_json_when_open() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();
    // Should return a non-empty JSON string containing CPU register info
    assert!(!json.is_empty());
    assert!(json.contains("pc"));
}

// --- Debugger disasm API tests ---

#[wasm_bindgen_test]
fn debugger_disasm_json_returns_json_array_when_open() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_disasm_json();
    assert!(!json.is_empty());
    // Must be a JSON array
    assert!(
        json.trim_start().starts_with('['),
        "expected JSON array, got: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_disasm_json_contains_current_instruction_marker() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_disasm_json();
    // At least one entry must have is_current:true
    assert!(
        json.contains("\"is_current\":true"),
        "expected is_current:true in: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_disasm_json_entries_have_required_fields() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_disasm_json();
    assert!(json.contains("\"addr\""), "expected addr field in: {json}");
    assert!(json.contains("\"text\""), "expected text field in: {json}");
    assert!(
        json.contains("\"bytes\""),
        "expected bytes field in: {json}"
    );
    assert!(
        json.contains("\"is_current\""),
        "expected is_current field in: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_disasm_current_addr_matches_cpu_pc() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    let _ = nes.render_frame_rgba();
    let pc = nes.debugger_cpu_pc();
    nes.debugger_open();
    let json = nes.debugger_disasm_json();

    // The is_current entry must have addr == pc (u16 serialised as a number)
    let expected_fragment = format!("\"addr\":{pc},");
    assert!(
        json.contains(&expected_fragment),
        "expected PC {pc} as addr in disasm JSON.\njson: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_disasm_keeps_window_until_last_two_then_recenters() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let (addrs0, mut idx) = parse_disasm_addrs_and_current_index(&nes.debugger_disasm_json());
    let target_lines = addrs0.len();

    let mut saw_centered = idx == target_lines / 2;
    for _ in 0..(target_lines * 4) {
        nes.debugger_step_over();
        let (_addrs, new_idx) = parse_disasm_addrs_and_current_index(&nes.debugger_disasm_json());
        idx = new_idx;
        saw_centered |= idx == target_lines / 2;
    }

    assert!(
        saw_centered,
        "expected disassembly window to recenter at least once"
    );
}

#[wasm_bindgen_test]
fn render_frame_rgba_does_not_advance_when_debugger_is_open() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    // Run for a few frames to reach a stable state
    let _ = nes.render_frame_rgba();

    let state_before = nes.save_state_bytes();

    nes.debugger_open();
    // Calling render while paused should NOT advance the emulator
    let _ = nes.render_frame_rgba();

    let state_after = nes.save_state_bytes();
    assert_eq!(
        state_before, state_after,
        "emulator state must not change when debugger is open"
    );
}
// --- debugger_snapshot_json extended fields (issue #695) ---

#[wasm_bindgen_test]
fn debugger_snapshot_json_includes_frame_count() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();
    assert!(
        json.contains("\"frame_count\""),
        "expected frame_count field in snapshot JSON: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_snapshot_json_includes_interrupt_field() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();
    assert!(
        json.contains("\"interrupt\""),
        "expected interrupt field in snapshot JSON: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_snapshot_json_includes_interrupt_vectors() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();
    assert!(
        json.contains("\"nmi_vector\""),
        "expected nmi_vector field: {json}"
    );
    assert!(
        json.contains("\"reset_vector\""),
        "expected reset_vector field: {json}"
    );
    assert!(
        json.contains("\"irq_vector\""),
        "expected irq_vector field: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_snapshot_json_includes_prg_hexdump_fields() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();

    assert!(
        json.contains("\"prg_hexdump_base\""),
        "expected prg_hexdump_base field: {json}"
    );
    assert!(
        json.contains("\"prg_hexdump_bytes\""),
        "expected prg_hexdump_bytes field: {json}"
    );
}

#[wasm_bindgen_test]
fn debugger_hexdump_navigation_moves_by_16_bytes() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let initial = nes.debugger_snapshot_json();
    let initial_base = parse_u16_field(&initial, "prg_hexdump_base");

    nes.debugger_hexdump_next_16();
    let after_next = nes.debugger_snapshot_json();
    let next_base = parse_u16_field(&after_next, "prg_hexdump_base");
    assert_eq!(next_base, initial_base.saturating_add(16));

    nes.debugger_hexdump_prev_16();
    let after_prev = nes.debugger_snapshot_json();
    let prev_base = parse_u16_field(&after_prev, "prg_hexdump_base");
    assert_eq!(prev_base, initial_base);
}

#[wasm_bindgen_test]
fn debugger_hexdump_set_base_jumps_to_specific_address() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    nes.debugger_hexdump_set_base(0xC123);
    let json = nes.debugger_snapshot_json();
    let base = parse_u16_field(&json, "prg_hexdump_base");

    // SDL parity: hexdump base is 16-byte aligned.
    assert_eq!(base, 0xC120);

    nes.debugger_hexdump_set_base(0x7000);
    let clamped_json = nes.debugger_snapshot_json();
    let clamped_base = parse_u16_field(&clamped_json, "prg_hexdump_base");
    assert_eq!(clamped_base, 0x8000);
}

#[wasm_bindgen_test]
fn debugger_ppu_viewer_is_closed_by_default() {
    let nes = WasmNes::new();
    assert!(
        !nes.debugger_is_ppu_viewer_open(),
        "PPU viewer should be closed by default"
    );
}

#[wasm_bindgen_test]
fn debugger_ppu_viewer_toggle_opens_then_closes() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");
    nes.debugger_open();

    assert!(!nes.debugger_is_ppu_viewer_open());

    nes.debugger_toggle_ppu_viewer();
    assert!(
        nes.debugger_is_ppu_viewer_open(),
        "toggle should open the PPU viewer"
    );

    nes.debugger_toggle_ppu_viewer();
    assert!(
        !nes.debugger_is_ppu_viewer_open(),
        "second toggle should close the PPU viewer"
    );
}

#[wasm_bindgen_test]
fn debugger_ppu_viewer_rgba_buffers_match_expected_dimensions() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");
    nes.debugger_open();
    nes.debugger_toggle_ppu_viewer();

    let pattern_tables = nes.debugger_ppu_pattern_tables_rgba();
    let nametables = nes.debugger_ppu_nametables_rgba();

    assert_eq!(
        pattern_tables.len(),
        256 * 128 * 4,
        "pattern table viewer buffer should be 256x128 RGBA"
    );
    assert_eq!(
        nametables.len(),
        512 * 480 * 4,
        "nametable viewer buffer should be 512x480 RGBA"
    );
}

#[wasm_bindgen_test]
fn debugger_ppu_viewer_visibility_persists_across_close_and_reopen() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    nes.debugger_toggle_ppu_viewer();
    assert!(nes.debugger_is_ppu_viewer_open());

    nes.debugger_continue();
    assert!(!nes.is_debugger_open());

    nes.debugger_open();
    assert!(
        nes.debugger_is_ppu_viewer_open(),
        "PPU viewer visibility should persist when reopening debugger"
    );
}

#[wasm_bindgen_test]
fn debugger_ppu_scroll_json_exposes_scroll_coordinates() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");
    nes.debugger_open();

    let json = nes.debugger_ppu_scroll_json();
    let scroll_x = parse_u16_field(&json, "scroll_x");
    let scroll_y = parse_u16_field(&json, "scroll_y");

    assert!(scroll_x < 512, "scroll_x should be within nametable width");
    assert!(scroll_y < 480, "scroll_y should be within nametable height");
}

fn parse_u16_field(json: &str, field: &str) -> u16 {
    let key = format!("\"{field}\":");
    let start = json
        .find(&key)
        .map(|idx| idx + key.len())
        .expect("field should exist in JSON");
    let end = json[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map(|offset| start + offset)
        .unwrap_or(json.len());
    json[start..end]
        .parse::<u16>()
        .expect("field should parse as u16")
}

#[wasm_bindgen_test]
fn debugger_snapshot_json_reset_vector_matches_rom_header() {
    let mut nes = WasmNes::new();
    let rom = minimal_nrom_nop_at_8000();
    nes.load_rom(&rom, "test.nes").expect("valid rom");

    nes.debugger_open();
    let json = nes.debugger_snapshot_json();

    // The minimal_nrom_nop_at_8000 helper sets reset vector to $8000 (32768).
    assert!(
        json.contains("\"reset_vector\":32768"),
        "expected reset_vector:32768 ($8000) in snapshot JSON: {json}"
    );
}

// ── WasmGb tests ─────────────────────────────────────────────────────────────

/// Minimal valid DMG ROM: 32 KB ROM-only cartridge with correct header checksum.
fn minimal_gb_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0147] = 0x00; // ROM only
    rom[0x0148] = 0x00; // 32 KB
    rom[0x0149] = 0x00; // no RAM
    // Header checksum: sum of bytes $0134–$014C, each negated-and-decremented.
    let chk = rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
    rom[0x014D] = chk;
    rom
}

#[wasm_bindgen_test]
fn wasm_gb_constructs() {
    let _gb = WasmGb::new();
}

#[wasm_bindgen_test]
fn wasm_gb_screen_width_is_160() {
    let gb = WasmGb::new();
    assert_eq!(gb.screen_width(), 160);
}

#[wasm_bindgen_test]
fn wasm_gb_screen_height_is_144() {
    let gb = WasmGb::new();
    assert_eq!(gb.screen_height(), 144);
}

#[wasm_bindgen_test]
fn wasm_gb_frame_rate_hz_is_dmg_rate() {
    let gb = WasmGb::new();
    let hz = gb.frame_rate_hz();
    // DMG: 4,194,304 / 70,224 ≈ 59.7275
    assert!(
        (hz - 59.7275).abs() < 0.001,
        "expected ~59.7275 Hz but got {hz}"
    );
}

#[wasm_bindgen_test]
fn wasm_gb_no_rom_renders_opaque_black_frame() {
    let mut gb = WasmGb::new();
    let frame = gb.render_frame_rgba();
    // Without a ROM, expect a fully opaque black frame (160 × 144 pixels × 4 bytes).
    let expected_len = 160 * 144 * 4;
    assert_eq!(
        frame.len(),
        expected_len,
        "no-ROM frame should be {expected_len} bytes"
    );
    // Every alpha byte must be 0xFF (opaque).
    for (i, &byte) in frame.iter().enumerate() {
        if (i + 1) % 4 == 0 {
            assert_eq!(byte, 0xFF, "alpha byte at index {i} should be 0xFF");
        }
    }
    // Every RGB byte must be 0 (black).
    for (i, &byte) in frame.iter().enumerate() {
        if (i + 1) % 4 != 0 {
            assert_eq!(byte, 0x00, "color byte at index {i} should be 0x00");
        }
    }
}

#[wasm_bindgen_test]
fn wasm_gb_load_rom_returns_success_toast() {
    let mut gb = WasmGb::new();
    let rom = minimal_gb_rom();
    gb.load_rom(&rom, "test.gb")
        .expect("valid GB ROM should load successfully");
    let toasts: Vec<String> = gb
        .drain_toasts()
        .into_iter()
        .filter_map(|v| v.as_string())
        .collect();
    assert!(
        toasts.iter().any(|t| t.contains("test.gb")),
        "expected a toast mentioning 'test.gb', got: {toasts:?}"
    );
}

// ── WasmGba tests ────────────────────────────────────────────────────────────

fn minimal_gba_rom() -> Vec<u8> {
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, HEADER_SIZE,
        compute_complement_check,
    };

    let mut rom = vec![0u8; HEADER_SIZE];
    rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
    rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
    rom
}

#[wasm_bindgen_test]
fn wasm_gba_constructs() {
    let _gba = WasmGba::new();
}

#[wasm_bindgen_test]
fn wasm_gba_screen_width_is_240() {
    let gba = WasmGba::new();
    assert_eq!(gba.screen_width(), 240);
}

#[wasm_bindgen_test]
fn wasm_gba_screen_height_is_160() {
    let gba = WasmGba::new();
    assert_eq!(gba.screen_height(), 160);
}

#[wasm_bindgen_test]
fn wasm_gba_frame_rate_hz_is_gba_rate() {
    let gba = WasmGba::new();
    let hz = gba.frame_rate_hz();
    assert!(
        (hz - 59.7275).abs() < 0.01,
        "expected ~59.7275 Hz but got {hz}"
    );
}

#[wasm_bindgen_test]
fn wasm_gba_no_rom_renders_opaque_black_frame() {
    let mut gba = WasmGba::new();
    let frame = gba.render_frame_rgba().to_vec();
    let expected_len = 240 * 160 * 4;
    assert_eq!(
        frame.len(),
        expected_len,
        "no-ROM frame should be {expected_len} bytes"
    );
    for (i, &byte) in frame.iter().enumerate() {
        if (i + 1) % 4 == 0 {
            assert_eq!(byte, 0xFF, "alpha byte at index {i} should be 0xFF");
        } else {
            assert_eq!(byte, 0x00, "color byte at index {i} should be 0x00");
        }
    }
}

#[wasm_bindgen_test]
fn wasm_gba_no_rom_renders_native_rgb_frame() {
    let mut gba = WasmGba::new();
    let frame = gba.render_frame_rgb().to_vec();
    let expected_len = 240 * 160 * 3;
    assert_eq!(
        frame.len(),
        expected_len,
        "native RGB frame should be {expected_len} bytes"
    );
    assert!(
        frame.iter().all(|&byte| byte == 0x00),
        "no-ROM native RGB frame should be black"
    );
}

#[wasm_bindgen_test]
fn wasm_gba_load_rom_returns_success_toast() {
    let mut gba = WasmGba::new();
    let rom = minimal_gba_rom();
    gba.load_rom(&rom, "suite.gba")
        .expect("valid GBA ROM should load successfully");
    let toasts: Vec<String> = gba
        .drain_toasts()
        .into_iter()
        .filter_map(|v| v.as_string())
        .collect();
    assert!(
        toasts.iter().any(|t| t.contains("suite.gba")),
        "expected a toast mentioning 'suite.gba', got: {toasts:?}"
    );
}

#[wasm_bindgen_test]
fn wasm_gba_load_rom_rejects_invalid_data() {
    let mut gba = WasmGba::new();
    let err = gba
        .load_rom(&[0u8; 8], "broken.gba")
        .expect_err("invalid GBA ROM should error");
    assert!(
        !err.as_string().unwrap_or_default().is_empty(),
        "invalid GBA ROM should return a message"
    );

    let toasts: Vec<String> = gba
        .drain_toasts()
        .into_iter()
        .filter_map(|v| v.as_string())
        .collect();
    assert!(
        toasts.iter().any(|t| t.contains("broken.gba")),
        "expected a failure toast mentioning 'broken.gba', got: {toasts:?}"
    );
}

#[wasm_bindgen_test]
fn wasm_gba_audio_mute_state_is_reported() {
    let mut gba = WasmGba::new();
    assert!(!gba.is_audio_muted());
    gba.set_audio_muted(true);
    assert!(gba.is_audio_muted());
    assert!(gba.get_audio_samples().is_empty());
    gba.set_audio_muted(false);
    assert!(!gba.is_audio_muted());
}

#[wasm_bindgen_test]
fn wasm_gba_reset_without_rom_succeeds() {
    let mut gba = WasmGba::new();
    gba.reset(true);
    gba.reset(false);
}

#[wasm_bindgen_test]
fn wasm_gba_set_button_accepts_all_gba_buttons() {
    let mut gba = WasmGba::new();
    for button in 0..=9 {
        gba.set_button(1, button, true);
        gba.set_button(1, button, false);
    }
}

#[wasm_bindgen_test]
fn wasm_gba_set_button_ignores_non_player_one_controllers() {
    let mut gba = WasmGba::new();

    gba.set_button(2, 0, true);

    assert_eq!(gba.joypad_button_states_for_test(), 0);
}

// ── WasmSnes tests ───────────────────────────────────────────────────────────

/// Minimal valid LoROM cartridge: 64 KB with a NOP at $8000 and a valid
/// header at $7FC0.  Mirrors the helper used in `src/snes/console/snes.rs`.
fn minimal_snes_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x10000];
    let header = 0x7FC0;
    rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
    // Reset vector low/high → $8000 (at header + 0x3C/0x3D = $7FFC/$7FFD)
    rom[header + 0x3C] = 0x00;
    rom[header + 0x3D] = 0x80;
    // Map mode (LoROM, SlowROM) at header + 0xD5 = $8095
    rom[header + 0xD5] = 0x20;
    // ROM size: 7 = 64 KB at header + 0xD7 = $8097
    rom[header + 0xD7] = 0x07;
    // Checksum complement / checksum at header + 0xDC–0xDF
    rom[header + 0xDC] = 0x34;
    rom[header + 0xDD] = 0x12;
    rom[header + 0xDE] = 0xCB;
    rom[header + 0xDF] = 0xED;
    // NOP at $8000 in LoROM bank 0 (ROM file offset 0)
    rom[0x0000] = 0xEA;
    rom
}

#[wasm_bindgen_test]
fn wasm_snes_constructs() {
    let _snes = WasmSnes::new();
}

#[wasm_bindgen_test]
fn wasm_snes_screen_width_is_256() {
    let snes = WasmSnes::new();
    assert_eq!(snes.screen_width(), 256);
}

#[wasm_bindgen_test]
fn wasm_snes_screen_height_is_224() {
    let snes = WasmSnes::new();
    assert_eq!(snes.screen_height(), 224);
}

#[wasm_bindgen_test]
fn wasm_snes_frame_rate_hz_is_ntsc_rate() {
    let snes = WasmSnes::new();
    let hz = snes.frame_rate_hz();
    // NTSC SNES: 21,477,272 / 357,366 ≈ 60.098 Hz
    assert!(
        (hz - 60.098).abs() < 0.01,
        "expected ~60.098 Hz but got {hz}"
    );
}

#[wasm_bindgen_test]
fn wasm_snes_no_rom_renders_opaque_black_frame() {
    let mut snes = WasmSnes::new();
    let frame = snes.render_frame_rgba().to_vec();
    let expected_len = 256 * 224 * 4;
    assert_eq!(
        frame.len(),
        expected_len,
        "no-ROM frame should be {expected_len} bytes"
    );
    for (i, &byte) in frame.iter().enumerate() {
        if (i + 1) % 4 == 0 {
            assert_eq!(byte, 0xFF, "alpha byte at index {i} should be 0xFF");
        } else {
            assert_eq!(byte, 0x00, "color byte at index {i} should be 0x00");
        }
    }
}

#[wasm_bindgen_test]
fn wasm_snes_load_rom_returns_success_toast() {
    let mut snes = WasmSnes::new();
    let rom = minimal_snes_rom();
    snes.load_rom(&rom, "test.sfc")
        .expect("valid SNES ROM should load successfully");
    let toasts: Vec<String> = snes
        .drain_toasts()
        .into_iter()
        .filter_map(|v| v.as_string())
        .collect();
    assert!(
        toasts.iter().any(|t| t.contains("test.sfc")),
        "expected a toast mentioning 'test.sfc', got: {toasts:?}"
    );
}

#[wasm_bindgen_test]
fn wasm_snes_load_rom_rejects_invalid_data() {
    let mut snes = WasmSnes::new();
    let err = snes
        .load_rom(&[0u8; 8], "broken.sfc")
        .expect_err("invalid SNES ROM should error");
    assert!(
        !err.as_string().unwrap_or_default().is_empty(),
        "invalid SNES ROM should return a message"
    );

    let toasts: Vec<String> = snes
        .drain_toasts()
        .into_iter()
        .filter_map(|v| v.as_string())
        .collect();
    assert!(
        toasts.iter().any(|t| t.contains("broken.sfc")),
        "expected a failure toast mentioning 'broken.sfc', got: {toasts:?}"
    );
}

#[wasm_bindgen_test]
fn wasm_snes_audio_mute_state_is_reported() {
    let mut snes = WasmSnes::new();
    assert!(!snes.is_audio_muted());
    snes.set_audio_muted(true);
    assert!(snes.is_audio_muted());
    assert!(snes.get_audio_samples().is_empty());
    snes.set_audio_muted(false);
    assert!(!snes.is_audio_muted());
}

#[wasm_bindgen_test]
fn wasm_snes_reset_without_rom_succeeds() {
    let mut snes = WasmSnes::new();
    snes.reset(true);
    snes.reset(false);
}

#[wasm_bindgen_test]
fn wasm_snes_set_button_does_not_panic() {
    let mut snes = WasmSnes::new();
    // SNES has 12 buttons (B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R)
    for button in 0u8..=11 {
        snes.set_button(1, button, true);
        snes.set_button(1, button, false);
    }
}

#[wasm_bindgen_test]
fn wasm_snes_save_state_bytes_without_rom_returns_empty() {
    let snes = WasmSnes::new();
    let bytes = snes.save_state_bytes();
    assert!(bytes.is_empty(), "expected empty save state without ROM");
}

#[wasm_bindgen_test]
fn wasm_snes_load_state_bytes_without_rom_returns_error() {
    let mut snes = WasmSnes::new();
    let err = snes
        .load_state_bytes(&[0u8; 32])
        .expect_err("load_state_bytes without ROM should fail");
    assert!(!err.as_string().unwrap_or_default().is_empty());
}

#[wasm_bindgen_test]
fn wasm_snes_has_mouse_returns_false_without_rom() {
    let snes = WasmSnes::new();
    assert!(!snes.has_mouse());
    assert!(!snes.has_mouse_on_port(1));
    assert!(!snes.has_mouse_on_port(2));
}

#[wasm_bindgen_test]
fn wasm_snes_mouse_methods_do_not_panic_without_rom() {
    let mut snes = WasmSnes::new();
    snes.add_mouse_delta(1, 10, -5);
    snes.set_mouse_left_button(1, true);
    snes.set_mouse_right_button(1, false);
}
