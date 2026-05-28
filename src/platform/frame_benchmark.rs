//! Shared frame benchmark statistics helpers.

use std::time::Duration;

/// Summary statistics for a frame-timing benchmark run.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameTimingStats {
    /// Number of measured frames.
    pub frames: usize,
    /// Total measured duration.
    pub total: Duration,
    /// Mean frame time in milliseconds.
    pub average_ms: f64,
    /// Median frame time in milliseconds.
    pub p50_ms: f64,
    /// 95th percentile frame time in milliseconds.
    pub p95_ms: f64,
    /// Slowest measured frame in milliseconds.
    pub max_ms: f64,
    /// Effective frames per second.
    pub fps: f64,
}

/// Errors returned while computing benchmark statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTimingStatsError {
    /// No frame timing samples were provided.
    Empty,
    /// The measured frames completed in zero total time.
    ZeroTotal,
}

/// Command-line configuration for a frame benchmark binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBenchmarkConfig {
    /// ROM path to benchmark.
    pub rom_path: String,
    /// Number of frames in each measured run.
    pub frames: usize,
    /// Frames to run before measuring.
    pub warmup_frames: usize,
    /// Additional measured runs for stability reporting.
    pub stability_runs: usize,
    /// Whether the GBA BIOS intro should be skipped before benchmark warmup.
    pub skip_gba_bios_intro: bool,
    /// Whether each stability run should reload the ROM before warmup.
    pub reset_stability_runs: bool,
}

/// Errors returned while parsing benchmark command-line options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameBenchmarkConfigError {
    /// The ROM path argument was omitted.
    MissingRomPath,
    /// A flag that requires a value was missing that value.
    MissingValue { name: &'static str },
    /// A numeric argument could not be parsed.
    InvalidNumber { name: &'static str, value: String },
    /// An unsupported argument was provided.
    UnknownArgument(String),
}

impl FrameBenchmarkConfig {
    /// Parse frame benchmark command-line arguments.
    pub fn parse_args(args: &[String]) -> Result<Self, FrameBenchmarkConfigError> {
        let mut args = args.iter();
        let _program = args.next();
        let rom_path = args
            .next()
            .ok_or(FrameBenchmarkConfigError::MissingRomPath)?
            .clone();

        let mut config = Self {
            rom_path,
            frames: 600,
            warmup_frames: 60,
            stability_runs: 5,
            skip_gba_bios_intro: true,
            reset_stability_runs: true,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--frames" => {
                    config.frames = parse_positive_usize_arg(&mut args, "frames")?;
                }
                "--warmup" => {
                    config.warmup_frames = parse_usize_arg(&mut args, "warmup")?;
                }
                "--stability-runs" => {
                    config.stability_runs = parse_usize_arg(&mut args, "stability-runs")?;
                }
                "--include-bios-intro" => {
                    config.skip_gba_bios_intro = false;
                }
                "--skip-bios-intro" => {
                    config.skip_gba_bios_intro = true;
                }
                "--continue-stability-runs" => {
                    config.reset_stability_runs = false;
                }
                _ => return Err(FrameBenchmarkConfigError::UnknownArgument(arg.clone())),
            }
        }

        Ok(config)
    }
}

fn parse_usize_arg<'a, I>(
    args: &mut I,
    name: &'static str,
) -> Result<usize, FrameBenchmarkConfigError>
where
    I: Iterator<Item = &'a String>,
{
    let value = args
        .next()
        .ok_or(FrameBenchmarkConfigError::MissingValue { name })?;
    value
        .parse()
        .map_err(|_| FrameBenchmarkConfigError::InvalidNumber {
            name,
            value: value.clone(),
        })
}

fn parse_positive_usize_arg<'a, I>(
    args: &mut I,
    name: &'static str,
) -> Result<usize, FrameBenchmarkConfigError>
where
    I: Iterator<Item = &'a String>,
{
    let value = args
        .next()
        .ok_or(FrameBenchmarkConfigError::MissingValue { name })?;
    let parsed = value
        .parse()
        .map_err(|_| FrameBenchmarkConfigError::InvalidNumber {
            name,
            value: value.clone(),
        })?;
    if parsed == 0 {
        return Err(FrameBenchmarkConfigError::InvalidNumber {
            name,
            value: value.clone(),
        });
    }
    Ok(parsed)
}

impl FrameTimingStats {
    /// Compute summary statistics for measured frame durations.
    pub fn from_samples(samples: &[Duration]) -> Result<Self, FrameTimingStatsError> {
        if samples.is_empty() {
            return Err(FrameTimingStatsError::Empty);
        }

        let total = samples.iter().copied().sum();
        if total == Duration::ZERO {
            return Err(FrameTimingStatsError::ZeroTotal);
        }

        let frames = samples.len();
        let total_ms = duration_ms(total);
        let mut sorted_ms: Vec<f64> = samples.iter().map(|&sample| duration_ms(sample)).collect();
        sorted_ms.sort_by(f64::total_cmp);

        Ok(Self {
            frames,
            total,
            average_ms: total_ms / frames as f64,
            p50_ms: percentile(&sorted_ms, 0.50),
            p95_ms: percentile(&sorted_ms, 0.95),
            max_ms: sorted_ms[frames - 1],
            fps: frames as f64 / total.as_secs_f64(),
        })
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn percentile(sorted_samples: &[f64], percentile: f64) -> f64 {
    let index = percentile * (sorted_samples.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    let fraction = index - lower as f64;
    sorted_samples[lower] * (1.0 - fraction) + sorted_samples[upper] * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn frame_timing_stats_calculate_percentiles_and_fps() {
        let samples = [ms(12), ms(16), ms(20), ms(10), ms(18)];

        let stats = FrameTimingStats::from_samples(&samples).unwrap();

        assert_eq!(stats.frames, 5);
        assert_eq!(stats.total, ms(76));
        assert_close(stats.average_ms, 15.2);
        assert_close(stats.p50_ms, 16.0);
        assert_close(stats.p95_ms, 19.6);
        assert_close(stats.max_ms, 20.0);
        assert_close(stats.fps, 65.789);
    }

    #[test]
    fn frame_timing_stats_handle_single_sample() {
        let stats = FrameTimingStats::from_samples(&[ms(17)]).unwrap();

        assert_eq!(stats.frames, 1);
        assert_eq!(stats.total, ms(17));
        assert_close(stats.average_ms, 17.0);
        assert_close(stats.p50_ms, 17.0);
        assert_close(stats.p95_ms, 17.0);
        assert_close(stats.max_ms, 17.0);
        assert_close(stats.fps, 58.824);
    }

    #[test]
    fn frame_timing_stats_reject_empty_sample_set() {
        assert_eq!(
            FrameTimingStats::from_samples(&[]),
            Err(FrameTimingStatsError::Empty)
        );
    }

    #[test]
    fn frame_timing_stats_reject_zero_total_time() {
        assert_eq!(
            FrameTimingStats::from_samples(&[Duration::ZERO]),
            Err(FrameTimingStatsError::ZeroTotal)
        );
    }

    #[test]
    fn frame_benchmark_config_uses_gba_title_intro_defaults() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "roms/games/metroid-zero-mission.gba".to_string(),
        ];

        let config = FrameBenchmarkConfig::parse_args(&args).unwrap();

        assert_eq!(config.rom_path, "roms/games/metroid-zero-mission.gba");
        assert_eq!(config.frames, 600);
        assert_eq!(config.warmup_frames, 60);
        assert_eq!(config.stability_runs, 5);
        assert!(config.skip_gba_bios_intro);
        assert!(config.reset_stability_runs);
    }

    #[test]
    fn frame_benchmark_config_parses_overrides() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--frames".to_string(),
            "120".to_string(),
            "--warmup".to_string(),
            "30".to_string(),
            "--stability-runs".to_string(),
            "2".to_string(),
            "--include-bios-intro".to_string(),
        ];

        let config = FrameBenchmarkConfig::parse_args(&args).unwrap();

        assert_eq!(
            config,
            FrameBenchmarkConfig {
                rom_path: "rom.gba".to_string(),
                frames: 120,
                warmup_frames: 30,
                stability_runs: 2,
                skip_gba_bios_intro: false,
                reset_stability_runs: true,
            }
        );
    }

    #[test]
    fn frame_benchmark_config_can_continue_stability_runs_without_resetting() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--continue-stability-runs".to_string(),
        ];

        let config = FrameBenchmarkConfig::parse_args(&args).unwrap();

        assert!(!config.reset_stability_runs);
    }

    #[test]
    fn frame_benchmark_config_rejects_invalid_frame_count() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--frames".to_string(),
            "abc".to_string(),
        ];

        assert_eq!(
            FrameBenchmarkConfig::parse_args(&args),
            Err(FrameBenchmarkConfigError::InvalidNumber {
                name: "frames",
                value: "abc".to_string(),
            })
        );
    }

    #[test]
    fn frame_benchmark_config_rejects_missing_rom_path() {
        let args = vec!["gba_frame_bench".to_string()];

        assert_eq!(
            FrameBenchmarkConfig::parse_args(&args),
            Err(FrameBenchmarkConfigError::MissingRomPath)
        );
    }

    #[test]
    fn frame_benchmark_config_rejects_zero_frame_count() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--frames".to_string(),
            "0".to_string(),
        ];

        assert_eq!(
            FrameBenchmarkConfig::parse_args(&args),
            Err(FrameBenchmarkConfigError::InvalidNumber {
                name: "frames",
                value: "0".to_string(),
            })
        );
    }

    #[test]
    fn frame_benchmark_config_rejects_missing_flag_value() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--frames".to_string(),
        ];

        assert_eq!(
            FrameBenchmarkConfig::parse_args(&args),
            Err(FrameBenchmarkConfigError::MissingValue { name: "frames" })
        );
    }

    #[test]
    fn frame_benchmark_config_rejects_unknown_argument() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--frame".to_string(),
        ];

        assert_eq!(
            FrameBenchmarkConfig::parse_args(&args),
            Err(FrameBenchmarkConfigError::UnknownArgument(
                "--frame".to_string()
            ))
        );
    }

    #[test]
    fn frame_benchmark_config_allows_zero_warmup_and_stability_runs() {
        let args = vec![
            "gba_frame_bench".to_string(),
            "rom.gba".to_string(),
            "--warmup".to_string(),
            "0".to_string(),
            "--stability-runs".to_string(),
            "0".to_string(),
        ];

        let config = FrameBenchmarkConfig::parse_args(&args).unwrap();

        assert_eq!(config.warmup_frames, 0);
        assert_eq!(config.stability_runs, 0);
    }
}
