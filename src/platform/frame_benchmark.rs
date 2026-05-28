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
    let rank = (percentile * sorted_samples.len() as f64).ceil() as usize;
    sorted_samples[rank.saturating_sub(1).min(sorted_samples.len() - 1)]
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
        assert_close(stats.p95_ms, 20.0);
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
}
