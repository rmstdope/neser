//! Video and window configuration parsing (VSync, fullscreen, display, window size).

use super::FrontendConfig;
use super::cli::{has_negation_flag, parse_bool, parse_bool_arg, parse_u32_arg};

/// Apply video/window command-line flags.
pub(super) fn apply_args(cfg: &mut FrontendConfig, args: &[String]) -> Result<(), String> {
    // VSync: --vsync true/false, --no-vsync, --disable-vsync
    if let Some(vsync) = parse_bool_arg(args, "--vsync")? {
        cfg.vsync_enabled = vsync;
    }
    if has_negation_flag(args, &["--no-vsync", "--disable-vsync"]) {
        cfg.vsync_enabled = false;
    }

    // Fullscreen (value-based)
    if let Some(fullscreen) = parse_bool_arg(args, "--fullscreen")? {
        cfg.fullscreen = fullscreen;
    }

    // Window height
    if let Some(height) = parse_u32_arg(args, "--window-height")? {
        cfg.window_height = height;
    }
    Ok(())
}

/// Apply a video/window config-file key/value. Returns `true` if the key was handled.
pub(super) fn apply_config_value(
    cfg: &mut FrontendConfig,
    key: &str,
    value: &str,
) -> Result<bool, String> {
    match key {
        "vsync" => {
            if let Ok(b) = parse_bool(value) {
                cfg.vsync_enabled = b;
            }
        }
        "fullscreen" => {
            if let Ok(b) = parse_bool(value) {
                cfg.fullscreen = b;
            }
        }
        "display" => {
            if let Ok(d) = value.parse::<i32>()
                && d >= 0
            {
                cfg.fullscreen_display = Some(d);
            }
        }
        "window_height" => {
            if let Ok(s) = value.parse::<u32>() {
                cfg.window_height = s;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
