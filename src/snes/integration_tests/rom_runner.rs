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
pub(crate) struct RunConfig {
    pub max_ticks: u64,
    pub max_frames: u32,
}

impl RunConfig {
    pub(crate) const fn new(max_ticks: u64, max_frames: u32) -> Self {
        Self {
            max_ticks,
            max_frames,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunExitReason {
    PassMarker,
    FailMarker,
    TickLimit,
    FrameLimit,
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
    run_rom_with_capture(
        rom,
        name,
        config,
        std::env::var_os("NESER_CAPTURE_SCREEN").is_some(),
    )
}

fn run_rom_with_capture(
    rom: &[u8],
    name: &str,
    config: RunConfig,
    capture_screen: bool,
) -> RunResult {
    let mut snes = Snes::new(AppContext::default());
    snes.load_rom(rom, name)
        .unwrap_or_else(|err| panic!("failed to load SNES runner ROM {name}: {err}"));

    let mut ticks = 0u64;
    let mut frames = 0u32;

    loop {
        let step_ticks = u64::from(snes.run_tick());
        ticks = ticks.saturating_add(step_ticks);

        if snes.is_ready_to_render() {
            frames = frames.saturating_add(1);
            snes.clear_ready_to_render();
        }

        let pc = snes.cpu_pc_for_tests().unwrap_or(0);
        let marker = read_marker(&snes);
        if marker[..4] == MARKER_MAGIC {
            match (marker[4], pc) {
                (PASS_STATUS, PASS_IDLE_PC) => {
                    return finish_result(
                        &snes,
                        name,
                        RunExitReason::PassMarker,
                        true,
                        ticks,
                        frames,
                        pc,
                        marker,
                        capture_screen,
                    );
                }
                (FAIL_STATUS, FAIL_IDLE_PC) => {
                    return finish_result(
                        &snes,
                        name,
                        RunExitReason::FailMarker,
                        false,
                        ticks,
                        frames,
                        pc,
                        marker,
                        capture_screen,
                    );
                }
                _ => {}
            }
        }

        if config.max_frames != 0 && frames >= config.max_frames {
            return finish_result(
                &snes,
                name,
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
    exit_reason: RunExitReason,
    passed: bool,
    ticks: u64,
    frames: u32,
    pc: u16,
    marker: [u8; 5],
    capture_screen: bool,
) -> RunResult {
    let screen_crc32 = snes.screen_crc32();
    let capture_path = maybe_write_capture_png(snes, name, screen_crc32, capture_screen);

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
    crc: u32,
    capture_screen: bool,
) -> Option<PathBuf> {
    if !capture_screen {
        return None;
    }

    let path = capture_output_path(&capture_stem(name), crc);
    let rgb = snes.screen_snapshot();
    crate::platform::png_utils::write_rgb_png(
        &path,
        &rgb,
        snes.screen_width(),
        snes.screen_height(),
    );
    Some(path)
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

pub(crate) fn capture_output_path(stem: &str, crc: u32) -> PathBuf {
    PathBuf::from("target/snes_test_captures").join(format!("{stem}_crc_{crc:08X}.png"))
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
    fn capture_output_path_uses_snes_target_directory_and_crc() {
        assert_eq!(
            capture_output_path("pass", 0x8C90_CEE0),
            PathBuf::from("target/snes_test_captures/pass_crc_8C90CEE0.png")
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
}
