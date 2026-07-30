use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;
use crate::snes::input::{SnesButton, SnesControllerType};
use std::path::Path;
use std::path::PathBuf;

pub(crate) const MARKER_ADDR: u32 = 0x7E_1FF0;
pub(crate) const MARKER_MAGIC: [u8; 4] = *b"NSER";
pub(crate) const PASS_STATUS: u8 = 0x01;
pub(crate) const FAIL_STATUS: u8 = 0x02;
pub(crate) const PASS_IDLE_PC: u16 = 0x8100;
pub(crate) const FAIL_IDLE_PC: u16 = 0x8110;
pub(crate) const TIMEOUT_IDLE_PC: u16 = 0x8120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunOracle {
    Marker,
    BusByte {
        addr: u32,
        pass_value: u8,
        fail_value: u8,
    },
    /// Run to a fixed frame, then compare the rendered screen CRC32 against a
    /// human-approved golden value. Used for blargg-style ROMs that only report
    /// pass/fail by drawing text to the screen.
    ScreenCrc {
        frames: u32,
        expected_crc: u32,
    },
}

/// A mouse button on a scripted SNES Mouse device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseButton {
    Left,
    Right,
}

/// The device change a scripted [`InputEvent`] applies. `port` is 0 for
/// controller port 1 and 1 for controller port 2, matching the `Snes`
/// input-injection APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputAction {
    /// A standard-pad button edge.
    Button {
        port: u8,
        button: SnesButton,
        pressed: bool,
    },
    /// Relative SNES Mouse motion in host-space units.
    MouseDelta { port: u8, dx: i16, dy: i16 },
    /// An SNES Mouse button edge.
    MouseButton {
        port: u8,
        button: MouseButton,
        pressed: bool,
    },
}

/// A scripted input change, applied once the runner's completed-frame
/// counter reaches `frame`: the device state is set before any tick of the
/// following frame executes, so the change is picked up by the auto-joypad
/// read ($4218-$421B) at most one frame later (the frame counter advances at
/// VBlank entry, slightly before the auto-joypad latch dot, so exact pickup
/// depends on where the CPU instruction boundary falls). Events sharing a
/// frame stamp are applied in list order as one atomic update before any
/// tick runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputEvent {
    pub frame: u32,
    pub action: InputAction,
}

impl InputEvent {
    /// A controller-port-1 standard-pad button edge — the shape every
    /// pre-existing scripted suite uses.
    pub(crate) const fn button(frame: u32, button: SnesButton, pressed: bool) -> Self {
        Self {
            frame,
            action: InputAction::Button {
                port: 0,
                button,
                pressed,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RunConfig<'a> {
    pub max_ticks: u64,
    pub max_frames: u32,
    /// If set, a soft CPU reset is triggered every time the PC reaches this address. Some
    /// blargg SPC/APU ROMs (e.g. `timer_at_power_reset.smc`) signal "please reset now" by
    /// jumping into zeroed-out low WRAM ($0000), which the real test rig/hardware answers
    /// with a physical reset button press; this models that handshake for automated runs.
    pub reset_on_pc_trap: Option<u16>,
    /// Scripted input edges, sorted by [`InputEvent::frame`] (ascending;
    /// validated at run start). Used to drive interactive test ROMs (e.g.
    /// byuu's `test_oam` menu) deterministically.
    pub input_script: &'a [InputEvent],
    /// Device connected to controller port 1 (default: standard pad).
    pub controller_port1: SnesControllerType,
    /// Device connected to controller port 2 (default: standard pad).
    pub controller_port2: SnesControllerType,
}

impl<'a> RunConfig<'a> {
    pub(crate) const fn new(max_ticks: u64, max_frames: u32) -> Self {
        Self {
            max_ticks,
            max_frames,
            reset_on_pc_trap: None,
            input_script: &[],
            controller_port1: SnesControllerType::Standard,
            controller_port2: SnesControllerType::Standard,
        }
    }

    /// Selects the devices connected to controller ports 1 and 2.
    pub(crate) const fn with_controller_ports(
        mut self,
        port1: SnesControllerType,
        port2: SnesControllerType,
    ) -> Self {
        self.controller_port1 = port1;
        self.controller_port2 = port2;
        self
    }

    /// Enables the reset-on-PC-trap handshake (see [`RunConfig::reset_on_pc_trap`]).
    pub(crate) const fn with_reset_on_pc_trap(mut self, trap_pc: u16) -> Self {
        self.reset_on_pc_trap = Some(trap_pc);
        self
    }

    /// Attaches a scripted controller-1 input sequence (see
    /// [`RunConfig::input_script`]).
    pub(crate) const fn with_input_script(mut self, script: &'a [InputEvent]) -> Self {
        self.input_script = script;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunExitReason {
    PassMarker,
    FailMarker,
    TickLimit,
    FrameLimit,
    /// The screen-CRC oracle reached its target frame and compared the screen.
    ScreenCrcFrame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunResult {
    pub passed: bool,
    pub exit_reason: RunExitReason,
    pub ticks: u64,
    pub frames: u32,
    pub pc: u16,
    pub marker: [u8; 5],
    pub screen_crc32: u32,
    /// Final RGB888 frame (`256 * 224 * 3`, row-major, the same layout as
    /// `Snes::screen_snapshot`), for tests that need to assert on pixels rather than on a
    /// golden CRC. Populated only when the [`RunOracle::ScreenCrc`] oracle actually reached
    /// its target frame, so the marker/bus-byte suites pay no allocation -- a ScreenCrc run
    /// that instead hits the tick limit carries `None`.
    pub screen_rgb: Option<Vec<u8>>,
    pub capture_path: Option<PathBuf>,
}

pub(crate) fn run_rom(rom: &[u8], name: &str, config: RunConfig) -> RunResult {
    run_rom_with_oracle_and_capture(
        rom,
        name,
        "rom_runner",
        config,
        RunOracle::Marker,
        std::env::var_os("NESER_CAPTURE_SCREEN").is_some(),
    )
}

/// `suite` names the calling integration-test module (e.g. `"blargg_apu_tests"`)
/// and is used only to namespace `NESER_CAPTURE_SCREEN` PNG output under
/// `target/snes_test_captures/<suite>/` so captures from different ROM suites
/// don't collide or land in one flat directory.
pub(crate) fn run_rom_with_oracle(
    rom: &[u8],
    name: &str,
    suite: &str,
    config: RunConfig,
    oracle: RunOracle,
) -> RunResult {
    run_rom_with_oracle_and_capture(
        rom,
        name,
        suite,
        config,
        oracle,
        std::env::var_os("NESER_CAPTURE_SCREEN").is_some(),
    )
}

/// Reads `root/file`, runs it with the screen-CRC oracle, and asserts it
/// passes at `frames` with `expected_crc`. Shared by ROM suites whose ROMs
/// report pass/fail purely by drawing text to the screen and freezing
/// (blargg- and gilyon-style test ROMs). See [`run_rom_with_oracle`] for
/// what `suite` is used for.
pub(crate) fn assert_rom_screen_crc(
    root: &str,
    file: &str,
    suite: &str,
    frames: u32,
    expected_crc: u32,
    config: RunConfig,
) {
    let path = Path::new(root).join(file);
    let rom = std::fs::read(&path)
        .unwrap_or_else(|err| panic!("failed to read ROM {}: {err}", path.display()));
    let result = run_rom_with_oracle(
        &rom,
        file,
        suite,
        config,
        RunOracle::ScreenCrc {
            frames,
            expected_crc,
        },
    );
    assert!(
        result.passed && result.exit_reason == RunExitReason::ScreenCrcFrame,
        "{file}: expected screen-CRC PASS at frame {frames}, \
         got crc=0x{:08X} passed={} exit={:?}",
        result.screen_crc32,
        result.passed,
        result.exit_reason
    );
}

fn run_rom_with_capture(
    rom: &[u8],
    name: &str,
    config: RunConfig,
    capture_screen: bool,
) -> RunResult {
    run_rom_with_oracle_and_capture(
        rom,
        name,
        "rom_runner",
        config,
        RunOracle::Marker,
        capture_screen,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_rom_with_oracle_and_capture(
    rom: &[u8],
    name: &str,
    suite: &str,
    config: RunConfig,
    oracle: RunOracle,
    capture_screen: bool,
) -> RunResult {
    let mut app_config = crate::platform::config::Config::default();
    app_config.snes.controller_port1 = config.controller_port1;
    app_config.snes.controller_port2 = config.controller_port2;
    let mut snes = Snes::new(AppContext::new_with_config(app_config));
    snes.load_rom(rom, name)
        .unwrap_or_else(|err| panic!("failed to load SNES runner ROM {name}: {err}"));

    let script = config.input_script;
    assert!(
        script.windows(2).all(|pair| pair[0].frame <= pair[1].frame),
        "input_script must be sorted by frame (ascending) for {name}"
    );
    assert!(
        config.max_frames == 0
            || script
                .last()
                .is_none_or(|event| event.frame < config.max_frames),
        "input_script for {name} has an event at frame {} that can never fire \
         (max_frames is {})",
        script.last().map(|event| event.frame).unwrap_or(0),
        config.max_frames
    );

    let mut ticks = 0u64;
    let mut frames = 0u32;
    let mut resets_triggered = 0u32;
    let mut next_input = 0usize;
    const MAX_AUTO_RESETS: u32 = 16;

    loop {
        while next_input < script.len() && script[next_input].frame <= frames {
            let event = script[next_input];
            match event.action {
                InputAction::Button {
                    port,
                    button,
                    pressed,
                } => {
                    snes.set_button(port, crate::snes::input::button_to_id(button), pressed);
                }
                InputAction::MouseDelta { port, dx, dy } => {
                    snes.add_mouse_delta(port, dx, dy);
                }
                InputAction::MouseButton {
                    port,
                    button,
                    pressed,
                } => match button {
                    MouseButton::Left => snes.set_mouse_left_button(port, pressed),
                    MouseButton::Right => snes.set_mouse_right_button(port, pressed),
                },
            }
            next_input += 1;
        }

        if let Some(trap_pc) = config.reset_on_pc_trap
            && snes.cpu_pc_for_tests() == Some(trap_pc)
            && resets_triggered < MAX_AUTO_RESETS
        {
            snes.reset(true);
            resets_triggered += 1;
            continue;
        }

        let step_ticks = u64::from(snes.run_tick());
        ticks = ticks.saturating_add(step_ticks);

        if snes.is_ready_to_render() {
            frames = frames.saturating_add(1);
            snes.clear_ready_to_render();
        }

        let pc = snes.cpu_pc_for_tests().unwrap_or(0);
        let marker = read_marker(&snes);
        if let Some((exit_reason, passed)) = evaluate_oracle(&snes, oracle, pc, marker, frames) {
            return finish_result(
                &snes,
                name,
                suite,
                exit_reason,
                passed,
                ticks,
                frames,
                pc,
                marker,
                capture_screen,
            );
        }

        if config.max_frames != 0 && frames >= config.max_frames {
            return finish_result(
                &snes,
                name,
                suite,
                RunExitReason::FrameLimit,
                false,
                ticks,
                frames,
                pc,
                marker,
                capture_screen,
            );
        }

        if config.max_ticks != 0 && ticks >= config.max_ticks {
            return finish_result(
                &snes,
                name,
                suite,
                RunExitReason::TickLimit,
                false,
                ticks,
                frames,
                pc,
                marker,
                capture_screen,
            );
        }
    }
}

fn evaluate_oracle(
    snes: &Snes,
    oracle: RunOracle,
    pc: u16,
    marker: [u8; 5],
    frames: u32,
) -> Option<(RunExitReason, bool)> {
    match oracle {
        RunOracle::Marker if marker[..4] == MARKER_MAGIC => match (marker[4], pc) {
            (PASS_STATUS, PASS_IDLE_PC) => Some((RunExitReason::PassMarker, true)),
            (FAIL_STATUS, FAIL_IDLE_PC) => Some((RunExitReason::FailMarker, false)),
            _ => None,
        },
        RunOracle::BusByte {
            addr,
            pass_value,
            fail_value,
        } => {
            let value = snes.read_bus_for_debugger_for_tests(addr).unwrap_or(0);
            match (value, pc) {
                (v, PASS_IDLE_PC) if v == pass_value => Some((RunExitReason::PassMarker, true)),
                (v, FAIL_IDLE_PC) if v == fail_value => Some((RunExitReason::FailMarker, false)),
                _ => None,
            }
        }
        RunOracle::ScreenCrc {
            frames: target_frames,
            expected_crc,
        } if frames >= target_frames => {
            let actual_crc = snes.screen_crc32();
            Some((RunExitReason::ScreenCrcFrame, actual_crc == expected_crc))
        }
        _ => None,
    }
}

fn read_marker(snes: &Snes) -> [u8; 5] {
    std::array::from_fn(|offset| {
        snes.read_bus_for_debugger_for_tests(MARKER_ADDR + offset as u32)
            .unwrap_or(0)
    })
}

#[allow(clippy::too_many_arguments)]
fn finish_result(
    snes: &Snes,
    name: &str,
    suite: &str,
    exit_reason: RunExitReason,
    passed: bool,
    ticks: u64,
    frames: u32,
    pc: u16,
    marker: [u8; 5],
    capture_screen: bool,
) -> RunResult {
    // A single snapshot serves all three consumers: the golden CRC, the optional PNG
    // capture, and `screen_rgb`. `Snes::screen_crc32` is defined as crc32 over exactly
    // these bytes -- an invariant pinned by
    // `snes::console::snes::tests::screen_crc32_matches_snapshot_crc` -- so deriving the
    // CRC from them here cannot drift from the golden's definition.
    let rgb = snes.screen_snapshot();
    let screen_crc32 = crate::platform::crc32::crc32(&[&rgb]);
    let capture_path =
        maybe_write_capture_png(snes, name, suite, screen_crc32, capture_screen, &rgb);
    let screen_rgb = (exit_reason == RunExitReason::ScreenCrcFrame).then_some(rgb);

    RunResult {
        passed,
        exit_reason,
        ticks,
        frames,
        pc,
        marker,
        screen_crc32,
        screen_rgb,
        capture_path,
    }
}

fn maybe_write_capture_png(
    snes: &Snes,
    name: &str,
    suite: &str,
    crc: u32,
    capture_screen: bool,
    rgb: &[u8],
) -> Option<PathBuf> {
    if !capture_screen || capture_is_disabled_for_fixture(name) {
        return None;
    }

    let path = capture_output_path(suite, &capture_stem(name), crc);
    crate::platform::png_utils::write_rgb_png(
        &path,
        rgb,
        snes.screen_width(),
        snes.screen_height(),
    );
    Some(path)
}

fn capture_is_disabled_for_fixture(name: &str) -> bool {
    let stem = Path::new(name).file_stem().and_then(|stem| stem.to_str());

    matches!(
        stem,
        Some("bus-byte-pass")
            | Some("fail")
            | Some("input-mouse-delta")
            | Some("input-mouse-id")
            | Some("input-mouse-left")
            | Some("input-mouse-right")
            | Some("input-port2-press")
            | Some("input-script-none")
            | Some("input-script-press")
            | Some("input-script-release")
            | Some("input-script-unsorted")
            | Some("mouse-clamp")
            | Some("mouse-example-sequence")
            | Some("mouse-identify")
            | Some("mouse-port2")
            | Some("mouse-speed-cycle")
            | Some("pad-auto-matches-serial")
            | Some("pad-example-sequence")
            | Some("pad-serial-order")
            | Some("pad-strobe-semantics")
            | Some("pad2-serial-order")
            | Some("pass")
            | Some("screen-crc-match")
            | Some("screen-crc-mismatch")
            | Some("screen-crc-probe")
            | Some("timeout")
    )
}

fn capture_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("snes_rom");

    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn capture_output_path(suite: &str, stem: &str, crc: u32) -> PathBuf {
    PathBuf::from("target/snes_test_captures")
        .join(suite)
        .join(format!("{stem}_crc_{crc:08X}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::integration_tests::fixture_rom::FixtureRom;

    fn pass_marker_rom() -> Vec<u8> {
        fixture_rom(Some(PASS_STATUS), PASS_IDLE_PC)
    }

    fn fail_marker_rom() -> Vec<u8> {
        fixture_rom(Some(FAIL_STATUS), FAIL_IDLE_PC)
    }

    fn timeout_rom() -> Vec<u8> {
        fixture_rom(None, TIMEOUT_IDLE_PC)
    }

    fn pass_bus_byte_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        emit_write_long(&mut rom, &mut cursor, 0x7E_1FE0, PASS_STATUS);
        emit_jmp_abs(&mut rom, &mut cursor, PASS_IDLE_PC);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        rom
    }

    fn fixture_rom(status: Option<u8>, idle_pc: u16) -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        if let Some(status) = status {
            for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
                emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + offset as u32, byte);
            }
            emit_write_long(&mut rom, &mut cursor, MARKER_ADDR + 4, status);
        }
        emit_jmp_abs(&mut rom, &mut cursor, idle_pc);
        write_idle_loop(&mut rom, PASS_IDLE_PC);
        write_idle_loop(&mut rom, FAIL_IDLE_PC);
        write_idle_loop(&mut rom, TIMEOUT_IDLE_PC);
        rom
    }

    fn write_lorom_header(rom: &mut [u8]) {
        let header = 0x7FC0;
        let title = b"SNES RUNNER TEST";
        rom[header..header + 21].fill(b' ');
        rom[header..header + title.len()].copy_from_slice(title);
        rom[header + 0x15] = 0x20;
        rom[header + 0x16] = 0x00;
        rom[header + 0x17] = 0x07;
        rom[header + 0x18] = 0x00;
        rom[header + 0x1C] = 0x34;
        rom[header + 0x1D] = 0x12;
        rom[header + 0x1E] = 0xCB;
        rom[header + 0x1F] = 0xED;
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
    }

    fn emit_write_long(rom: &mut [u8], cursor: &mut usize, addr: u32, value: u8) {
        rom[*cursor] = 0xA9; // LDA #imm
        rom[*cursor + 1] = value;
        rom[*cursor + 2] = 0x8F; // STA long
        rom[*cursor + 3] = (addr & 0xFF) as u8;
        rom[*cursor + 4] = ((addr >> 8) & 0xFF) as u8;
        rom[*cursor + 5] = ((addr >> 16) & 0xFF) as u8;
        *cursor += 6;
    }

    fn emit_jmp_abs(rom: &mut [u8], cursor: &mut usize, addr: u16) {
        rom[*cursor] = 0x4C;
        rom[*cursor + 1] = (addr & 0x00FF) as u8;
        rom[*cursor + 2] = (addr >> 8) as u8;
        *cursor += 3;
    }

    fn emit_lda_abs(rom: &mut [u8], cursor: &mut usize, addr: u16) {
        rom[*cursor] = 0xAD;
        rom[*cursor + 1] = (addr & 0x00FF) as u8;
        rom[*cursor + 2] = (addr >> 8) as u8;
        *cursor += 3;
    }

    fn emit_cmp_imm(rom: &mut [u8], cursor: &mut usize, value: u8) {
        rom[*cursor] = 0xC9;
        rom[*cursor + 1] = value;
        *cursor += 2;
    }

    fn emit_bne(rom: &mut [u8], cursor: &mut usize, rel: i8) {
        rom[*cursor] = 0xD0;
        rom[*cursor + 1] = rel as u8;
        *cursor += 2;
    }

    fn emit_pass_marker_and_idle(rom: &mut [u8], cursor: &mut usize) {
        for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
            emit_write_long(rom, cursor, MARKER_ADDR + offset as u32, byte);
        }
        emit_write_long(rom, cursor, MARKER_ADDR + 4, PASS_STATUS);
        emit_jmp_abs(rom, cursor, PASS_IDLE_PC);
        write_idle_loop(rom, PASS_IDLE_PC);
    }

    /// Start button bit in JOY1H ($4219): B Y Select Start Up Down Left Right
    /// from bit 7 down to bit 0.
    const START_BIT_JOY1H: u8 = 0x10;

    /// A ROM that enables auto-joypad reads, polls JOY1H ($4219) until it
    /// equals `expected_joy1h`, then reports PASS via the WRAM marker.
    fn joypad_press_wait_rom(expected_joy1h: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        emit_write_long(&mut rom, &mut cursor, 0x00_4200, 0x01);
        emit_lda_abs(&mut rom, &mut cursor, 0x4219);
        emit_cmp_imm(&mut rom, &mut cursor, expected_joy1h);
        emit_bne(&mut rom, &mut cursor, -7);
        emit_pass_marker_and_idle(&mut rom, &mut cursor);
        rom
    }

    /// Like [`joypad_press_wait_rom`], but after observing the press it also
    /// waits for JOY1H to return to zero (all buttons released) before
    /// reporting PASS, so it can only pass if a release edge is delivered.
    fn joypad_press_release_wait_rom(expected_joy1h: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        write_lorom_header(&mut rom);

        let mut cursor = 0usize;
        emit_write_long(&mut rom, &mut cursor, 0x00_4200, 0x01);
        emit_lda_abs(&mut rom, &mut cursor, 0x4219);
        emit_cmp_imm(&mut rom, &mut cursor, expected_joy1h);
        emit_bne(&mut rom, &mut cursor, -7);
        emit_lda_abs(&mut rom, &mut cursor, 0x4219);
        emit_bne(&mut rom, &mut cursor, -5);
        emit_pass_marker_and_idle(&mut rom, &mut cursor);
        rom
    }

    fn write_idle_loop(rom: &mut [u8], pc: u16) {
        let mut cursor = usize::from(pc - 0x8000);
        emit_jmp_abs(rom, &mut cursor, pc);
    }

    fn short_config() -> RunConfig<'static> {
        RunConfig::new(10_000, 2)
    }

    #[test]
    fn pass_fixture_exits_with_pass_marker_and_diagnostics() {
        let result = run_rom(&pass_marker_rom(), "pass.sfc", short_config());

        assert!(result.passed);
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
        assert_eq!(result.pc, PASS_IDLE_PC);
        assert_eq!(&result.marker[..4], &MARKER_MAGIC);
        assert_eq!(result.marker[4], PASS_STATUS);
        assert!(result.ticks > 0);
        assert!(result.screen_crc32 != 0);
    }

    #[test]
    fn fail_fixture_exits_with_fail_marker_and_diagnostics() {
        let result = run_rom(&fail_marker_rom(), "fail.sfc", short_config());

        assert!(!result.passed);
        assert_eq!(result.exit_reason, RunExitReason::FailMarker);
        assert_eq!(result.pc, FAIL_IDLE_PC);
        assert_eq!(&result.marker[..4], &MARKER_MAGIC);
        assert_eq!(result.marker[4], FAIL_STATUS);
        assert!(result.ticks > 0);
        assert!(result.screen_crc32 != 0);
    }

    #[test]
    fn timeout_fixture_reports_tick_limit_with_diagnostics() {
        let result = run_rom(&timeout_rom(), "timeout.sfc", RunConfig::new(100, 60));

        assert!(!result.passed);
        assert_eq!(result.exit_reason, RunExitReason::TickLimit);
        assert_eq!(result.pc, TIMEOUT_IDLE_PC);
        assert_eq!(result.marker, [0; 5]);
        assert!(result.ticks >= 100);
        assert!(result.screen_crc32 != 0);
    }

    #[test]
    fn frame_limit_is_reported_before_tick_limit_when_frame_budget_is_lower() {
        let result = run_rom(&timeout_rom(), "timeout.sfc", RunConfig::new(u64::MAX, 1));

        assert!(!result.passed);
        assert_eq!(result.exit_reason, RunExitReason::FrameLimit);
        assert_eq!(result.frames, 1);
    }

    #[test]
    fn zero_tick_limit_is_disabled_like_zero_frame_limit() {
        let result = run_rom(&timeout_rom(), "timeout.sfc", RunConfig::new(0, 1));

        assert!(!result.passed);
        assert_eq!(result.exit_reason, RunExitReason::FrameLimit);
        assert_eq!(result.frames, 1);
        assert!(result.ticks > 0);
    }

    #[test]
    fn screen_crc_oracle_exposes_the_final_frame_pixels() {
        // `screen_rgb` is the substitute oracle for vectors that cannot be cross-checked
        // against a reference emulator (see neser_obj_tests' obj-y-wrap structural test).
        let rom = pass_marker_rom();
        let with_pixels = run_rom_with_oracle(
            &rom,
            "screen-rgb-probe.sfc",
            "rom_runner",
            RunConfig::new(20_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 3,
                expected_crc: 0xDEAD_BEEF,
            },
        );
        assert_eq!(with_pixels.exit_reason, RunExitReason::ScreenCrcFrame);
        let rgb = with_pixels
            .screen_rgb
            .expect("a ScreenCrc run that reached its frame captures the pixels");
        assert_eq!(rgb.len(), 256 * 224 * 3, "RGB888 at the SNES screen size");

        // Other oracles must not pay for a snapshot they never look at.
        let marker = run_rom(&rom, "screen-rgb-marker.sfc", RunConfig::new(20_000_000, 0));
        assert!(
            marker.screen_rgb.is_none(),
            "non-ScreenCrc runs carry no frame"
        );
    }

    #[test]
    fn screen_crc_oracle_runs_to_target_frame_and_passes_on_matching_crc() {
        let rom = pass_marker_rom();

        // Probe with a deliberately-wrong expected CRC to discover the actual
        // screen CRC at the target frame without hard-coding it.
        let probe = run_rom_with_oracle(
            &rom,
            "screen-crc-probe.sfc",
            "rom_runner",
            RunConfig::new(20_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 3,
                expected_crc: 0xDEAD_BEEF,
            },
        );
        assert_eq!(probe.exit_reason, RunExitReason::ScreenCrcFrame);
        assert_eq!(probe.frames, 3);
        assert!(!probe.passed, "probe with a wrong CRC must not pass");
        let actual_crc = probe.screen_crc32;

        // Replaying with the discovered CRC must pass at the same frame.
        let result = run_rom_with_oracle(
            &rom,
            "screen-crc-match.sfc",
            "rom_runner",
            RunConfig::new(20_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 3,
                expected_crc: actual_crc,
            },
        );
        assert!(result.passed, "matching CRC must pass");
        assert_eq!(result.exit_reason, RunExitReason::ScreenCrcFrame);
        assert_eq!(result.frames, 3);
        assert_eq!(result.screen_crc32, actual_crc);
    }

    #[test]
    fn screen_crc_oracle_reports_failure_on_crc_mismatch() {
        let result = run_rom_with_oracle(
            &pass_marker_rom(),
            "screen-crc-mismatch.sfc",
            "rom_runner",
            RunConfig::new(20_000_000, 0),
            RunOracle::ScreenCrc {
                frames: 2,
                expected_crc: 0x0000_0001,
            },
        );

        assert!(!result.passed);
        assert_eq!(result.exit_reason, RunExitReason::ScreenCrcFrame);
        assert_eq!(result.frames, 2);
    }

    #[test]
    fn bus_byte_oracle_can_mark_pass_without_wram_marker() {
        let result = run_rom_with_oracle(
            &pass_bus_byte_rom(),
            "bus-byte-pass.sfc",
            "rom_runner",
            short_config(),
            RunOracle::BusByte {
                addr: 0x7E_1FE0,
                pass_value: PASS_STATUS,
                fail_value: FAIL_STATUS,
            },
        );

        assert!(result.passed);
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
        assert_eq!(result.pc, PASS_IDLE_PC);
        assert_eq!(result.marker, [0; 5]);
    }

    #[test]
    fn scripted_press_edge_reaches_the_rom_via_auto_joypad() {
        let script = [InputEvent::button(5, SnesButton::Start, true)];
        let result = run_rom(
            &joypad_press_wait_rom(START_BIT_JOY1H),
            "input-script-press.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe the scripted Start press via $4219 \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn scripted_release_edge_reaches_the_rom_via_auto_joypad() {
        let script = [
            InputEvent::button(5, SnesButton::Start, true),
            InputEvent::button(30, SnesButton::Start, false),
        ];
        let result = run_rom(
            &joypad_press_release_wait_rom(START_BIT_JOY1H),
            "input-script-release.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe the Start press followed by its release \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn joypad_wait_rom_times_out_without_an_input_script() {
        let result = run_rom(
            &joypad_press_wait_rom(START_BIT_JOY1H),
            "input-script-none.sfc",
            RunConfig::new(400_000_000, 30),
        );

        assert!(
            !result.passed,
            "without scripted input the polling ROM must never see the press"
        );
        assert_eq!(result.exit_reason, RunExitReason::FrameLimit);
    }

    #[test]
    #[should_panic(expected = "input_script must be sorted by frame")]
    fn unsorted_input_script_panics_at_run_start() {
        let script = [
            InputEvent::button(9, SnesButton::Start, true),
            InputEvent::button(5, SnesButton::Start, false),
        ];
        run_rom(
            &joypad_press_wait_rom(START_BIT_JOY1H),
            "input-script-unsorted.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );
    }

    /// Builds a fixture that enables auto-joypad reads and polls the given
    /// JOY register until it equals `expected`, then reports PASS.
    fn autojoy_poll_rom(joy_addr: u16, expected: u8) -> Vec<u8> {
        let mut fx = FixtureRom::new(b"NESER AUTOJOY POLL");
        fx.write_long(0x00_4200, 0x01);
        let poll = fx.pos();
        fx.lda_abs(joy_addr);
        fx.cmp_imm(expected);
        fx.bne_to(poll);
        fx.pass_marker_and_idle();
        fx.build()
    }

    /// Builds a fixture that manually strobes and serially reads a full
    /// 32-bit mouse packet from `$4016`, polling until byte 2 shows an idle
    /// mouse (ID only) and bytes 3/4 match the expected vertical/horizontal
    /// motion bytes, then reports PASS.
    fn mouse_serial_delta_rom(expected_vertical: u8, expected_horizontal: u8) -> Vec<u8> {
        let mut fx = FixtureRom::new(b"NESER MOUSE DELTA");
        let poll = fx.pos();
        fx.strobe_pulse();
        fx.serial_read_bits(0x4016, 32, 0x0010);
        fx.lda_abs(0x0011); // buttons | speed | ID
        fx.cmp_imm(0x01);
        fx.bne_to(poll);
        fx.lda_abs(0x0012); // vertical direction + magnitude
        fx.cmp_imm(expected_vertical);
        fx.bne_to(poll);
        fx.lda_abs(0x0013); // horizontal direction + magnitude
        fx.cmp_imm(expected_horizontal);
        fx.bne_to(poll);
        fx.pass_marker_and_idle();
        fx.build()
    }

    #[test]
    fn scripted_port2_button_press_reaches_the_rom_via_joy2() {
        let script = [InputEvent {
            frame: 5,
            action: InputAction::Button {
                port: 1,
                button: SnesButton::Start,
                pressed: true,
            },
        }];
        let result = run_rom(
            &autojoy_poll_rom(0x421B, START_BIT_JOY1H),
            "input-port2-press.sfc",
            RunConfig::new(400_000_000, 120).with_input_script(&script),
        );

        assert!(
            result.passed,
            "ROM should observe the scripted port-2 Start press via $421B \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn mouse_on_port1_reports_its_id_through_auto_joypad() {
        let result = run_rom(
            &autojoy_poll_rom(0x4218, 0x01),
            "input-mouse-id.sfc",
            RunConfig::new(400_000_000, 120)
                .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard),
        );

        assert!(
            result.passed,
            "with a mouse on port 1, JOY1L should read the mouse ID 0x01 \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn scripted_mouse_left_button_press_reaches_joy1l() {
        let script = [InputEvent {
            frame: 5,
            action: InputAction::MouseButton {
                port: 0,
                button: MouseButton::Left,
                pressed: true,
            },
        }];
        let result = run_rom(
            &autojoy_poll_rom(0x4218, 0x41),
            "input-mouse-left.sfc",
            RunConfig::new(400_000_000, 120)
                .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard)
                .with_input_script(&script),
        );

        assert!(
            result.passed,
            "JOY1L should show the mouse left button (0x40) plus ID (0x01) \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn scripted_mouse_right_button_press_reaches_joy1l() {
        let script = [InputEvent {
            frame: 5,
            action: InputAction::MouseButton {
                port: 0,
                button: MouseButton::Right,
                pressed: true,
            },
        }];
        let result = run_rom(
            &autojoy_poll_rom(0x4218, 0x81),
            "input-mouse-right.sfc",
            RunConfig::new(400_000_000, 120)
                .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard)
                .with_input_script(&script),
        );

        assert!(
            result.passed,
            "JOY1L should show the mouse right button (0x80) plus ID (0x01) \
             (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn scripted_mouse_delta_is_visible_in_the_serial_packet() {
        let script = [InputEvent {
            frame: 5,
            action: InputAction::MouseDelta {
                port: 0,
                dx: 5,
                dy: 0,
            },
        }];
        let result = run_rom(
            &mouse_serial_delta_rom(0x00, 0x05),
            "input-mouse-delta.sfc",
            RunConfig::new(400_000_000, 120)
                .with_controller_ports(SnesControllerType::Mouse, SnesControllerType::Standard)
                .with_input_script(&script),
        );

        assert!(
            result.passed,
            "a +5 horizontal delta should appear as magnitude 5 with the \
             direction bit clear in the serial packet (exit={:?} frames={})",
            result.exit_reason, result.frames
        );
        assert_eq!(result.exit_reason, RunExitReason::PassMarker);
    }

    #[test]
    fn capture_output_path_uses_snes_target_directory_and_crc() {
        assert_eq!(
            capture_output_path("blargg_apu_tests", "pass", 0x8C90_CEE0),
            PathBuf::from("target/snes_test_captures/blargg_apu_tests/pass_crc_8C90CEE0.png")
        );
    }

    #[test]
    fn capture_is_disabled_when_capture_flag_is_false() {
        let result = run_rom_with_capture(&pass_marker_rom(), "pass.sfc", short_config(), false);

        assert!(result.capture_path.is_none());
    }

    #[test]
    fn capture_is_written_when_capture_flag_is_true() {
        let result = run_rom_with_capture(
            &pass_marker_rom(),
            "capture enabled.sfc",
            short_config(),
            true,
        );

        let path = result.capture_path.expect("capture path");
        assert!(
            path.exists(),
            "capture should be written to {}",
            path.display()
        );
        std::fs::remove_file(path).expect("remove test capture");
    }

    #[test]
    fn capture_is_disabled_for_known_fixture_names_even_when_enabled() {
        for name in [
            "bus-byte-pass.sfc",
            "fail.sfc",
            "input-mouse-delta.sfc",
            "input-mouse-id.sfc",
            "input-mouse-left.sfc",
            "input-mouse-right.sfc",
            "input-port2-press.sfc",
            "input-script-none.sfc",
            "input-script-press.sfc",
            "input-script-release.sfc",
            "input-script-unsorted.sfc",
            "mouse-clamp.sfc",
            "mouse-example-sequence.sfc",
            "mouse-identify.sfc",
            "mouse-port2.sfc",
            "mouse-speed-cycle.sfc",
            "pad-auto-matches-serial.sfc",
            "pad-example-sequence.sfc",
            "pad-serial-order.sfc",
            "pad-strobe-semantics.sfc",
            "pad2-serial-order.sfc",
            "pass.sfc",
            "screen-crc-match.sfc",
            "screen-crc-mismatch.sfc",
            "screen-crc-probe.sfc",
            "timeout.sfc",
        ] {
            let result = run_rom_with_capture(&pass_marker_rom(), name, short_config(), true);
            assert!(result.capture_path.is_none(), "{name} should not capture");
        }
    }

    #[test]
    fn capture_is_disabled_for_timeout_fixture_even_when_enabled() {
        let result = run_rom_with_capture(&timeout_rom(), "timeout.sfc", short_config(), true);
        assert!(result.capture_path.is_none());
    }
}
