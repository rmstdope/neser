use crate::audio::types::{AudioStats, process_sample, queue_sample_to_producer};
use crate::audio::{AudioResampler, NesAudio};
use cpal::SampleRate;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::Consumer;
use ringbuf::traits::Split;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::audio::AudioProducer;
use crate::debugging::log_info;

/// Audio output handler using cpal for the native frontend.
///
/// Mirrors the SDL audio backend's pipeline architecture:
/// ring buffer → adaptive resampler → volume scaling → audio device.
pub struct NativeAudio {
    _stream: cpal::Stream,
    sample_producer: AudioProducer,
    volume: Arc<AtomicU32>,
    stats: Arc<AudioStats>,
    fill_level: Arc<AtomicUsize>,
    actual_sample_rate: i32,
    paused: Arc<AtomicBool>,
}

impl NativeAudio {
    /// Audio buffer size in samples.
    /// At 44.1kHz, this provides ~0.5 seconds of buffering.
    const BUFFER_SIZE: usize = 22050;

    /// Create a new cpal-based audio output handler.
    ///
    /// # Arguments
    /// * `sample_rate` - Target sample rate in Hz (e.g., 44100)
    ///
    /// # Errors
    /// Returns an error if no audio output device is found or stream creation fails.
    pub fn new(sample_rate: i32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No audio output device found".to_string())?;

        let desired_sample_rate = SampleRate(sample_rate as u32);
        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: desired_sample_rate,
            buffer_size: cpal::BufferSize::Fixed(1024),
        };

        // Check if the device supports our desired config, fall back to default if not
        let (actual_config, actual_rate) = match device.supported_output_configs() {
            Ok(mut configs) => {
                let supports_desired = configs.any(|range| {
                    range.channels() == 1
                        && range.min_sample_rate() <= desired_sample_rate
                        && range.max_sample_rate() >= desired_sample_rate
                });
                if supports_desired {
                    (config, sample_rate)
                } else {
                    // Fall back to device default
                    let default_config = device
                        .default_output_config()
                        .map_err(|e| format!("Failed to get default audio config: {e}"))?;
                    let rate = default_config.sample_rate().0 as i32;
                    let fallback = cpal::StreamConfig {
                        channels: 1,
                        sample_rate: default_config.sample_rate(),
                        buffer_size: cpal::BufferSize::Fixed(1024),
                    };
                    (fallback, rate)
                }
            }
            Err(_) => {
                // Can't query supported configs, try our desired config anyway
                (config, sample_rate)
            }
        };

        if actual_rate != sample_rate {
            log_info(format!(
                "Audio: requested {} Hz, got {} Hz from cpal device",
                sample_rate, actual_rate
            ));
        }

        let ring_buffer = HeapRb::<f32>::new(Self::BUFFER_SIZE);
        let (producer, consumer) = ring_buffer.split();
        let fill_level = Arc::new(AtomicUsize::new(0));

        let volume = Arc::new(AtomicU32::new(f32::to_bits(0.75)));
        let stats = Arc::new(AudioStats::default());
        let paused = Arc::new(AtomicBool::new(true));

        let volume_cb = Arc::clone(&volume);
        let stats_cb = Arc::clone(&stats);
        let fill_level_cb = Arc::clone(&fill_level);
        let paused_cb = Arc::clone(&paused);

        let mut consumer = consumer;
        let mut resampler = AudioResampler::new(Self::BUFFER_SIZE / 2);

        let stream = device
            .build_output_stream(
                &actual_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if paused_cb.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }

                    let volume = f32::from_bits(volume_cb.load(Ordering::Relaxed));
                    let fill_level_val = fill_level_cb.load(Ordering::Relaxed);
                    resampler.update_rate(fill_level_val);

                    for sample in data.iter_mut() {
                        let raw_sample = resampler.render_next(&mut || {
                            let s = consumer.try_pop();
                            if s.is_some() {
                                fill_level_cb.fetch_sub(1, Ordering::Relaxed);
                            }
                            s
                        });

                        match raw_sample {
                            Some(raw) => {
                                stats_cb.received_samples.fetch_add(1, Ordering::Relaxed);
                                *sample = process_sample(raw, volume);
                            }
                            None => {
                                stats_cb.underrun_samples.fetch_add(1, Ordering::Relaxed);
                                *sample = 0.0;
                            }
                        }
                    }
                },
                move |err| {
                    eprintln!("cpal audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {e}"))?;

        // Start the stream immediately (paused flag controls actual output)
        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(Self {
            _stream: stream,
            sample_producer: producer,
            volume,
            stats,
            fill_level,
            actual_sample_rate: actual_rate,
            paused,
        })
    }

    /// Returns the current buffered sample count in the ring buffer.
    #[cfg(test)]
    pub fn buffered_samples(&self) -> usize {
        self.fill_level.load(Ordering::Relaxed)
    }
}

impl NesAudio for NativeAudio {
    fn queue_sample(&mut self, sample: f32) {
        queue_sample_to_producer(
            &mut self.sample_producer,
            sample,
            &self.stats,
            &self.fill_level,
        );
    }

    fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    fn pause(&self) {
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
            queue_sample_to_producer(
                &mut self.sample_producer,
                0.0,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_clamping() {
        let audio = NativeAudio::new(44100);
        if let Err(e) = &audio {
            eprintln!("Skipping test_volume_clamping: no audio device ({e})");
            return;
        }
        let audio = audio.unwrap();

        audio.set_volume(0.5);
        assert_eq!(audio.get_volume(), 0.5);

        audio.set_volume(2.0);
        assert_eq!(audio.get_volume(), 1.0);

        audio.set_volume(-0.5);
        assert_eq!(audio.get_volume(), 0.0);
    }

    #[test]
    fn test_default_volume_is_75_percent() {
        let audio = NativeAudio::new(44100);
        if let Err(e) = &audio {
            eprintln!("Skipping test_default_volume: no audio device ({e})");
            return;
        }
        let audio = audio.unwrap();
        assert_eq!(audio.get_volume(), 0.75);
    }

    #[test]
    fn test_stats_reset() {
        let audio = NativeAudio::new(44100);
        if let Err(e) = &audio {
            eprintln!("Skipping test_stats_reset: no audio device ({e})");
            return;
        }
        let audio = audio.unwrap();

        let (received, dropped, underrun) = audio.take_and_reset_stats();
        assert_eq!(received, 0);
        assert_eq!(dropped, 0);
        // Underrun count may be > 0 since the stream is running but paused
        let _ = underrun;

        // Second call should also return zeros for received/dropped
        let (received2, dropped2, _) = audio.take_and_reset_stats();
        assert_eq!(received2, 0);
        assert_eq!(dropped2, 0);
    }

    #[test]
    fn test_prime_startup_and_queue_sample() {
        let audio = NativeAudio::new(44100);
        if let Err(e) = &audio {
            eprintln!("Skipping test_prime_startup: no audio device ({e})");
            return;
        }
        let mut audio = audio.unwrap();

        // Prime buffer
        audio.prime_startup(100);
        assert!(audio.buffered_samples() >= 100);

        // Queue additional samples
        audio.queue_sample(0.5);
        audio.queue_sample(0.3);
        assert!(audio.buffered_samples() >= 102);
    }

    #[test]
    fn test_resume_and_pause() {
        let audio = NativeAudio::new(44100);
        if let Err(e) = &audio {
            eprintln!("Skipping test_resume_and_pause: no audio device ({e})");
            return;
        }
        let audio = audio.unwrap();

        // Starts paused
        assert!(audio.paused.load(Ordering::Relaxed));

        audio.resume();
        assert!(!audio.paused.load(Ordering::Relaxed));

        audio.pause();
        assert!(audio.paused.load(Ordering::Relaxed));
    }
}
