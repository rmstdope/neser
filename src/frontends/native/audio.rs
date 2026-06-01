use crate::platform::audio::types::{
    AudioConsumer, AudioStats, queue_sample_to_producer, queue_stereo_sample_to_producer,
};
use crate::platform::audio::{AudioProducer, AudioResampler, EmulatorAudio};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SampleRate, SizedSample, StreamConfig};
use ringbuf::HeapRb;
use ringbuf::traits::Consumer;
use ringbuf::traits::Split;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::platform::debugging::log_info;

/// Shared state between the `NativeAudio` owner and the cpal audio callback.
///
/// All fields are `Arc`-wrapped atomics so both sides can read/write without locks.
struct StreamSharedState {
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    fill_level: Arc<AtomicUsize>,
    paused: Arc<AtomicBool>,
    /// Number of stale pre-pause ring-buffer samples to discard silently on resume.
    /// Written by `NativeAudio::pause()`, decremented by the cpal callback.
    drain_stale: Arc<AtomicUsize>,
}

impl StreamSharedState {
    fn new() -> Self {
        Self {
            volume: Arc::new(AtomicU32::new(f32::to_bits(0.75))),
            stats: Arc::new(AudioStats::default()),
            fill_level: Arc::new(AtomicUsize::new(0)),
            paused: Arc::new(AtomicBool::new(true)),
            drain_stale: Arc::new(AtomicUsize::new(0)),
        }
    }
}

/// Audio output handler using cpal for the native frontend.
///
/// Mirrors the SDL audio backend's pipeline architecture:
/// ring buffer → adaptive resampler → volume scaling → audio device.
pub struct NativeAudio {
    _stream: Option<cpal::Stream>,
    sample_producer: AudioProducer,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    fill_level: Arc<AtomicUsize>,
    actual_sample_rate: i32,
    buffer_samples: usize,
    startup_prime_samples: usize,
    paused: Arc<AtomicBool>,
    /// Number of stale ring-buffer samples to discard silently on the next resume.
    ///
    /// Snapshotted from `fill_level` in `pause()`.  The cpal callback drains
    /// exactly this many samples (outputting silence) before switching to normal
    /// playback, preventing pre-pause audio from bleeding into post-resume audio.
    drain_stale: Arc<AtomicUsize>,
}

impl NativeAudio {
    /// Smallest ring buffer we will use, regardless of the configured latency.
    const MIN_BUFFER_SAMPLES: usize = 2048;

    /// Create a new cpal-based audio output handler.
    ///
    /// # Arguments
    /// * `sample_rate` - Target sample rate in Hz (e.g., 44100)
    /// * `buffer_ms` - Target audio buffering in milliseconds
    ///
    /// # Errors
    /// Returns an error if no audio output device is found or stream creation fails.
    pub fn new(sample_rate: i32, buffer_ms: u32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No audio output device found".to_string())?;

        let (channels, actual_rate, sample_format, stream_config) =
            Self::select_stream_config(&device, sample_rate)?;

        if actual_rate != sample_rate {
            log_info(format!(
                "Audio: no mono f32 stream at {} Hz available — falling back to device default at {} Hz",
                sample_rate, actual_rate
            ));
        }

        let buffer_samples = Self::buffer_samples_for(actual_rate, buffer_ms);
        let startup_prime_samples = Self::startup_prime_samples_for(buffer_samples);
        log_info(format!(
            "Audio: using {} ms buffer -> {} samples (~{:.1} ms) at {} Hz",
            buffer_ms,
            buffer_samples,
            buffer_samples as f32 * 1000.0 / actual_rate as f32,
            actual_rate
        ));

        let ring_buffer = HeapRb::<[f32; 2]>::new(buffer_samples);
        let (producer, consumer) = ring_buffer.split();
        let shared = StreamSharedState::new();

        let stream = Self::build_stream(
            &device,
            &stream_config,
            channels,
            sample_format,
            buffer_samples,
            consumer,
            &shared,
        )?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(Self {
            _stream: Some(stream),
            sample_producer: producer,
            volume: shared.volume,
            stats: shared.stats,
            fill_level: shared.fill_level,
            actual_sample_rate: actual_rate,
            buffer_samples,
            startup_prime_samples,
            paused: shared.paused,
            drain_stale: shared.drain_stale,
        })
    }

    fn buffer_samples_for(sample_rate: i32, buffer_ms: u32) -> usize {
        let sample_rate = sample_rate.max(1) as u128;
        let buffer_ms = buffer_ms as u128;
        let requested = sample_rate.saturating_mul(buffer_ms).saturating_add(999) / 1000;
        let requested = requested.min(usize::MAX as u128) as usize;
        requested.max(Self::MIN_BUFFER_SAMPLES)
    }

    fn startup_prime_samples_for(buffer_samples: usize) -> usize {
        (buffer_samples / 4)
            .max(256)
            .min(buffer_samples.saturating_sub(1))
    }

    /// Selects the best available stream config for the device.
    ///
    /// Prefers mono f32 at the requested rate; falls back to the device default
    /// (preserving its actual channel count and sample format).
    fn select_stream_config(
        device: &cpal::Device,
        desired_rate: i32,
    ) -> Result<(u16, i32, SampleFormat, StreamConfig), String> {
        let desired_sample_rate = SampleRate(desired_rate as u32);

        // Prefer mono f32 at the desired sample rate.
        if let Ok(mut configs) = device.supported_output_configs()
            && configs.any(|r| {
                r.channels() == 1
                    && r.sample_format() == SampleFormat::F32
                    && r.min_sample_rate() <= desired_sample_rate
                    && r.max_sample_rate() >= desired_sample_rate
            })
        {
            return Ok((
                1,
                desired_rate,
                SampleFormat::F32,
                StreamConfig {
                    channels: 1,
                    sample_rate: desired_sample_rate,
                    buffer_size: BufferSize::Fixed(1024),
                },
            ));
        }

        // Fall back to the device default, keeping its actual channel count and
        // sample format so build_output_stream does not fail on format mismatch.
        let default = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default audio config: {e}"))?;
        let channels = default.channels();
        let rate = default.sample_rate().0 as i32;
        let format = default.sample_format();
        let config = StreamConfig {
            channels,
            sample_rate: default.sample_rate(),
            buffer_size: BufferSize::Fixed(1024),
        };
        Ok((channels, rate, format, config))
    }

    /// Dispatches stream construction to the correct typed builder based on the
    /// device's native sample format.
    fn build_stream(
        device: &cpal::Device,
        config: &StreamConfig,
        channels: u16,
        sample_format: SampleFormat,
        buffer_samples: usize,
        consumer: AudioConsumer,
        shared: &StreamSharedState,
    ) -> Result<cpal::Stream, String> {
        match sample_format {
            SampleFormat::F32 => Self::build_typed_stream::<f32>(
                device,
                config,
                channels,
                buffer_samples,
                consumer,
                shared,
            ),
            SampleFormat::I16 => Self::build_typed_stream::<i16>(
                device,
                config,
                channels,
                buffer_samples,
                consumer,
                shared,
            ),
            SampleFormat::U16 => Self::build_typed_stream::<u16>(
                device,
                config,
                channels,
                buffer_samples,
                consumer,
                shared,
            ),
            _ => Err(format!(
                "Unsupported audio sample format: {sample_format:?}"
            )),
        }
    }

    /// Builds a typed cpal output stream for the given sample type.
    ///
    /// Converts the stereo `[f32; 2]` audio frames to the device's sample format `T`.
    /// For stereo devices, channel 0 receives left and channel 1 receives right.
    /// For mono devices, both channels are averaged.  Extra channels (>2) receive left.
    fn build_typed_stream<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: &StreamConfig,
        channels: u16,
        buffer_samples: usize,
        mut consumer: AudioConsumer,
        shared: &StreamSharedState,
    ) -> Result<cpal::Stream, String> {
        let volume = Arc::clone(&shared.volume);
        let stats = Arc::clone(&shared.stats);
        let fill_level = Arc::clone(&shared.fill_level);
        let paused = Arc::clone(&shared.paused);
        let drain_stale = Arc::clone(&shared.drain_stale);
        let mut resampler = AudioResampler::new(buffer_samples / 2);

        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    if paused.load(Ordering::Relaxed) {
                        data.fill(T::EQUILIBRIUM);
                        return;
                    }

                    let vol = f32::from_bits(volume.load(Ordering::Relaxed));
                    let fill = fill_level.load(Ordering::Relaxed);
                    resampler.update_rate(fill);

                    // `data` is interleaved: [L, R, L, R, ...] for stereo.
                    // Each ring-buffer entry is a `[f32; 2]` stereo frame [L, R].
                    for frame in data.chunks_mut(channels as usize) {
                        // Drain stale pre-pause samples silently before playing
                        // fresh emulation audio.  When drain_stale > 0, pop one
                        // sample from the ring buffer and output silence instead.
                        let stale = drain_stale.load(Ordering::Relaxed);
                        if stale > 0 {
                            if consumer.try_pop().is_some() {
                                let _ = fill_level.fetch_update(
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                    |l| Some(l.saturating_sub(1)),
                                );
                                drain_stale.fetch_sub(1, Ordering::Relaxed);
                                if drain_stale.load(Ordering::Relaxed) == 0 {
                                    resampler.reset();
                                }
                            }
                            frame.fill(T::EQUILIBRIUM);
                            continue;
                        }
                        let raw = resampler.render_next(&mut || {
                            let s = consumer.try_pop();
                            if s.is_some() {
                                // Saturating to prevent underflow if the callback
                                // races ahead of the fill_level increment.
                                let _ = fill_level.fetch_update(
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                    |level| Some(level.saturating_sub(1)),
                                );
                            }
                            s
                        });

                        match raw {
                            Some([l, r]) => {
                                stats.received_samples.fetch_add(1, Ordering::Relaxed);
                                // Ring buffer holds bipolar PCM in [-1.0, 1.0].
                                // System-specific normalization (e.g. NES ÷ 1.177) is
                                // applied by the caller before queue_sample(), so only
                                // volume scaling is needed here.
                                let l_out = T::from_sample((l * vol).clamp(-1.0, 1.0));
                                let r_out = T::from_sample((r * vol).clamp(-1.0, 1.0));
                                for (i, ch) in frame.iter_mut().enumerate() {
                                    *ch = if channels == 1 {
                                        // Mono device: average L and R.
                                        T::from_sample(((l + r) / 2.0 * vol).clamp(-1.0, 1.0))
                                    } else if i == 0 {
                                        l_out
                                    } else if i == 1 {
                                        r_out
                                    } else {
                                        // >2 channels: fill with left.
                                        l_out
                                    };
                                }
                            }
                            None => {
                                stats.underrun_samples.fetch_add(1, Ordering::Relaxed);
                                frame.fill(T::EQUILIBRIUM);
                            }
                        }
                    }
                },
                move |err| {
                    eprintln!("cpal audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {e}"))
    }

    /// Returns the current buffered sample count in the ring buffer.
    #[cfg(test)]
    pub fn buffered_samples(&self) -> usize {
        self.fill_level.load(Ordering::Relaxed)
    }

    pub fn startup_prime_samples(&self) -> usize {
        debug_assert!(self.startup_prime_samples <= self.buffer_samples);
        self.startup_prime_samples
    }

    #[cfg(test)]
    pub fn buffer_capacity_samples(&self) -> usize {
        self.buffer_samples
    }

    /// Returns `true` if audio output is currently paused.
    #[cfg(test)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Returns the number of stale samples to be drained silently on next resume.
    #[cfg(test)]
    pub fn drain_stale_count(&self) -> usize {
        self.drain_stale.load(Ordering::Relaxed)
    }

    /// Creates a `NativeAudio` without opening a real cpal device or stream.
    ///
    /// Intended for unit tests that exercise pure logic (volume, stats, buffering)
    /// without requiring audio hardware.
    #[cfg(test)]
    fn new_without_stream(sample_rate: i32, buffer_ms: u32) -> Self {
        let buffer_samples = Self::buffer_samples_for(sample_rate, buffer_ms);
        let startup_prime_samples = Self::startup_prime_samples_for(buffer_samples);
        let ring_buffer = HeapRb::<[f32; 2]>::new(buffer_samples);
        let (producer, _consumer) = ring_buffer.split();
        let shared = StreamSharedState::new();
        Self {
            _stream: None,
            sample_producer: producer,
            volume: shared.volume,
            stats: shared.stats,
            fill_level: shared.fill_level,
            actual_sample_rate: sample_rate,
            buffer_samples,
            startup_prime_samples,
            paused: shared.paused,
            drain_stale: shared.drain_stale,
        }
    }
}

impl EmulatorAudio for NativeAudio {
    fn queue_sample(&mut self, sample: f32) {
        // Drop silently when paused: the cpal callback stops draining while paused,
        // so pushing would fill the ring buffer and spin-block the main thread.
        if self.paused.load(Ordering::Relaxed) {
            return;
        }
        queue_sample_to_producer(
            &mut self.sample_producer,
            sample,
            &self.stats,
            &self.fill_level,
        );
    }

    fn queue_stereo_sample(&mut self, left: f32, right: f32) {
        // Drop silently when paused.
        if self.paused.load(Ordering::Relaxed) {
            return;
        }
        queue_stereo_sample_to_producer(
            &mut self.sample_producer,
            [left, right],
            &self.stats,
            &self.fill_level,
        );
    }

    fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    fn pause(&self) {
        // Snapshot fill_level so the cpal callback discards these stale samples
        // silently on the next resume, preventing pre-pause audio from bleeding
        // into post-resume audio.
        self.drain_stale
            .store(self.fill_level.load(Ordering::Relaxed), Ordering::Relaxed);
        self.paused.store(true, Ordering::Relaxed);
    }

    fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume.store(f32::to_bits(clamped), Ordering::Relaxed);
    }

    fn get_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    fn prime_startup(&mut self, samples: usize) {
        for _ in 0..samples {
            queue_stereo_sample_to_producer(
                &mut self.sample_producer,
                [0.0, 0.0],
                &self.stats,
                &self.fill_level,
            );
        }
    }

    fn take_and_reset_stats(&self) -> (u64, u64, u64) {
        let received = self.stats.received_samples.swap(0, Ordering::Relaxed);
        let dropped = self.stats.dropped_samples.swap(0, Ordering::Relaxed);
        let underrun = self.stats.underrun_samples.swap(0, Ordering::Relaxed);
        (received, dropped, underrun)
    }

    fn actual_sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }

    fn drain_buffer(&self) {
        // Snapshot the current fill level so the cpal callback discards exactly
        // these samples silently before switching to fresh audio.  This is the
        // same mechanism as pause() but without changing the paused flag, so
        // the emulator continues queuing new samples immediately.
        self.drain_stale
            .store(self.fill_level.load(Ordering::Relaxed), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_clamping() {
        let audio = NativeAudio::new_without_stream(44100, 60);

        audio.set_volume(0.5);
        assert_eq!(audio.get_volume(), 0.5);

        audio.set_volume(2.0);
        assert_eq!(audio.get_volume(), 1.0);

        audio.set_volume(-0.5);
        assert_eq!(audio.get_volume(), 0.0);
    }

    #[test]
    fn test_default_volume_is_75_percent() {
        let audio = NativeAudio::new_without_stream(44100, 60);
        assert_eq!(audio.get_volume(), 0.75);
    }

    #[test]
    fn test_stats_reset() {
        let audio = NativeAudio::new_without_stream(44100, 60);

        let (received, dropped, underrun) = audio.take_and_reset_stats();
        assert_eq!(received, 0);
        assert_eq!(dropped, 0);
        // While paused, the callback returns early and does not record underruns.
        assert_eq!(underrun, 0);

        // Second call should also return zeros
        let (received2, dropped2, underrun2) = audio.take_and_reset_stats();
        assert_eq!(received2, 0);
        assert_eq!(dropped2, 0);
        assert_eq!(underrun2, 0);
    }

    #[test]
    fn test_prime_startup_and_queue_sample() {
        let mut audio = NativeAudio::new_without_stream(44100, 60);

        // Prime buffer (bypasses queue_sample, so works regardless of paused state)
        audio.prime_startup(100);
        assert!(audio.buffered_samples() >= 100);

        // queue_sample only pushes when resumed
        audio.resume();
        audio.queue_sample(0.5);
        audio.queue_sample(0.3);
        assert!(audio.buffered_samples() >= 102);
    }

    #[test]
    fn test_resume_and_pause() {
        let audio = NativeAudio::new_without_stream(44100, 60);

        // Starts paused
        assert!(audio.is_paused());

        audio.resume();
        assert!(!audio.is_paused());

        audio.pause();
        assert!(audio.is_paused());
    }

    #[test]
    fn test_queue_sample_does_not_push_when_paused() {
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        assert!(audio.is_paused(), "starts paused");

        for i in 0..5 {
            audio.queue_sample(i as f32 * 0.1);
        }

        assert_eq!(
            audio.buffered_samples(),
            0,
            "queue_sample while paused must not push into the ring buffer"
        );
    }

    #[test]
    fn test_drain_buffer_marks_buffered_samples_as_stale_without_pausing() {
        // When a save-state is restored (F7), the ring buffer still contains
        // pre-restore samples.  drain_buffer() must snapshot fill_level into
        // drain_stale (so the cpal callback discards those samples silently)
        // WITHOUT changing the paused state, so emulation audio resumes
        // immediately without an explicit resume() call.
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        audio.resume();

        for _ in 0..10 {
            audio.queue_sample(0.5);
        }
        assert_eq!(audio.buffered_samples(), 10);

        audio.drain_buffer();

        assert_eq!(
            audio.drain_stale_count(),
            10,
            "drain_buffer must set drain_stale to the current fill_level"
        );
        assert!(
            !audio.is_paused(),
            "drain_buffer must not change the paused state"
        );
    }

    #[test]
    fn test_pause_snapshots_stale_sample_count_for_drain_on_resume() {
        // When audio.pause() is called (e.g. on focus loss), the ring buffer
        // still contains samples queued before the pause.  The callback stops
        // draining while paused, so those stale samples would play back
        // immediately on resume — producing an audible artifact.
        //
        // pause() must snapshot the current fill_level so the callback knows
        // exactly how many samples to discard silently when it resumes.
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        audio.resume();

        for _ in 0..10 {
            audio.queue_sample(0.5);
        }
        assert_eq!(audio.buffered_samples(), 10);

        audio.pause();

        assert_eq!(
            audio.drain_stale_count(),
            10,
            "pause() must snapshot fill_level into drain_stale so stale samples are discarded on resume"
        );
    }

    #[test]
    fn test_queue_stereo_sample_increments_buffered_count() {
        // queue_stereo_sample() must push one frame into the ring buffer
        // (stereo pair counts as a single "sample" from fill_level's perspective).
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        audio.resume();

        audio.queue_stereo_sample(0.5, -0.3);
        assert_eq!(
            audio.buffered_samples(),
            1,
            "one stereo frame must register as 1 in buffered_samples()"
        );

        audio.queue_stereo_sample(0.2, 0.8);
        assert_eq!(
            audio.buffered_samples(),
            2,
            "two stereo frames must register as 2 in buffered_samples()"
        );
    }

    #[test]
    fn test_queue_stereo_sample_does_not_push_when_paused() {
        // Like queue_sample, queue_stereo_sample must be silent while paused.
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        assert!(audio.is_paused());

        audio.queue_stereo_sample(0.5, -0.5);

        assert_eq!(
            audio.buffered_samples(),
            0,
            "queue_stereo_sample while paused must not push into the ring buffer"
        );
    }

    #[test]
    fn test_queue_sample_and_queue_stereo_sample_count_together() {
        // Both queue_sample and queue_stereo_sample contribute to the same
        // fill_level counter, and the counts must be consistent.
        let mut audio = NativeAudio::new_without_stream(44100, 60);
        audio.resume();

        audio.queue_sample(0.5);
        audio.queue_stereo_sample(0.3, -0.3);
        audio.queue_sample(0.1);

        assert_eq!(
            audio.buffered_samples(),
            3,
            "3 frames (mix of mono and stereo) must register as 3 buffered samples"
        );
    }

    #[test]
    fn test_buffer_samples_are_derived_from_ms_and_floor_to_safe_minimum() {
        assert_eq!(NativeAudio::buffer_samples_for(44100, 60), 2646);
        assert_eq!(NativeAudio::buffer_samples_for(44100, 20), 2048);
    }

    #[test]
    fn test_startup_prime_samples_scale_with_buffer_size() {
        assert_eq!(NativeAudio::startup_prime_samples_for(2646), 661);
        assert_eq!(NativeAudio::startup_prime_samples_for(2048), 512);
    }

    #[test]
    fn test_new_without_stream_exposes_buffer_metadata() {
        let audio = NativeAudio::new_without_stream(44100, 60);

        assert_eq!(audio.buffer_capacity_samples(), 2646);
        assert_eq!(audio.startup_prime_samples(), 661);
    }
}
