//! Autorun (record/playback) configuration parsing and validation.

use super::cli::{parse_bool_arg, parse_cli_string_arg, parse_i64_arg, parse_u32_arg};
use super::{FrontendConfig, RamInitMode};
use crate::platform::autorun::{AutorunFormat, AutorunMode};

/// Apply autorun command-line flags and validation.
///
/// `cli_ram_init_mode` is the raw value of `--ram-init-mode` captured by the
/// caller. Autorun recording/playback must be deterministic, so it forces
/// zero-initialized RAM and rejects any explicit non-zero `--ram-init-mode`.
pub(super) fn apply_args(
    cfg: &mut FrontendConfig,
    args: &[String],
    cli_ram_init_mode: Option<&str>,
) -> Result<(), String> {
    // Autorun mode flags
    let has_create_recording = args.iter().any(|arg| arg == "--create-recording");
    let has_extend_recording = args.iter().any(|arg| arg == "--extend-recording");
    let has_playback = args.iter().any(|arg| arg == "--playback");
    let has_playback_headless = args.iter().any(|arg| arg == "--playback-headless");

    if has_create_recording && has_extend_recording {
        return Err("Cannot specify both --create-recording and --extend-recording".to_string());
    }
    if (has_create_recording || has_extend_recording) && (has_playback || has_playback_headless) {
        return Err("Cannot specify both a recording flag and a playback flag".to_string());
    }

    if has_create_recording {
        cfg.autorun_mode = AutorunMode::Record;
        cfg.autorun_overwrite = true;
    } else if has_extend_recording {
        cfg.autorun_mode = AutorunMode::Record;
        cfg.autorun_extend = true;
    } else if has_playback || has_playback_headless {
        cfg.autorun_mode = AutorunMode::Playback;
        cfg.autorun_headless = has_playback_headless;
    }

    if let Some(v) = parse_i64_arg(args, "--playback-from-checkpoint")? {
        cfg.autorun_from_checkpoint = Some(v);
        // Implies playback mode if no explicit mode was set
        if cfg.autorun_mode == AutorunMode::None {
            cfg.autorun_mode = AutorunMode::Playback;
        }
    }

    if let Some(v) = parse_i64_arg(args, "--playback-headless-from-checkpoint")? {
        cfg.autorun_from_checkpoint = Some(v);
        cfg.autorun_mode = AutorunMode::Playback;
        cfg.autorun_headless = true;
    }

    if let Some(v) = parse_u32_arg(args, "--trim-checkpoints")? {
        cfg.autorun_trim_checkpoints = Some(v as usize);
    }

    if let Some(convert_autorun_requested) = parse_bool_arg(args, "--convert-autorun")? {
        cfg.autorun_convert = convert_autorun_requested;
    }

    if let Some(recalculate_autorun_requested) = parse_bool_arg(args, "--recalculate-autorun")? {
        cfg.autorun_recalculate = recalculate_autorun_requested;
    }

    if let Some(format_str) = parse_cli_string_arg(args, "--autorun-format") {
        cfg.autorun_format = match format_str.as_str() {
            "binary" => AutorunFormat::Binary,
            "json" => AutorunFormat::Json,
            other => {
                return Err(format!(
                    "Unknown autorun format '{other}': expected 'binary' or 'json'"
                ));
            }
        };
    }

    // Autorun validation
    if cfg.autorun_trim_checkpoints.is_some() && cfg.autorun_convert {
        return Err("Cannot specify both --trim-checkpoints and --convert-autorun".to_string());
    }

    if cfg.autorun_trim_checkpoints.is_some() && cfg.autorun_recalculate {
        return Err("Cannot specify both --trim-checkpoints and --recalculate-autorun".to_string());
    }

    if cfg.autorun_convert && cfg.autorun_recalculate {
        return Err("Cannot specify both --convert-autorun and --recalculate-autorun".to_string());
    }

    if cfg.autorun_recalculate && cfg.autorun_mode != AutorunMode::None {
        return Err(
            "Cannot combine --recalculate-autorun with recording/playback flags".to_string(),
        );
    }

    if cfg.autorun_recalculate && cfg.autorun_from_checkpoint.is_some() {
        return Err(
            "Cannot combine --recalculate-autorun with checkpoint playback flags".to_string(),
        );
    }

    // Autorun recording/playback must be deterministic.
    // Force zero-initialized RAM when autorun is active, and reject an explicit
    // non-zero CLI --ram-init-mode for these modes.
    if cfg.autorun_mode != AutorunMode::None || cfg.autorun_recalculate {
        if let Some(value) = cli_ram_init_mode
            && !value.eq_ignore_ascii_case("zero")
        {
            return Err("Autorun recording/playback requires --ram-init-mode zero".to_string());
        }
        cfg.ram_init_mode = RamInitMode::Zero;
    }

    Ok(())
}
