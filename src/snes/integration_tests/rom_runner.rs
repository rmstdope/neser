use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RunConfig {
    pub max_ticks: u64,
    pub max_frames: u32,
    /// If set, a soft CPU reset is triggered every time the PC reaches this address. Some
    /// blargg SPC/APU ROMs (e.g. `timer_at_power_reset.smc`) signal "please reset now" by
    /// jumping into zeroed-out low WRAM ($0000), which the real test rig/hardware answers
    /// with a physical reset button press; this models that handshake for automated runs.
    pub reset_on_pc_trap: Option<u16>,
}

impl RunConfig {
    pub(crate) const fn new(max_ticks: u64, max_frames: u32) -> Self {
        Self {
            max_ticks,
            max_frames,
            reset_on_pc_trap: None,
        }
    }

    /// Enables the reset-on-PC-trap handshake (see [`RunConfig::reset_on_pc_trap`]).
    pub(crate) const fn with_reset_on_pc_trap(mut self, trap_pc: u16) -> Self {
        self.reset_on_pc_trap = Some(trap_pc);
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
    let mut snes = Snes::new(AppContext::default());
    snes.load_rom(rom, name)
        .unwrap_or_else(|err| panic!("failed to load SNES runner ROM {name}: {err}"));

    let mut ticks = 0u64;
    let mut frames = 0u32;
    let mut resets_triggered = 0u32;
    const MAX_AUTO_RESETS: u32 = 16;

    loop {
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
        RunOracle::Marker => {
            if marker[..4] == MARKER_MAGIC {
                match (marker[4], pc) {
                    (PASS_STATUS, PASS_IDLE_PC) => Some((RunExitReason::PassMarker, true)),
                    (FAIL_STATUS, FAIL_IDLE_PC) => Some((RunExitReason::FailMarker, false)),
                    _ => None,
                }
            } else {
                None
            }
        }
        RunOracle::BusByte {
            addr,
            pass_value,
            fail_value,
        } => {
            let value = snes.read_bus_for_debugger_for_tests(addr).unwrap_or(0);
            if value == pass_value && pc == PASS_IDLE_PC {
                Some((RunExitReason::PassMarker, true))
            } else if value == fail_value && pc == FAIL_IDLE_PC {
                Some((RunExitReason::FailMarker, false))
            } else {
                None
            }
        }
        RunOracle::ScreenCrc {
            frames: target_frames,
            expected_crc,
        } => {
            if frames >= target_frames {
                let actual_crc = snes.screen_crc32();
                Some((RunExitReason::ScreenCrcFrame, actual_crc == expected_crc))
            } else {
                None
            }
        }
    }
}

fn read_marker(snes: &Snes) -> [u8; 5] {
    let mut marker = [0; 5];
    for (offset, byte) in marker.iter_mut().enumerate() {
        *byte = snes
            .read_bus_for_debugger_for_tests(MARKER_ADDR + offset as u32)
            .unwrap_or(0);
    }
    marker
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
    let screen_crc32 = snes.screen_crc32();
    let capture_path = maybe_write_capture_png(snes, name, suite, screen_crc32, capture_screen);

    RunResult {
        passed,
        exit_reason,
        ticks,
        frames,
        pc,
        marker,
        screen_crc32,
        capture_path,
    }
}

fn maybe_write_capture_png(
    snes: &Snes,
    name: &str,
    suite: &str,
    crc: u32,
    capture_screen: bool,
) -> Option<PathBuf> {
    if !capture_screen || capture_is_disabled_for_fixture(name) {
        return None;
    }

    let path = capture_output_path(suite, &capture_stem(name), crc);
    let rgb = snes.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(
        &path,
        &rgb,
        snes.screen_width(),
        snes.screen_height(),
    );
    Some(path)
}

fn capture_is_disabled_for_fixture(name: &str) -> bool {
    matches!(
        Path::new(name).file_stem().and_then(|stem| stem.to_str()),
        Some("bus-byte-pass")
            | Some("fail")
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
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
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

    fn write_idle_loop(rom: &mut [u8], pc: u16) {
        let mut cursor = usize::from(pc - 0x8000);
        emit_jmp_abs(rom, &mut cursor, pc);
    }

    fn short_config() -> RunConfig {
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
