//! Approved S-DSP audio sample golden checks (#2877).
//!
//! Each test drives the S-DSP directly with a synthetic ARAM fixture and
//! captures a short deterministic window of native 32 kHz stereo output
//! (one sample per 32 DSP phases; the DAC latches the finished sample at
//! phase 27). The committed baseline is the CRC32 of the interleaved L,R
//! i16 window plus the metadata in each [`GoldenAudioWindow`].
//!
//! Approval workflow:
//! 1. Write or change the test with `approved_crc32: 0` — it fails and
//!    prints the actual CRC.
//! 2. Run with `NESER_CAPTURE_AUDIO=1` to write a review WAV under
//!    `target/snes_test_captures/dsp_audio_golden_tests/`.
//! 3. Review the WAV (listen and/or plot with
//!    `python scripts/display_audio_output.py <wav>`).
//! 4. Record the CRC in `approved_crc32` and describe the approval in
//!    `review_note`.
//! 5. Re-run without the env var; the test passes. WAV artifacts live under
//!    the git-ignored `target/` directory and are never committed.

use crate::platform::crc32::crc32;
use crate::snes::apu::dsp::Sdsp;
use std::path::{Path, PathBuf};

/// Native S-DSP output rate: one stereo sample per 32 DSP phases.
const NATIVE_SAMPLE_RATE_HZ: u32 = 32_000;
const PHASES_PER_SAMPLE: usize = 32;
const ARAM_SIZE: usize = 0x1_0000;

/// Approved golden metadata for one deterministic S-DSP capture window.
struct GoldenAudioWindow {
    /// Test/fixture name; also the WAV capture file stem.
    name: &'static str,
    /// Native S-DSP output rate; always 32000.
    sample_rate_hz: u32,
    /// Sample frames run (and discarded) before the window to absorb the
    /// KON poll cadence and key-on delay; part of the approved baseline.
    warmup_samples: usize,
    /// Captured window length in 32 kHz stereo sample frames.
    window_samples: usize,
    /// Fixture description: the synthetic ARAM/register programme that is
    /// the "source ROM or vector" for this baseline.
    source: &'static str,
    /// CRC32 (crate::platform::crc32) over the interleaved L,R i16 window
    /// serialized little-endian.
    approved_crc32: u32,
    /// Who approved the baseline, when, and how it was reviewed.
    review_note: &'static str,
}

/// Interleaved L,R i16 sample window plus its baseline CRC32.
struct CapturedAudio {
    samples: Vec<i16>,
    crc32: u32,
}

/// Drives a standalone [`Sdsp`] over a private zeroed ARAM and records the
/// native 32 kHz output as interleaved L,R i16 samples.
struct DspGoldenRecorder {
    dsp: Sdsp,
    aram: Box<[u8; ARAM_SIZE]>,
    samples: Vec<i16>,
}

impl DspGoldenRecorder {
    fn new() -> Self {
        Self {
            dsp: Sdsp::new(),
            aram: Box::new([0; ARAM_SIZE]),
            samples: Vec::new(),
        }
    }

    /// Fixture access to the recorder's ARAM (directory + BRR data).
    fn aram_mut(&mut self) -> &mut [u8] {
        &mut self.aram[..]
    }

    fn write_reg(&mut self, addr: u8, value: u8) {
        self.dsp.write_reg(addr, value);
    }

    /// Runs `frames` sample frames (32 phases each) without capturing.
    fn run_discard(&mut self, frames: usize) {
        for _ in 0..frames {
            self.step_one_sample_frame();
        }
    }

    /// Runs `frames` sample frames (32 phases each), capturing the DAC
    /// output latched at phase 27 of every frame as exact i16 L,R pairs.
    fn run_capture(&mut self, frames: usize) {
        self.samples.reserve(frames * 2);
        for _ in 0..frames {
            self.step_one_sample_frame();
            let (left, right) = self.dsp.current_stereo_sample();
            self.samples.push(dac_float_to_i16(left));
            self.samples.push(dac_float_to_i16(right));
        }
    }

    fn step_one_sample_frame(&mut self) {
        for _ in 0..PHASES_PER_SAMPLE {
            self.dsp.step_phase_with_memory(&mut self.aram[..]);
        }
    }

    fn finish(self) -> CapturedAudio {
        let crc32 = window_crc32(&self.samples);
        CapturedAudio {
            samples: self.samples,
            crc32,
        }
    }
}

/// Recovers the DAC's i16 output exactly from `current_stereo_sample`'s
/// `i16 / 32768.0` encoding: every i16 is representable in f32 and the
/// power-of-two scale is exact, so the round-trip is lossless across the
/// full range including `i16::MIN`.
fn dac_float_to_i16(sample: f32) -> i16 {
    (sample * 32768.0) as i16
}

/// CRC32 over the window's interleaved i16 samples serialized little-endian.
fn window_crc32(samples: &[i16]) -> u32 {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    crc32(&[&bytes])
}

/// Mirrors `rom_runner::capture_output_path` but with a `.wav` suffix and
/// the fixed `dsp_audio_golden_tests` suite directory.
fn capture_wav_path(stem: &str, crc: u32) -> PathBuf {
    PathBuf::from("target/snes_test_captures")
        .join("dsp_audio_golden_tests")
        .join(format!("{stem}_crc_{crc:08X}.wav"))
}

/// Writes `samples` as a 16-bit stereo WAV at the window's sample rate,
/// creating parent directories as needed.
fn write_capture_wav(path: &Path, sample_rate_hz: u32, samples: &[i16]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|err| panic!("create capture dir {}: {err}", parent.display()));
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .unwrap_or_else(|err| panic!("create capture wav {}: {err}", path.display()));
    for &sample in samples {
        writer
            .write_sample(sample)
            .unwrap_or_else(|err| panic!("write wav sample to {}: {err}", path.display()));
    }
    writer
        .finalize()
        .unwrap_or_else(|err| panic!("finalize capture wav {}: {err}", path.display()));
}

/// Writes the review WAV when `capture_enabled` is set (from the
/// `NESER_CAPTURE_AUDIO` env var) and returns the written path.
fn maybe_write_capture_wav(
    golden: &GoldenAudioWindow,
    captured: &CapturedAudio,
    capture_enabled: bool,
) -> Option<PathBuf> {
    if !capture_enabled {
        return None;
    }
    let path = capture_wav_path(golden.name, captured.crc32);
    write_capture_wav(&path, golden.sample_rate_hz, &captured.samples);
    Some(path)
}

/// Shared golden assertion: builds a recorder, lets `fixture` program the
/// ARAM/registers and drive the warmup/capture segments, writes the
/// on-demand review WAV, and asserts the window CRC against the approved
/// baseline with re-approval instructions in the failure message.
fn assert_golden_audio(golden: &GoldenAudioWindow, fixture: impl FnOnce(&mut DspGoldenRecorder)) {
    assert_eq!(
        golden.sample_rate_hz, NATIVE_SAMPLE_RATE_HZ,
        "{}: golden sample_rate_hz must be the native DSP output rate",
        golden.name
    );

    let mut rec = DspGoldenRecorder::new();
    fixture(&mut rec);
    let captured = rec.finish();

    assert_eq!(
        captured.samples.len(),
        golden.window_samples * 2,
        "{}: fixture captured {} sample frames but window_samples is {}",
        golden.name,
        captured.samples.len() / 2,
        golden.window_samples
    );

    let capture_enabled = std::env::var_os("NESER_CAPTURE_AUDIO").is_some();
    if let Some(path) = maybe_write_capture_wav(golden, &captured, capture_enabled) {
        eprintln!("{}: wrote review WAV to {}", golden.name, path.display());
    }

    assert_eq!(
        captured.crc32,
        golden.approved_crc32,
        "{}: audio window CRC mismatch: actual=0x{:08X} approved=0x{:08X} \
         (rate={} Hz, warmup={} frames, window={} frames; source: {}; \
         review note: {}). To (re-)approve: run with NESER_CAPTURE_AUDIO=1, \
         review the WAV under target/snes_test_captures/dsp_audio_golden_tests/, \
         then update approved_crc32 and review_note.",
        golden.name,
        captured.crc32,
        golden.approved_crc32,
        golden.sample_rate_hz,
        golden.warmup_samples,
        golden.window_samples,
        golden.source,
        golden.review_note
    );
}

/// Writes the 4-byte sample-directory entry for `srcn` (DIR = 0): BRR start
/// and loop addresses, little-endian.
fn write_dir_entry(aram: &mut [u8], srcn: usize, start: u16, loop_start: u16) {
    let base = srcn * 4;
    aram[base] = (start & 0xFF) as u8;
    aram[base + 1] = (start >> 8) as u8;
    aram[base + 2] = (loop_start & 0xFF) as u8;
    aram[base + 3] = (loop_start >> 8) as u8;
}

/// Writes one 9-byte BRR block (header + 8 data bytes) at `addr`.
fn write_brr_block(aram: &mut [u8], addr: usize, header: u8, data: [u8; 8]) {
    aram[addr] = header;
    aram[addr + 1..addr + 9].copy_from_slice(&data);
}

/// Programs the globals shared by the golden fixtures: FLG $20 (running,
/// unmuted, echo writes disabled), sample directory at $0000, and
/// MVOL $60/$60. Echo/noise windows override FLG afterwards as needed.
fn program_common_globals(rec: &mut DspGoldenRecorder) {
    rec.write_reg(0x6C, 0x20); // FLG: no reset, unmuted, echo writes disabled
    rec.write_reg(0x5D, 0x00); // DIR: directory at $0000
    rec.write_reg(0x0C, 0x60); // MVOL L
    rec.write_reg(0x1C, 0x60); // MVOL R
}

/// Programs one voice's volume, pitch, and source-number registers.
fn program_voice(
    rec: &mut DspGoldenRecorder,
    voice: u8,
    vol_l: u8,
    vol_r: u8,
    pitch: u16,
    srcn: u8,
) {
    let base = voice << 4;
    rec.write_reg(base, vol_l);
    rec.write_reg(base + 1, vol_r);
    rec.write_reg(base + 2, (pitch & 0xFF) as u8);
    rec.write_reg(base + 3, (pitch >> 8) as u8);
    rec.write_reg(base + 4, srcn);
}

/// BRR golden window (a): a six-block stream exercising filters 0-3,
/// shifts 0/8/12, and the LOOP+END wrap back to the loop address.
const BRR_DECODE_GOLDEN: GoldenAudioWindow = GoldenAudioWindow {
    name: "brr_decode_filters_and_loop",
    sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
    warmup_samples: 8,
    window_samples: 224,
    source: "synthetic six-block BRR stream at ARAM $0100 (filter 0 shift 0, \
             filter 0 shift 12, filters 1-3 shift 8, LOOP+END wrap to $0100); \
             V0 SRCN 0, pitch $1000, ADSR $8F/$E0, VOL $50/$50, MVOL $60/$60, \
             FLG $20",
    approved_crc32: 0x203B_AE2B,
    review_note: "approved by navigator 2026-07-11: WAV reviewed via waveform \
                  plot and an independent fullsnes-spec BRR decode (Pearson \
                  r=1.0000 after gaussian smoothing; amplitude matches the \
                  ENV*VOL*MVOL gain product to 0.2%; clean 96-frame loop wrap; \
                  no clipping)",
};

/// Programs the BRR fixture and drives the golden warmup/capture windows.
fn brr_decode_fixture(rec: &mut DspGoldenRecorder) {
    let aram = rec.aram_mut();
    write_dir_entry(aram, 0, 0x0100, 0x0100);
    // Header layout: SSSS FFLE (shift, filter, loop, end).
    write_brr_block(
        aram,
        0x0100,
        0x00, // filter 0, shift 0
        [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF],
    );
    write_brr_block(
        aram,
        0x0109,
        0xC0, // filter 0, shift 12
        [0x70, 0x07, 0x70, 0x07, 0x90, 0x09, 0x90, 0x09],
    );
    write_brr_block(
        aram,
        0x0112,
        0x84, // filter 1, shift 8
        [0x44, 0x44, 0x44, 0x44, 0xCC, 0xCC, 0xCC, 0xCC],
    );
    write_brr_block(
        aram,
        0x011B,
        0x88, // filter 2, shift 8
        [0x35, 0x35, 0x35, 0x35, 0xB5, 0xB5, 0xB5, 0xB5],
    );
    write_brr_block(
        aram,
        0x0124,
        0x8C, // filter 3, shift 8
        [0x26, 0x26, 0x26, 0x26, 0xA6, 0xA6, 0xA6, 0xA6],
    );
    write_brr_block(
        aram,
        0x012D,
        0xC3, // filter 0, shift 12, LOOP+END -> wraps to the loop address
        [0x60, 0x06, 0x60, 0x06, 0xA0, 0x0A, 0xA0, 0x0A],
    );

    program_common_globals(rec);
    program_voice(rec, 0, 0x50, 0x50, 0x1000, 0x00);
    rec.write_reg(0x05, 0x8F); // V0 ADSR1: ADSR on, fastest attack
    rec.write_reg(0x06, 0xE0); // V0 ADSR2: SL 7, SR 0 (hold at sustain)
    rec.write_reg(0x4C, 0x01); // KON V0

    rec.run_discard(BRR_DECODE_GOLDEN.warmup_samples);
    rec.run_capture(BRR_DECODE_GOLDEN.window_samples);
}

/// ADSR golden window (b): attack -> decay -> sustain on a looping tone,
/// then KOFF mid-window to capture the release ramp.
const ADSR_ENVELOPE_GOLDEN: GoldenAudioWindow = GoldenAudioWindow {
    name: "adsr_envelope_attack_decay_sustain_release",
    sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
    warmup_samples: 8,
    window_samples: 768,
    source: "synthetic two-block looping tone at ARAM $0200 (filter 0 shift 10, \
             32-sample loop); V0 SRCN 0, pitch $1000, ADSR1 $FF (AR 15, DR 7), \
             ADSR2 $5A (SL 2, SR 26), VOL $50/$50, MVOL $60/$60, FLG $20; \
             KOFF written after 640 captured frames, release captured for 128",
    approved_crc32: 0x1068_BDBC,
    review_note: "approved by navigator 2026-07-11: WAV reviewed via waveform \
                  plot and envelope-stage analysis (instant AR-15 attack, \
                  exponential decay matching the spec ~502-frame time to SL, \
                  sustain plateau 0.355 of peak vs 0.375 SL-2 prediction with \
                  the SR-26 slope explaining the deficit, linear -8/sample \
                  release reaching exact zero as predicted)",
};

/// Writes a simple 32-sample looping tone (two filter-0 shift-10 BRR blocks,
/// ramp up then ramp down) at `start` for use as an envelope carrier.
fn write_loop_tone(aram: &mut [u8], srcn: usize, start: u16) {
    write_dir_entry(aram, srcn, start, start);
    let addr = usize::from(start);
    write_brr_block(
        aram,
        addr,
        0xA0, // filter 0, shift 10: ramp up
        [0x01, 0x23, 0x45, 0x67, 0x77, 0x77, 0x77, 0x77],
    );
    write_brr_block(
        aram,
        addr + 9,
        0xA3, // filter 0, shift 10, LOOP+END: ramp back down
        [0x77, 0x65, 0x43, 0x21, 0x0F, 0xED, 0xCB, 0xA9],
    );
}

/// Programs the ADSR fixture: keyed-on looping tone, then KOFF between the
/// two capture segments so the window covers all four envelope stages.
fn adsr_envelope_fixture(rec: &mut DspGoldenRecorder) {
    write_loop_tone(rec.aram_mut(), 0, 0x0200);

    program_common_globals(rec);
    program_voice(rec, 0, 0x50, 0x50, 0x1000, 0x00);
    rec.write_reg(0x05, 0xFF); // V0 ADSR1: ADSR on, AR 15 (instant), DR 7
    rec.write_reg(0x06, 0x5A); // V0 ADSR2: SL 2, SR 26
    rec.write_reg(0x4C, 0x01); // KON V0

    rec.run_discard(ADSR_ENVELOPE_GOLDEN.warmup_samples);
    rec.run_capture(640); // instant attack, exponential decay to SL, sustain
    rec.write_reg(0x5C, 0x01); // KOFF V0 -> release
    rec.run_capture(128); // linear release ramp (-8/sample)
}

/// GAIN golden window (c): four voices on the shared loop tone at distinct
/// pitches, one GAIN mode each; halfway through, the ramp voices switch to
/// their decrease modes and the direct voice drops to level zero.
const GAIN_MODES_GOLDEN: GoldenAudioWindow = GoldenAudioWindow {
    name: "gain_modes_direct_and_ramps",
    sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
    warmup_samples: 8,
    window_samples: 192,
    source: "shared two-block loop tone at ARAM $0200; V0-V3 SRCN 0 at pitches \
             $0800/$0C00/$1000/$1400, ADSR off, VOL $50/$50, MVOL $60/$60, \
             FLG $20; GAIN V0 $7F direct, V1 $DF linear inc r31, V2 $FF bent \
             inc r31, V3 $7F direct; after 96 captured frames V1 -> $9F linear \
             dec, V2 -> $BF exp dec, V3 -> $00 direct zero",
    approved_crc32: 0xDAC1_CB47,
    review_note: "approved by navigator 2026-07-12: solo-voice diagnostic \
                  captures verified each mode independently (direct constant, \
                  linear ramp full at ~64 frames and back to zero at ~160, \
                  bent ramp with the $600 knee, exponential-decrease tail, \
                  instant direct-zero cut); the golden is their linear mix \
                  with no clamping",
};

/// Programs the GAIN fixture: four keyed-on voices with per-voice GAIN
/// modes, switching to the decrease modes between the capture segments.
fn gain_modes_fixture(rec: &mut DspGoldenRecorder) {
    write_loop_tone(rec.aram_mut(), 0, 0x0200);

    program_common_globals(rec);
    for (voice, pitch) in [(0u8, 0x0800u16), (1, 0x0C00), (2, 0x1000), (3, 0x1400)] {
        program_voice(rec, voice, 0x50, 0x50, pitch, 0x00);
        rec.write_reg((voice << 4) | 0x05, 0x00); // ADSR off -> GAIN mode
    }
    rec.write_reg(0x07, 0x7F); // V0 GAIN: direct, max level
    rec.write_reg(0x17, 0xDF); // V1 GAIN: linear increase, rate 31
    rec.write_reg(0x27, 0xFF); // V2 GAIN: bent increase, rate 31
    rec.write_reg(0x37, 0x7F); // V3 GAIN: direct, max level (switched later)
    rec.write_reg(0x4C, 0x0F); // KON V0-V3

    rec.run_discard(GAIN_MODES_GOLDEN.warmup_samples);
    rec.run_capture(96); // direct level + linear/bent increase ramps
    rec.write_reg(0x17, 0x9F); // V1 GAIN: linear decrease, rate 31
    rec.write_reg(0x27, 0xBF); // V2 GAIN: exponential decrease, rate 31
    rec.write_reg(0x37, 0x00); // V3 GAIN: direct, level 0
    rec.run_capture(96); // decrease ramps + direct drop to silence
}

/// PMON golden window (d): V1 carrier tone pitch-modulated by a silent
/// low-frequency V0 modulator (PMON bit 1); the window spans one full
/// modulation cycle of frequency wobble.
const PITCH_MODULATION_GOLDEN: GoldenAudioWindow = GoldenAudioWindow {
    name: "pitch_modulation_pmon",
    sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
    warmup_samples: 8,
    window_samples: 256,
    source: "shared two-block loop tone at ARAM $0200; V0 modulator SRCN 0 at \
             pitch $0200 with VOL 0/0 (silent, OUTX only), V1 carrier SRCN 0 \
             at pitch $1000 with VOL $60/$60; both ADSR $8F/$E0; PMON $02, \
             MVOL $60/$60, FLG $20, KON $03",
    approved_crc32: 0x6FA3_42B2,
    review_note: "approved by navigator 2026-07-12: zero-crossing frequency \
                  analysis shows the carrier sweeping 879-1217 Hz and back \
                  over one 256-frame modulator period (flat 1000 Hz without \
                  PMON); amplitude matches the VOL/MVOL gain prediction",
};

/// Programs the PMON fixture: keyed-on modulator and carrier with pitch
/// modulation enabled for the carrier only.
fn pitch_modulation_fixture(rec: &mut DspGoldenRecorder) {
    write_loop_tone(rec.aram_mut(), 0, 0x0200);

    program_common_globals(rec);
    program_voice(rec, 0, 0x00, 0x00, 0x0200, 0x00); // silent modulator
    program_voice(rec, 1, 0x60, 0x60, 0x1000, 0x00); // audible carrier
    rec.write_reg(0x05, 0x8F); // V0 ADSR1: ADSR on, fastest attack
    rec.write_reg(0x06, 0xE0); // V0 ADSR2: hold at sustain
    rec.write_reg(0x15, 0x8F); // V1 ADSR1
    rec.write_reg(0x16, 0xE0); // V1 ADSR2
    rec.write_reg(0x2D, 0x02); // PMON: V1 modulated by V0 OUTX
    rec.write_reg(0x4C, 0x03); // KON V0+V1

    rec.run_discard(PITCH_MODULATION_GOLDEN.warmup_samples);
    rec.run_capture(PITCH_MODULATION_GOLDEN.window_samples);
}

/// Gaussian interpolation golden window (f): a jagged source (nibbles
/// alternating +7/-7 every sample) played at pitch $0555 (~1/3 rate), so
/// every output sample lands on a fresh fractional position and the
/// gaussian table's shape and rolloff dominate the output.
const GAUSSIAN_INTERPOLATION_GOLDEN: GoldenAudioWindow = GoldenAudioWindow {
    name: "gaussian_interpolation_fractional_pitch",
    sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
    warmup_samples: 8,
    window_samples: 192,
    source: "two-block loop at ARAM $0300 of alternating +7/-7 nibbles \
             (filter 0 shift 10, source-Nyquist square); V0 SRCN 0, pitch \
             $0555, ADSR $8F/$E0, VOL $60/$60, MVOL $60/$60, FLG $20",
    approved_crc32: 0x9E35_530D,
    review_note: "approved by navigator 2026-07-12: FFT shows the source \
                  alternation at the predicted 5332 Hz resample frequency \
                  with amplitude attenuated to 0.274 of full scale, matching \
                  the gaussian table's ~-11 dB rolloff near source Nyquist",
};

/// Programs the gaussian fixture: keyed-on jagged loop at a heavily
/// fractional pitch.
fn gaussian_interpolation_fixture(rec: &mut DspGoldenRecorder) {
    let aram = rec.aram_mut();
    write_dir_entry(aram, 0, 0x0300, 0x0300);
    // 0x79 nibbles decode to +7,-7 alternating at shift 10.
    write_brr_block(aram, 0x0300, 0xA0, [0x79; 8]);
    write_brr_block(aram, 0x0309, 0xA3, [0x79; 8]);

    program_common_globals(rec);
    program_voice(rec, 0, 0x60, 0x60, 0x0555, 0x00);
    rec.write_reg(0x05, 0x8F); // V0 ADSR1: ADSR on, fastest attack
    rec.write_reg(0x06, 0xE0); // V0 ADSR2: hold at sustain
    rec.write_reg(0x4C, 0x01); // KON V0

    rec.run_discard(GAUSSIAN_INTERPOLATION_GOLDEN.warmup_samples);
    rec.run_capture(GAUSSIAN_INTERPOLATION_GOLDEN.window_samples);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit-test golden with a per-process unique WAV stem so concurrent
    /// `cargo test` invocations sharing `target/` cannot race on the same
    /// capture file.
    fn wav_gate_test_golden() -> GoldenAudioWindow {
        GoldenAudioWindow {
            name: Box::leak(format!("wav-gate-fixture-{}", std::process::id()).into_boxed_str()),
            sample_rate_hz: NATIVE_SAMPLE_RATE_HZ,
            warmup_samples: 0,
            window_samples: 2,
            source: "unit-test fixture (no DSP involved)",
            approved_crc32: 0,
            review_note: "unit-test fixture",
        }
    }

    #[test]
    fn given_full_i16_range_when_recovering_from_dac_float_encoding_then_roundtrip_is_exact() {
        for value in [i16::MIN, -1, 0, 1, i16::MAX] {
            let encoded = f32::from(value) / 32768.0;
            assert_eq!(dac_float_to_i16(encoded), value);
        }
    }

    #[test]
    fn given_stem_and_crc_when_building_capture_wav_path_then_uses_suite_directory_and_crc_suffix()
    {
        assert_eq!(
            capture_wav_path("brr_decode_filters_and_loop", 0x8C90_CEE0),
            PathBuf::from(
                "target/snes_test_captures/dsp_audio_golden_tests/\
                 brr_decode_filters_and_loop_crc_8C90CEE0.wav"
            )
        );
    }

    #[test]
    fn given_known_samples_when_computing_window_crc_then_matches_platform_crc32_of_le_bytes() {
        let samples: [i16; 4] = [1, -2, 300, -400];
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }

        assert_eq!(window_crc32(&samples), crc32(&[&bytes]));
    }

    #[test]
    fn given_fresh_dsp_with_no_key_on_when_capturing_then_window_is_silent() {
        let mut rec = DspGoldenRecorder::new();
        rec.write_reg(0x6C, 0x20); // FLG: no reset, unmuted, echo writes disabled
        rec.run_discard(4);
        rec.run_capture(8);
        let captured = rec.finish();

        assert_eq!(captured.samples.len(), 16);
        assert!(
            captured.samples.iter().all(|&sample| sample == 0),
            "expected silence, got {:?}",
            captured.samples
        );
        assert_eq!(captured.crc32, window_crc32(&[0i16; 16]));
    }

    #[test]
    fn given_identical_fixture_when_capturing_twice_then_samples_and_crc_are_identical() {
        let capture = || {
            let mut rec = DspGoldenRecorder::new();
            brr_decode_fixture(&mut rec);
            rec.finish()
        };

        let first = capture();
        let second = capture();

        assert_eq!(first.samples, second.samples);
        assert_eq!(first.crc32, second.crc32);
        assert!(
            first.samples.iter().any(|&sample| sample != 0),
            "BRR fixture should produce audible (non-zero) output"
        );
    }

    #[test]
    fn given_captured_window_when_writing_wav_then_file_has_expected_spec_and_samples() {
        let samples: [i16; 8] = [0, 100, -100, 32767, -32768, 5, -5, 0];
        let stem = format!("wav-roundtrip-fixture-{}", std::process::id());
        let path = capture_wav_path(&stem, 0x1234_5678);

        write_capture_wav(&path, NATIVE_SAMPLE_RATE_HZ, &samples);

        let mut reader = hound::WavReader::open(&path).expect("open written wav");
        let spec = reader.spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, NATIVE_SAMPLE_RATE_HZ);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        let read_back: Vec<i16> = reader
            .samples::<i16>()
            .map(|sample| sample.expect("read wav sample"))
            .collect();
        assert_eq!(read_back, samples);

        std::fs::remove_file(&path).expect("remove test wav");
    }

    #[test]
    fn given_capture_disabled_when_finishing_golden_then_no_wav_is_written() {
        let golden = wav_gate_test_golden();
        let captured = CapturedAudio {
            samples: vec![0, 0],
            crc32: 0,
        };

        assert_eq!(maybe_write_capture_wav(&golden, &captured, false), None);
    }

    #[test]
    fn given_capture_enabled_when_finishing_golden_then_wav_is_written_to_suite_directory() {
        let golden = wav_gate_test_golden();
        let captured = CapturedAudio {
            samples: vec![1, -1, 2, -2],
            crc32: 0xABCD_EF01,
        };

        let path =
            maybe_write_capture_wav(&golden, &captured, true).expect("capture path when enabled");

        assert_eq!(path, capture_wav_path(golden.name, captured.crc32));
        assert!(path.exists(), "wav should exist at {}", path.display());
        std::fs::remove_file(&path).expect("remove test wav");
    }

    #[test]
    fn given_brr_decode_filters_and_loop_fixture_when_capturing_window_then_crc_matches_approved_golden()
     {
        assert_golden_audio(&BRR_DECODE_GOLDEN, brr_decode_fixture);
    }

    #[test]
    fn given_adsr_envelope_fixture_when_capturing_window_then_crc_matches_approved_golden() {
        assert_golden_audio(&ADSR_ENVELOPE_GOLDEN, adsr_envelope_fixture);
    }

    #[test]
    fn given_gain_modes_fixture_when_capturing_window_then_crc_matches_approved_golden() {
        assert_golden_audio(&GAIN_MODES_GOLDEN, gain_modes_fixture);
    }

    #[test]
    fn given_pitch_modulation_fixture_when_capturing_window_then_crc_matches_approved_golden() {
        assert_golden_audio(&PITCH_MODULATION_GOLDEN, pitch_modulation_fixture);
    }

    #[test]
    fn given_gaussian_interpolation_fixture_when_capturing_window_then_crc_matches_approved_golden()
    {
        assert_golden_audio(
            &GAUSSIAN_INTERPOLATION_GOLDEN,
            gaussian_interpolation_fixture,
        );
    }
}
