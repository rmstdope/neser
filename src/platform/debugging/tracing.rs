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
//! ```rust
//! use neser::{trace_apu, trace_cpu, trace_mapper, trace_ppu};
//!
//! let pc = 0u16;
//! let opcode = 0u8;
//! let scanline = 0u16;
//! let pixel = 0u16;
//! let cycle = 0u64;
//! let bank = 0usize;
//!
//! trace_cpu!("PC={:04X} opcode={:02X}", pc, opcode);
//! trace_ppu!("scanline={} pixel={}", scanline, pixel);
//! trace_apu!("frame_counter={}", cycle);
//! trace_mapper!("bank switch to {}", bank);
//! ```
//!
//! In release builds, these macros expand to nothing.

/// Global tracing state for debug builds.
///
/// In tests we use thread-local storage to avoid cross-test interference.
/// In non-test debug builds, this is a single shared global.
#[cfg(all(debug_assertions, not(test)))]
pub static TRACING: std::sync::OnceLock<std::sync::RwLock<Tracing>> = std::sync::OnceLock::new();

#[cfg(all(debug_assertions, test))]
thread_local! {
    static TRACING: std::cell::RefCell<Tracing> = std::cell::RefCell::new(Tracing::default());
    static MAPPER_TRACE_OUTPUT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Initialize the global tracing state. Call this once at startup.
#[cfg(all(debug_assertions, not(test)))]
pub fn init_tracing(tracing: Tracing) {
    if let Some(lock) = TRACING.get() {
        let mut guard = lock.write().unwrap_or_else(|e| e.into_inner());
        *guard = tracing;
        return;
    }

    let lock = std::sync::RwLock::new(tracing);
    if TRACING.set(lock).is_err()
        && let Some(lock) = TRACING.get()
    {
        let mut guard = lock.write().unwrap_or_else(|e| e.into_inner());
        *guard = tracing;
    }
}

#[cfg(all(debug_assertions, test))]
pub fn init_tracing(tracing: Tracing) {
    TRACING.with(|cell| {
        *cell.borrow_mut() = tracing;
    });
}

#[cfg(all(debug_assertions, test))]
pub fn clear_mapper_traces() {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().clear());
}

#[cfg(all(debug_assertions, test))]
pub fn take_mapper_traces() -> Vec<String> {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().drain(..).collect())
}

#[cfg(all(debug_assertions, test))]
pub fn emit_mapper_trace(line: String) {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().push(line));
}

#[cfg(all(debug_assertions, not(test)))]
pub fn emit_mapper_trace(line: String) {
    println!("{}", line);
}

/// Initialize the global tracing state. No-op in release builds.
#[cfg(not(debug_assertions))]
pub fn init_tracing(_tracing: Tracing) {}

/// Get the CPU tracing level. Returns 0 if tracing is not initialized.
#[cfg(all(debug_assertions, not(test)))]
pub fn cpu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).cpu)
        .unwrap_or(0)
}

#[cfg(all(debug_assertions, test))]
pub fn cpu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().cpu)
}

/// Get the CPU tracing level. Always returns 0 in release builds.
#[cfg(not(debug_assertions))]
pub fn cpu_trace_level() -> u8 {
    0
}

/// Check if CPU tracing is enabled. Returns false if tracing is not initialized.
#[cfg(all(debug_assertions, not(test)))]
pub fn is_cpu_tracing_enabled() -> bool {
    cpu_trace_level() > 0
}

#[cfg(all(debug_assertions, test))]
pub fn is_cpu_tracing_enabled() -> bool {
    cpu_trace_level() > 0
}

/// Check if CPU tracing is enabled. Always returns false in release builds.
#[cfg(not(debug_assertions))]
pub fn is_cpu_tracing_enabled() -> bool {
    false
}

/// Trace CPU operations. Only active in debug builds when CPU tracing is enabled.
///
/// # Example
/// ```rust
/// use neser::trace_cpu;
/// let pc = 0u16;
/// let a = 0u8;
/// trace_cpu!("PC={:04X} A={:02X}", pc, a);  // defaults to level 1
/// trace_cpu!(2; "detailed info");           // only prints at level 2+
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_cpu {
    ($level:literal; $($arg:tt)*) => {
        if $crate::platform::debugging::cpu_trace_level() >= $level {
            println!("[CPU] {}", format!($($arg)*));
        }
    };
    ($($arg:tt)*) => {
        if $crate::platform::debugging::cpu_trace_level() >= 1 {
            println!("[CPU] {}", format!($($arg)*));
        }
    };
}

/// Trace CPU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_cpu {
    ($level:literal; $($arg:tt)*) => {};
    ($($arg:tt)*) => {};
}

/// Get the PPU tracing level.
#[cfg(all(debug_assertions, not(test)))]
pub fn ppu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).ppu)
        .unwrap_or(0)
}

#[cfg(all(debug_assertions, test))]
pub fn ppu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().ppu)
}

#[cfg(not(debug_assertions))]
pub fn ppu_trace_level() -> u8 {
    0
}

/// Trace PPU operations. Only active in debug builds when PPU tracing is enabled.
///
/// # Example
/// ```rust
/// use neser::trace_ppu;
/// let scanline = 0u16;
/// let pixel = 0u16;
/// trace_ppu!("scanline={} pixel={}", scanline, pixel);  // defaults to level 1
/// trace_ppu!(2; "detailed info");                       // only prints at level 2+
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_ppu {
    ($level:literal; $($arg:tt)*) => {
        if $crate::platform::debugging::ppu_trace_level() >= $level {
            println!("[PPU] {}", format!($($arg)*));
        }
    };
    ($($arg:tt)*) => {
        if $crate::platform::debugging::ppu_trace_level() >= 1 {
            println!("[PPU] {}", format!($($arg)*));
        }
    };
}

/// Trace PPU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_ppu {
    ($level:literal; $($arg:tt)*) => {};
    ($($arg:tt)*) => {};
}

/// Get the APU tracing level.
#[cfg(all(debug_assertions, not(test)))]
pub fn apu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).apu)
        .unwrap_or(0)
}

#[cfg(all(debug_assertions, test))]
pub fn apu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().apu)
}

#[cfg(not(debug_assertions))]
pub fn apu_trace_level() -> u8 {
    0
}

/// Trace APU operations. Only active in debug builds when APU tracing is enabled.
///
/// # Example
/// ```rust
/// use neser::trace_apu;
/// let fc = 0u64;
/// let cycle = 0u64;
/// trace_apu!("frame_counter={} cycle={}", fc, cycle);  // defaults to level 1
/// trace_apu!(2; "detailed info");                      // only prints at level 2+
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_apu {
    ($level:literal; $($arg:tt)*) => {
        if $crate::platform::debugging::apu_trace_level() >= $level {
            println!("[APU] {}", format!($($arg)*));
        }
    };
    ($($arg:tt)*) => {
        if $crate::platform::debugging::apu_trace_level() >= 1 {
            println!("[APU] {}", format!($($arg)*));
        }
    };
}

/// Trace APU operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_apu {
    ($level:literal; $($arg:tt)*) => {};
    ($($arg:tt)*) => {};
}

/// Get the mapper tracing level.
#[cfg(all(debug_assertions, not(test)))]
pub fn mapper_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).mapper)
        .unwrap_or(0)
}

#[cfg(all(debug_assertions, test))]
pub fn mapper_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().mapper)
}

#[cfg(not(debug_assertions))]
pub fn mapper_trace_level() -> u8 {
    0
}

/// Trace mapper operations. Only active in debug builds when mapper tracing is enabled.
///
/// # Example
/// ```rust
/// use neser::trace_mapper;
/// let bank = 0usize;
/// let addr = 0u16;
/// trace_mapper!("bank switch: PRG bank {} -> ${:04X}", bank, addr);  // defaults to level 1
/// trace_mapper!(2; "detailed info");                                 // only prints at level 2+
/// ```
#[macro_export]
#[cfg(debug_assertions)]
macro_rules! trace_mapper {
    ($level:literal; $($arg:tt)*) => {
        if $crate::platform::debugging::mapper_trace_level() >= $level {
            $crate::platform::debugging::emit_mapper_trace(format!("[MAP] {}", format!($($arg)*)));
        }
    };
    ($($arg:tt)*) => {
        if $crate::platform::debugging::mapper_trace_level() >= 1 {
            $crate::platform::debugging::emit_mapper_trace(format!("[MAP] {}", format!($($arg)*)));
        }
    };
}

/// Trace mapper operations. No-op in release builds.
#[macro_export]
#[cfg(not(debug_assertions))]
macro_rules! trace_mapper {
    ($level:literal; $($arg:tt)*) => {};
    ($($arg:tt)*) => {};
}

/// Tracing configuration with per-subsystem trace levels.
///
/// Each subsystem has a trace level (0 = off, 1+ = increasing verbosity).
/// The `nestest` field remains a boolean for compatibility with nestest format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tracing {
    pub enabled: bool,
    /// CPU trace level (0 = off, 1 = basic, 2+ = verbose)
    pub cpu: u8,
    /// PPU trace level (0 = off, 1 = basic, 2+ = verbose)
    pub ppu: u8,
    /// APU trace level (0 = off, 1 = basic, 2+ = verbose)
    pub apu: u8,
    /// Mapper trace level (0 = off, 1 = basic, 2+ = verbose)
    pub mapper: u8,
    /// Enable nestest-compatible output format
    pub nestest: bool,
}

impl Tracing {
    /// Apply command-line arguments to an existing Tracing config.
    /// Only overrides values that are explicitly specified in args.
    pub fn apply_args(&mut self, args: &[String]) {
        for arg in args {
            if arg == "--trace" {
                self.enabled = true;
                self.cpu = 1;
                continue;
            }

            if arg.starts_with("--trace-") {
                self.enabled = true;

                if arg == "--trace-nestest" {
                    self.nestest = true;
                } else if let Some(rest) = arg.strip_prefix("--trace-cpu") {
                    self.cpu = Self::parse_level(rest);
                } else if let Some(rest) = arg.strip_prefix("--trace-ppu") {
                    self.ppu = Self::clamp_ppu_level(Self::parse_level(rest));
                } else if let Some(rest) = arg.strip_prefix("--trace-apu") {
                    self.apu = Self::parse_level(rest);
                } else if let Some(rest) = arg.strip_prefix("--trace-mapper") {
                    self.mapper = Self::clamp_mapper_level(Self::parse_level(rest));
                }
            }
        }
    }

    /// Parse a level from "" or "=N" suffix. Returns 1 if empty, N if "=N".
    fn parse_level(suffix: &str) -> u8 {
        if suffix.is_empty() {
            1
        } else if let Some(num_str) = suffix.strip_prefix('=') {
            num_str.parse().unwrap_or(1)
        } else {
            1
        }
    }

    pub(crate) fn clamp_mapper_level(level: u8) -> u8 {
        level.min(5)
    }

    pub(crate) fn clamp_ppu_level(level: u8) -> u8 {
        level.min(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tracing(args: &[String]) -> Tracing {
        let mut tracing = Tracing::default();
        tracing.apply_args(args);
        tracing
    }

    fn get_tracing() -> Option<Tracing> {
        Some(TRACING.with(|cell| *cell.borrow()))
    }

    #[test]
    fn tracing_defaults_to_disabled() {
        let args = vec!["neser".to_string()];
        let tracing = parse_tracing(&args);
        assert!(!tracing.enabled);
        assert_eq!(tracing.cpu, 0);
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_is_enabled_with_trace_flag() {
        let args = vec!["neser".to_string(), "--trace".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 1); // --trace enables CPU tracing at level 1
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_uses_nestest_format_when_requested() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 0);
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
        assert!(tracing.nestest);
    }

    #[test]
    fn tracing_enables_cpu_trace_with_trace_cpu_flag() {
        let args = vec!["neser".to_string(), "--trace-cpu".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 1);
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_ppu_trace_with_trace_ppu_flag() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 0);
        assert_eq!(tracing.ppu, 1);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_apu_trace_with_trace_apu_flag() {
        let args = vec!["neser".to_string(), "--trace-apu".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 0);
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 1);
        assert_eq!(tracing.mapper, 0);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_mapper_trace_with_trace_mapper_flag() {
        let args = vec!["neser".to_string(), "--trace-mapper".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 0);
        assert_eq!(tracing.ppu, 0);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 1);
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
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 1);
        assert_eq!(tracing.ppu, 1);
        assert_eq!(tracing.apu, 1);
        assert_eq!(tracing.mapper, 1);
        assert!(!tracing.nestest);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn init_tracing_sets_global_state() {
        let tracing = Tracing {
            enabled: true,
            cpu: 1,
            ppu: 0,
            apu: 0,
            mapper: 0,
            nestest: false,
        };
        init_tracing(tracing);

        // After initialization, get_tracing should return the tracing config
        let retrieved = get_tracing();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(retrieved.enabled);
        assert_eq!(retrieved.cpu, 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn init_tracing_overwrites_existing_state() {
        let tracing_on = Tracing {
            enabled: true,
            cpu: 1,
            ppu: 0,
            apu: 0,
            mapper: 0,
            nestest: false,
        };
        init_tracing(tracing_on);

        let tracing_off = Tracing::default();
        init_tracing(tracing_off);

        let retrieved = get_tracing();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(!retrieved.enabled);
        assert_eq!(retrieved.cpu, 0);
    }

    #[cfg(all(debug_assertions, test))]
    #[test]
    fn tracing_is_thread_local_in_tests() {
        init_tracing(Tracing {
            enabled: true,
            cpu: 1,
            ppu: 0,
            apu: 0,
            mapper: 0,
            nestest: false,
        });

        let main_tracing = get_tracing().unwrap();
        assert_eq!(main_tracing.cpu, 1);

        let handle = std::thread::spawn(|| {
            let other_tracing = get_tracing().unwrap();
            assert_eq!(other_tracing.cpu, 0);

            init_tracing(Tracing {
                enabled: true,
                cpu: 1,
                ppu: 0,
                apu: 0,
                mapper: 0,
                nestest: false,
            });

            let updated = get_tracing().unwrap();
            assert_eq!(updated.cpu, 1);
        });

        handle.join().unwrap();

        let main_after = get_tracing().unwrap();
        assert_eq!(main_after.cpu, 1);
    }

    #[test]
    fn tracing_parses_levels_from_args() {
        let args = vec![
            "neser".to_string(),
            "--trace-cpu=2".to_string(),
            "--trace-ppu=3".to_string(),
        ];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 2);
        assert_eq!(tracing.ppu, 3);
        assert_eq!(tracing.apu, 0);
        assert_eq!(tracing.mapper, 0);
    }

    #[test]
    fn tracing_parses_all_levels() {
        let args = vec![
            "neser".to_string(),
            "--trace-cpu=5".to_string(),
            "--trace-ppu=4".to_string(),
            "--trace-apu=3".to_string(),
            "--trace-mapper=2".to_string(),
        ];
        let tracing = parse_tracing(&args);
        assert_eq!(tracing.cpu, 5);
        assert_eq!(tracing.ppu, 4);
        assert_eq!(tracing.apu, 3);
        assert_eq!(tracing.mapper, 2);
    }

    #[test]
    fn tracing_invalid_level_defaults_to_1() {
        let args = vec!["neser".to_string(), "--trace-cpu=invalid".to_string()];
        let tracing = parse_tracing(&args);
        assert_eq!(tracing.cpu, 1);
    }

    #[test]
    fn tracing_ppu_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-ppu=9".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.ppu, 5);
    }

    #[test]
    fn tracing_mapper_level_is_capped_at_five() {
        let args = vec!["neser".to_string(), "--trace-mapper=9".to_string()];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.mapper, 5);
    }
}
