//! Tracing functionality for debugging NES emulator timing issues.
//!
//! This module provides macros and configuration for tracing different
//! emulator components (CPU, PPU, APU, mapper). The tracing system is
//! designed to:
//!
//! - Only be active in debug builds (zero overhead in release)
//! - Be configurable per-component via command-line flags
//! - Output to stdout for easy debugging
//! - Keep nestest format unchanged for compatibility
//!
//! # Usage
//!
//! Use the trace macros to emit debug output:
//!
//! ```ignore
//! trace_cpu!("PC={:04X} opcode={:02X}", pc, opcode);
//! trace_ppu!("scanline={} pixel={}", scanline, pixel);
//! trace_apu!("frame_counter={}", cycle);
//! trace_mapper!("bank switch to {}", bank);
//! ```
//!
//! In release builds, these macros expand to nothing.

/// Global tracing state for debug builds.
///
/// This static is only present in debug builds and is used by the trace macros
/// to check if tracing is enabled without passing the Tracing struct everywhere.
#[cfg(debug_assertions)]
pub static TRACING: std::sync::OnceLock<Tracing> = std::sync::OnceLock::new();

/// Initialize the global tracing state. Call this once at startup.
#[cfg(debug_assertions)]
pub fn init_tracing(tracing: Tracing) {
    let _ = TRACING.set(tracing);
}

/// Initialize the global tracing state. No-op in release builds.
#[cfg(not(debug_assertions))]
pub fn init_tracing(_tracing: Tracing) {}

/// Get the current tracing configuration.
#[cfg(debug_assertions)]
pub fn get_tracing() -> Option<&'static Tracing> {
    TRACING.get()
}

/// Check if CPU tracing is enabled. Returns false if tracing is not initialized.
#[cfg(debug_assertions)]
pub fn is_cpu_tracing_enabled() -> bool {
    TRACING.get().is_some_and(|t| t.cpu)
}

/// Check if CPU tracing is enabled. Always returns false in release builds.
#[cfg(not(debug_assertions))]
pub fn is_cpu_tracing_enabled() -> bool {
    false
}

/// Trace CPU operations. Only active in debug builds when CPU tracing is enabled.
///
/// # Example
/// ```ignore
/// trace_cpu!("PC={:04X} A={:02X}", pc, a);
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_cpu {
    ($($arg:tt)*) => {
        if let Some(tracing) = $crate::tracing::get_tracing() {
            if tracing.cpu {
                println!("[CPU] {}", format!($($arg)*));
            }
        }
    };
}

/// Trace CPU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_cpu {
    ($($arg:tt)*) => {};
}

/// Trace PPU operations. Only active in debug builds when PPU tracing is enabled.
///
/// # Example
/// ```ignore
/// trace_ppu!("scanline={} pixel={}", scanline, pixel);
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_ppu {
    ($($arg:tt)*) => {
        if let Some(tracing) = $crate::tracing::get_tracing() {
            if tracing.ppu {
                println!("[PPU] {}", format!($($arg)*));
            }
        }
    };
}

/// Trace PPU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_ppu {
    ($($arg:tt)*) => {};
}

/// Trace APU operations. Only active in debug builds when APU tracing is enabled.
///
/// # Example
/// ```ignore
/// trace_apu!("frame_counter={} cycle={}", fc, cycle);
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_apu {
    ($($arg:tt)*) => {
        if let Some(tracing) = $crate::tracing::get_tracing() {
            if tracing.apu {
                println!("[APU] {}", format!($($arg)*));
            }
        }
    };
}

/// Trace APU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_apu {
    ($($arg:tt)*) => {};
}

/// Trace mapper operations. Only active in debug builds when mapper tracing is enabled.
///
/// # Example
/// ```ignore
/// trace_mapper!("bank switch: PRG bank {} -> ${:04X}", bank, addr);
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_mapper {
    ($($arg:tt)*) => {
        if let Some(tracing) = $crate::tracing::get_tracing() {
            if tracing.mapper {
                println!("[MAPPER] {}", format!($($arg)*));
            }
        }
    };
}

/// Trace mapper operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_mapper {
    ($($arg:tt)*) => {};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tracing {
    pub enabled: bool,
    pub cpu: bool,
    pub ppu: bool,
    pub apu: bool,
    pub mapper: bool,
    pub nestest: bool,
}

impl Tracing {
    pub fn from_args(args: &[String]) -> Self {
        let mut tracing = Tracing::default();

        for arg in args {
            if arg == "--trace" {
                tracing.enabled = true;
                tracing.cpu = true;
                continue;
            }

            if arg.starts_with("--trace-") {
                tracing.enabled = true;
                match arg.as_str() {
                    "--trace-nestest" => tracing.nestest = true,
                    "--trace-cpu" => tracing.cpu = true,
                    "--trace-ppu" => tracing.ppu = true,
                    "--trace-apu" => tracing.apu = true,
                    "--trace-mapper" => tracing.mapper = true,
                    _ => {}
                }
            }
        }

        tracing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_defaults_to_disabled() {
        let args = vec!["neser".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(!tracing.enabled);
        assert!(!tracing.cpu);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_is_enabled_with_trace_flag() {
        let args = vec!["neser".to_string(), "--trace".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(tracing.cpu); // --trace enables CPU tracing by default
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_uses_nestest_format_when_requested() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.cpu);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.mapper);
        assert!(tracing.nestest);
    }

    #[test]
    fn tracing_enables_cpu_trace_with_trace_cpu_flag() {
        let args = vec!["neser".to_string(), "--trace-cpu".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(tracing.cpu);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_ppu_trace_with_trace_ppu_flag() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.cpu);
        assert!(tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_apu_trace_with_trace_apu_flag() {
        let args = vec!["neser".to_string(), "--trace-apu".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.cpu);
        assert!(!tracing.ppu);
        assert!(tracing.apu);
        assert!(!tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_mapper_trace_with_trace_mapper_flag() {
        let args = vec!["neser".to_string(), "--trace-mapper".to_string()];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.cpu);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_multiple_flags_can_be_combined() {
        let args = vec![
            "neser".to_string(),
            "--trace-cpu".to_string(),
            "--trace-ppu".to_string(),
            "--trace-apu".to_string(),
            "--trace-mapper".to_string(),
        ];
        let tracing = Tracing::from_args(&args);
        assert!(tracing.enabled);
        assert!(tracing.cpu);
        assert!(tracing.ppu);
        assert!(tracing.apu);
        assert!(tracing.mapper);
        assert!(!tracing.nestest);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn init_tracing_sets_global_state() {
        let tracing = Tracing {
            enabled: true,
            cpu: true,
            ppu: false,
            apu: false,
            mapper: false,
            nestest: false,
        };
        init_tracing(tracing);

        // After initialization, get_tracing should return the tracing config
        let retrieved = get_tracing();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(retrieved.enabled);
        assert!(retrieved.cpu);
    }
}
