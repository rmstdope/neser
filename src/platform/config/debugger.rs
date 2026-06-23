//! Debugger, tracing, and breakpoint configuration parsing.

use super::FrontendConfig;
use super::cli::{parse_bool, parse_bool_arg, parse_cli_string_arg, parse_f32_arg};
use crate::platform::debugging::Tracing;
use crate::platform::debugging::breakpoints::BreakpointKind;

/// Apply debugger/tracing/breakpoint command-line flags.
pub(super) fn apply_args(cfg: &mut FrontendConfig, args: &[String]) -> Result<(), String> {
    // Debugger: --debugger true/false
    if let Some(debugger) = parse_bool_arg(args, "--debugger")? {
        cfg.debugger_enabled = debugger;
    }

    // Load state: --load-state true/false
    if let Some(load_state) = parse_bool_arg(args, "--load-state")? {
        cfg.load_state = load_state;
    }

    // Tracing (merge with existing config file values)
    cfg.tracing.apply_args(args);

    // Debugger alpha
    if let Some(alpha) = parse_f32_arg(args, "--debugger-alpha")? {
        cfg.debugger_alpha = alpha.clamp(0.1, 1.0);
    }

    // Breakpoints from --breakpoint flag (comma-separated list)
    if let Some(value) = parse_cli_string_arg(args, "--breakpoint") {
        cfg.breakpoints =
            parse_breakpoint_list(&value).map_err(|e| format!("--breakpoint: {e}"))?;
    }
    Ok(())
}

/// Apply a debugger/tracing config-file key/value. Returns `true` if handled.
pub(super) fn apply_config_value(
    cfg: &mut FrontendConfig,
    key: &str,
    value: &str,
) -> Result<bool, String> {
    match key {
        "debugger" => {
            if let Ok(b) = parse_bool(value) {
                cfg.debugger_enabled = b;
            }
        }
        "load_state" => {
            if let Ok(b) = parse_bool(value) {
                cfg.load_state = b;
            }
        }
        "debugger_alpha" => {
            if let Ok(v) = value.parse::<f32>() {
                cfg.debugger_alpha = v.clamp(0.1, 1.0);
            }
        }
        "trace_cpu" => {
            if let Ok(level) = value.parse::<u8>() {
                cfg.tracing.cpu = level;
                if level > 0 {
                    cfg.tracing.enabled = true;
                }
            }
        }
        "trace_ppu" => {
            if let Ok(level) = value.parse::<u8>() {
                cfg.tracing.ppu = Tracing::clamp_ppu_level(level);
                if level > 0 {
                    cfg.tracing.enabled = true;
                }
            }
        }
        "trace_apu" => {
            if let Ok(level) = value.parse::<u8>() {
                cfg.tracing.apu = level;
                if level > 0 {
                    cfg.tracing.enabled = true;
                }
            }
        }
        "trace_mapper" => {
            if let Ok(level) = value.parse::<u8>() {
                cfg.tracing.mapper = Tracing::clamp_mapper_level(level);
                if level > 0 {
                    cfg.tracing.enabled = true;
                }
            }
        }
        "trace_nestest" => {
            if let Ok(b) = parse_bool(value) {
                cfg.tracing.nestest = b;
                if b {
                    cfg.tracing.enabled = true;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Parse a 16-bit hex address (with or without `0x` prefix).
fn parse_hex_addr(s: &str) -> Option<u16> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).ok()
}

/// Parse a comma-separated breakpoint specification string into a list of [`BreakpointKind`].
///
/// Each entry is in the format `type=value`:
/// - `pc=ADDR` — PC breakpoint (hex address, e.g. `C000` or `0xC000`)
/// - `cycle=N` — Cycle breakpoint (decimal number)
/// - `frame=N` — Frame breakpoint (decimal number)
/// - `write=ADDR` — Write-address breakpoint (hex address)
///
/// Returns an error string if any entry is unrecognised or malformed.
fn parse_breakpoint_list(spec: &str) -> Result<Vec<BreakpointKind>, String> {
    spec.split(',')
        .map(|entry| {
            let entry = entry.trim();
            let (kind, value) = entry
                .split_once('=')
                .ok_or_else(|| format!("invalid breakpoint '{entry}': expected format type=value (e.g. pc=C000, cycle=100, frame=60, write=2006)"))?;
            match kind.trim() {
                "pc" => parse_hex_addr(value)
                    .map(BreakpointKind::Pc)
                    .ok_or_else(|| format!("invalid breakpoint address '{value}': expected a hex address (e.g. C000)")),
                "cycle" => value
                    .trim()
                    .parse::<u64>()
                    .map(BreakpointKind::Cycle)
                    .map_err(|_| format!("invalid breakpoint cycle '{value}': expected a decimal number")),
                "frame" => value
                    .trim()
                    .parse::<u64>()
                    .map(BreakpointKind::Frame)
                    .map_err(|_| format!("invalid breakpoint frame '{value}': expected a decimal number")),
                "write" => parse_hex_addr(value)
                    .map(BreakpointKind::WriteAddress)
                    .ok_or_else(|| format!("invalid breakpoint address '{value}': expected a hex address (e.g. 2006)")),
                other => Err(format!(
                    "invalid breakpoint type '{other}': expected pc, cycle, frame, or write"
                )),
            }
        })
        .collect()
}
