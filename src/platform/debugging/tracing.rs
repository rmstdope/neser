//! Tracing functionality for debugging NES emulator timing issues.
//!
//! This module provides macros and configuration for tracing different
//! emulator components (CPU, PPU, APU, mapper). The tracing system is
//! designed to:
//!
//! - Be configurable per-component via command-line flags
//! - Output to stdout for easy debugging
//! - Keep nestest format unchanged for compatibility
//! - Have minimal overhead when disabled (just an integer check)
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
//! When tracing is disabled (default), these expand to simple checks that
//! are easily optimized away by the compiler.

/// Global tracing state.
///
/// In tests we use thread-local storage to avoid cross-test interference.
/// In production builds, this is a single shared global.
#[cfg(not(test))]
pub static TRACING: std::sync::OnceLock<std::sync::RwLock<Tracing>> = std::sync::OnceLock::new();

#[cfg(test)]
thread_local! {
    static TRACING: std::cell::RefCell<Tracing> = std::cell::RefCell::new(Tracing::default());
    static MAPPER_TRACE_OUTPUT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Initialize the global tracing state. Call this once at startup.
#[cfg(not(test))]
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

#[cfg(test)]
pub fn init_tracing(tracing: Tracing) {
    TRACING.with(|cell| {
        *cell.borrow_mut() = tracing;
    });
}

#[cfg(test)]
pub fn clear_mapper_traces() {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().clear());
}

#[cfg(test)]
pub fn take_mapper_traces() -> Vec<String> {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().drain(..).collect())
}

#[cfg(test)]
pub fn emit_mapper_trace(line: String) {
    MAPPER_TRACE_OUTPUT.with(|cell| cell.borrow_mut().push(line));
}

#[cfg(not(test))]
pub fn emit_mapper_trace(line: String) {
    println!("{}", line);
}

/// Get the CPU tracing level. Returns 0 if tracing is not initialized.
#[cfg(not(test))]
pub fn cpu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).cpu)
        .unwrap_or(0)
}

#[cfg(test)]
pub fn cpu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().cpu)
}

/// Check if CPU tracing is enabled. Returns false if tracing is not initialized.
#[cfg(not(test))]
pub fn is_cpu_tracing_enabled() -> bool {
    cpu_trace_level() > 0
}

#[cfg(test)]
pub fn is_cpu_tracing_enabled() -> bool {
    cpu_trace_level() > 0
}

/// Trace CPU operations. Only active when CPU tracing is enabled.
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

/// Get the PPU tracing level.
#[cfg(not(test))]
pub fn ppu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).ppu)
        .unwrap_or(0)
}

#[cfg(test)]
pub fn ppu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().ppu)
}

/// Trace PPU operations. Only active when PPU tracing is enabled.
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

/// Get the APU tracing level.
#[cfg(not(test))]
pub fn apu_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).apu)
        .unwrap_or(0)
}

#[cfg(test)]
pub fn apu_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().apu)
}

/// Trace APU operations. Only active when APU tracing is enabled.
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

/// Get the mapper tracing level.
#[cfg(not(test))]
pub fn mapper_trace_level() -> u8 {
    TRACING
        .get()
        .map(|lock| lock.read().unwrap_or_else(|e| e.into_inner()).mapper)
        .unwrap_or(0)
}

#[cfg(test)]
pub fn mapper_trace_level() -> u8 {
    TRACING.with(|cell| cell.borrow().mapper)
}

/// Trace mapper operations. Only active when mapper tracing is enabled.
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

/// Whether a clock-stamped trace line for `master_clock` falls inside the configured
/// `--trace-from`/`--trace-to` window.
///
/// Clock-gating exists because a full bus trace of a real game runs to millions of lines;
/// narrowing both emulators to the same master-clock window keeps two logs comparable and
/// small enough to diff (see the SNES `$4210` skew investigation, #3050). An unset bound is
/// unbounded on that side, so the default configuration traces everything.
#[cfg(not(test))]
pub fn trace_clock_in_window(master_clock: u64) -> bool {
    TRACING
        .get()
        .map(|lock| {
            lock.read()
                .unwrap_or_else(|e| e.into_inner())
                .window(master_clock)
        })
        .unwrap_or(true)
}

#[cfg(test)]
pub fn trace_clock_in_window(master_clock: u64) -> bool {
    TRACING.with(|cell| cell.borrow().window(master_clock))
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
    /// Lowest master clock a clock-stamped trace line is emitted for (`None` = unbounded).
    pub clock_from: Option<u64>,
    /// Highest master clock a clock-stamped trace line is emitted for (`None` = unbounded).
    pub clock_to: Option<u64>,
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
                } else if let Some(rest) = arg.strip_prefix("--trace-from") {
                    self.clock_from = Self::parse_clock(rest);
                } else if let Some(rest) = arg.strip_prefix("--trace-to") {
                    self.clock_to = Self::parse_clock(rest);
                }
            }
        }
    }

    /// Whether `master_clock` falls inside this config's clock window.
    fn window(&self, master_clock: u64) -> bool {
        self.clock_from.is_none_or(|from| master_clock >= from)
            && self.clock_to.is_none_or(|to| master_clock <= to)
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

    /// Parse a master-clock bound from an "=N" suffix. Returns `None` (unbounded) for a
    /// missing or unparseable value, so a typo widens the window rather than silently
    /// suppressing every trace line.
    fn parse_clock(suffix: &str) -> Option<u64> {
        suffix.strip_prefix('=')?.parse().ok()
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
    fn tracing_clock_window_defaults_to_unbounded() {
        let tracing = parse_tracing(&["neser".to_string(), "--trace-cpu=2".to_string()]);
        assert_eq!(tracing.clock_from, None);
        assert_eq!(tracing.clock_to, None);
    }

    #[test]
    fn trace_from_and_to_flags_parse_a_clock_window() {
        let args = vec![
            "neser".to_string(),
            "--trace-cpu=2".to_string(),
            "--trace-from=1000".to_string(),
            "--trace-to=2000".to_string(),
        ];
        let tracing = parse_tracing(&args);
        assert!(tracing.enabled);
        assert_eq!(tracing.cpu, 2);
        assert_eq!(tracing.clock_from, Some(1000));
        assert_eq!(tracing.clock_to, Some(2000));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn clock_window_gates_clocks_outside_the_range() {
        init_tracing(Tracing {
            enabled: true,
            cpu: 2,
            clock_from: Some(100),
            clock_to: Some(200),
            ..Tracing::default()
        });
        assert!(!trace_clock_in_window(99));
        assert!(trace_clock_in_window(100));
        assert!(trace_clock_in_window(200));
        assert!(!trace_clock_in_window(201));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn an_unset_clock_window_bound_is_unbounded_on_that_side() {
        init_tracing(Tracing {
            enabled: true,
            cpu: 2,
            clock_from: Some(100),
            ..Tracing::default()
        });
        assert!(!trace_clock_in_window(99));
        assert!(trace_clock_in_window(u64::MAX));

        init_tracing(Tracing {
            enabled: true,
            cpu: 2,
            clock_to: Some(200),
            ..Tracing::default()
        });
        assert!(trace_clock_in_window(0));
        assert!(!trace_clock_in_window(201));
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
            ..Tracing::default()
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
            ..Tracing::default()
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
            ..Tracing::default()
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
                ..Tracing::default()
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
