//! Headless frame-capture configuration parsing and validation.
//!
//! `--headless` is a run mode rather than a persistent setting, so it has no
//! config-file key — the same treatment `--tui` and the autorun flags get.

use super::cli::{parse_cli_string_arg, parse_u32_arg};
use super::{DEFAULT_CAPTURE_FRAMES, FrontendConfig, HeadlessCapture, RamInitMode};
use crate::platform::autorun::AutorunMode;
use std::path::PathBuf;

/// Apply the `--headless`, `--frames` and `--output` flags.
///
/// Must run after [`super::autorun::apply_args`], which is what populates the
/// `autorun_mode` this checks against, and it needs the raw `--ram-init-mode`
/// value so an explicitly non-deterministic capture can be rejected.
pub(super) fn apply_args(
    cfg: &mut FrontendConfig,
    args: &[String],
    cli_ram_init_mode: Option<&str>,
) -> Result<(), String> {
    let headless = args.iter().any(|arg| arg == "--headless");
    let frames = parse_u32_arg(args, "--frames")?;
    let output = parse_cli_string_arg(args, "--output");

    if !headless {
        // Accepting these silently would let a mistyped capture command open a
        // normal window instead of failing.
        if frames.is_some() {
            return Err("--frames requires --headless".to_string());
        }
        if output.is_some() {
            return Err("--output requires --headless".to_string());
        }
        return Ok(());
    }

    if cfg.autorun_mode != AutorunMode::None {
        return Err("Cannot combine --headless with autorun recording/playback flags".to_string());
    }
    if args.iter().any(|arg| arg == "--tui") {
        return Err("Cannot combine --headless with --tui".to_string());
    }

    let frames = frames.unwrap_or(DEFAULT_CAPTURE_FRAMES);
    if frames == 0 {
        return Err("--frames must be at least 1".to_string());
    }

    let output = output.ok_or_else(|| "--headless requires --output <path>".to_string())?;
    if output.trim().is_empty() {
        return Err("--output needs a non-empty path".to_string());
    }

    // Captures must be reproducible, so force zero-initialized RAM and reject
    // an explicit non-zero mode -- the same rule autorun applies.
    if let Some(value) = cli_ram_init_mode
        && !value.eq_ignore_ascii_case("zero")
    {
        return Err("Headless capture requires --ram-init-mode zero".to_string());
    }
    cfg.ram_init_mode = RamInitMode::Zero;

    cfg.headless_capture = Some(HeadlessCapture {
        frames,
        output: PathBuf::from(output),
    });

    Ok(())
}

/// Reject `--headless` without a ROM path.
///
/// Separate from [`apply_args`] because the positional ROM argument is parsed
/// later, by `Config::apply_args`.
pub(crate) fn validate_rom_path(cfg: &FrontendConfig) -> Result<(), String> {
    if cfg.headless_capture.is_some() && cfg.rom_path.is_none() {
        return Err("--headless requires a ROM path".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::platform::config::test_support::{config_new, parse_config};
    use crate::platform::config::{DEFAULT_CAPTURE_FRAMES, HeadlessCapture, RamInitMode};
    use std::path::PathBuf;

    fn args(items: &[&str]) -> Vec<String> {
        std::iter::once("neser")
            .chain(items.iter().copied())
            .map(str::to_string)
            .collect()
    }

    fn error_for(items: &[&str]) -> String {
        match config_new(args(items)) {
            Ok(_) => panic!("expected {items:?} to be rejected"),
            Err(error) => error,
        }
    }

    // --- accepted forms ---

    #[test]
    fn headless_defaults_to_sixty_frames() {
        let config = parse_config(args(&["--headless", "--output", "shot.png", "game.nes"]));

        assert_eq!(
            config.frontend.headless_capture,
            Some(HeadlessCapture {
                frames: DEFAULT_CAPTURE_FRAMES,
                output: PathBuf::from("shot.png"),
            })
        );
    }

    #[test]
    fn headless_accepts_an_explicit_frame_count() {
        let config = parse_config(args(&[
            "--headless",
            "--frames",
            "30",
            "--output",
            "shot.png",
            "game.nes",
        ]));

        let capture = config
            .frontend
            .headless_capture
            .expect("capture should be configured");
        assert_eq!(capture.frames, 30);
        assert_eq!(capture.output, PathBuf::from("shot.png"));
    }

    #[test]
    fn headless_accepts_the_equals_form() {
        let config = parse_config(args(&[
            "--headless",
            "--frames=30",
            "--output=shot.png",
            "game.nes",
        ]));

        let capture = config
            .frontend
            .headless_capture
            .expect("capture should be configured");
        assert_eq!(capture.frames, 30);
        assert_eq!(capture.output, PathBuf::from("shot.png"));
    }

    #[test]
    fn headless_still_finds_the_positional_rom_after_valued_flags() {
        // `--frames`/`--output` must be declared with `has_value: true` or the
        // ROM-path scanner would treat their values as the positional argument.
        let config = parse_config(args(&[
            "--headless",
            "--frames",
            "30",
            "--output",
            "shot.png",
            "game.nes",
        ]));

        assert_eq!(config.frontend.rom_path.as_deref(), Some("game.nes"));
    }

    #[test]
    fn without_headless_no_capture_is_configured() {
        let config = parse_config(args(&["game.nes"]));

        assert_eq!(config.frontend.headless_capture, None);
    }

    // --- determinism ---

    #[test]
    fn headless_forces_zero_ram_init() {
        let config = parse_config(args(&["--headless", "--output", "shot.png", "game.nes"]));

        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn headless_accepts_an_explicit_zero_ram_init() {
        let config = parse_config(args(&[
            "--headless",
            "--ram-init-mode",
            "zero",
            "--output",
            "shot.png",
            "game.nes",
        ]));

        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    /// A config file setting a seeded RAM init mode, plus `args`.
    ///
    /// Deliberately `seeded-random:12345` rather than `random`: the non-wasm
    /// default is already `Random`, so a control asserting "not Zero" would
    /// pass even if the config file were never read at all. A seed no default
    /// produces makes both tests below prove the file was actually applied.
    fn with_seeded_ram_init_config(items: &[&str]) -> (tempfile::NamedTempFile, Vec<String>) {
        use std::io::Write as _;

        let mut file = tempfile::NamedTempFile::new().expect("create config file");
        writeln!(file, "ram_init_mode=seeded-random:12345").expect("write config");
        let path = file.path().to_string_lossy().into_owned();

        let mut all = vec!["--config".to_string(), path];
        all.extend(items.iter().map(|item| item.to_string()));
        let all = std::iter::once("neser".to_string()).chain(all).collect();

        (file, all)
    }

    #[test]
    fn headless_overrides_a_config_file_ram_init_mode() {
        // Given a config file asking for non-deterministic RAM
        let (_file, args) =
            with_seeded_ram_init_config(&["--headless", "--output", "shot.png", "game.nes"]);

        // When a capture is configured
        let config = parse_config(args);

        // Then the capture still gets zero RAM. Only an explicit
        // --ram-init-mode is an error; a persistent config file is silently
        // overridden, because otherwise anyone with a non-zero mode in their
        // config could never capture at all.
        assert_eq!(config.frontend.ram_init_mode, RamInitMode::Zero);
    }

    #[test]
    fn a_config_file_ram_init_mode_survives_without_headless() {
        // Control for the test above: without --headless the same file keeps
        // its value, so that assertion is about the override winning and not
        // about the config file being ignored.
        let (_file, args) = with_seeded_ram_init_config(&["game.nes"]);

        let config = parse_config(args);

        assert_eq!(
            config.frontend.ram_init_mode,
            RamInitMode::SeededRandom(12345)
        );
    }

    #[test]
    fn headless_rejects_a_non_zero_ram_init() {
        let error = error_for(&[
            "--headless",
            "--ram-init-mode",
            "random",
            "--output",
            "shot.png",
            "game.nes",
        ]);

        assert!(
            error.contains("--ram-init-mode zero"),
            "expected the required mode in {error:?}"
        );
    }

    // --- rejected forms ---

    #[test]
    fn headless_requires_an_output_path() {
        let error = error_for(&["--headless", "game.nes"]);

        assert!(error.contains("--output"), "expected --output in {error:?}");
    }

    #[test]
    fn headless_requires_a_rom_path() {
        let error = error_for(&["--headless", "--output", "shot.png"]);

        assert!(
            error.contains("ROM"),
            "expected the missing ROM to be named in {error:?}"
        );
    }

    #[test]
    fn headless_rejects_an_empty_output_path() {
        // Left to reach the writer, an empty path fails with a bare I/O error
        // that says nothing about which flag was wrong.
        let error = error_for(&["--headless", "--output", "", "game.nes"]);

        assert!(error.contains("--output"), "expected --output in {error:?}");
    }

    #[test]
    fn the_capture_flags_are_documented_in_help() {
        // These are run-mode flags with no config-file key, so `--help` is the
        // only place they are documented.
        let help = crate::platform::config::cli::help_text();

        for flag in ["--headless", "--frames", "--output"] {
            assert!(help.contains(flag), "expected {flag} in the help text");
        }
    }

    #[test]
    fn headless_rejects_a_zero_frame_count() {
        let error = error_for(&[
            "--headless",
            "--frames",
            "0",
            "--output",
            "shot.png",
            "game.nes",
        ]);

        assert!(error.contains("--frames"), "expected --frames in {error:?}");
    }

    #[test]
    fn frames_without_headless_is_rejected() {
        // Silently ignoring these would let a typo'd capture command launch a
        // normal windowed session instead of failing.
        let error = error_for(&["--frames", "30", "game.nes"]);

        assert!(
            error.contains("--frames") && error.contains("--headless"),
            "expected both flags named in {error:?}"
        );
    }

    #[test]
    fn output_without_headless_is_rejected() {
        let error = error_for(&["--output", "shot.png", "game.nes"]);

        assert!(
            error.contains("--output") && error.contains("--headless"),
            "expected both flags named in {error:?}"
        );
    }

    #[test]
    fn headless_cannot_be_combined_with_autorun_playback() {
        let error = error_for(&[
            "--headless",
            "--playback",
            "--output",
            "shot.png",
            "game.nes",
        ]);

        assert!(
            error.contains("--headless"),
            "expected --headless in {error:?}"
        );
    }

    #[test]
    fn headless_cannot_be_combined_with_autorun_recording() {
        let error = error_for(&[
            "--headless",
            "--create-recording",
            "--output",
            "shot.png",
            "game.nes",
        ]);

        assert!(
            error.contains("--headless"),
            "expected --headless in {error:?}"
        );
    }

    #[test]
    fn headless_cannot_be_combined_with_tui() {
        let error = error_for(&["--headless", "--tui", "--output", "shot.png", "game.nes"]);

        // Which message wins depends on the build: without the `tui` feature,
        // `--tui` is rejected earlier for needing that feature, so the conflict
        // check never runs. Either way the combination must be refused rather
        // than silently picking one mode.
        if cfg!(feature = "tui") {
            assert!(
                error.contains("--headless") && error.contains("--tui"),
                "expected both flags named in {error:?}"
            );
        } else {
            assert!(
                error.contains("--tui"),
                "expected --tui to be rejected in {error:?}"
            );
        }
    }
}
