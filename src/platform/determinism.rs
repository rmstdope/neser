//! Test harness for the determinism of a core (nr-ayk5.1).
//!
//! Network play runs one game on two machines in lockstep and resyncs a drifted one by
//! sending it a save state, so a core must be a pure function of (ROM, settings, inputs):
//!
//! - two consoles given the same ROM and the same pad presses agree on their whole state
//!   every frame ([`assert_two_instances_agree`]);
//! - a save state loaded into a fresh console continues exactly as the console it was taken
//!   from ([`assert_save_mid_run_continues_identically`]).
//!
//! Each frame compares the save state, the screen CRC and the audio samples produced, so
//! state a save leaves out shows up as soon as it changes what the console does.
//!
//! Consoles are built as the web frontend builds them (`Console::new_*` then `load_rom`),
//! with power-on RAM seeded: random-looking, as on hardware, but the same on both sides.

use crate::platform::app_context::AppContext;
use crate::platform::config::{Config, RamInitMode};
use crate::platform::emulator::{Console, SystemType};

/// How long a determinism run plays, and when the save/load run saves.
pub(crate) struct DeterminismRun {
    /// Frames played from power-on.
    pub frames: u32,
    /// Frame after which the save/load run saves and loads into a fresh console.
    pub save_at: u32,
}

/// The power-on RAM seed both consoles share.
const RAM_SEED: u64 = 0x5EED_1234;
/// Ticks one frame may take before the run is declared hung.
const MAX_TICKS_PER_FRAME: u32 = 10_000_000;
/// Frames each scripted pad pattern is held, so games see presses long enough to act on.
const FRAMES_PER_PAD_PATTERN: u32 = 8;

/// Two consoles fed the same ROM and pad presses agree on state, screen and audio every frame.
pub(crate) fn assert_two_instances_agree(system: SystemType, rom_path: &str, run: &DeterminismRun) {
    let rom = read_rom(rom_path);
    let mut a = loaded_console(system, &rom, rom_path);
    let mut b = loaded_console(system, &rom, rom_path);
    assert!(
        a.save_state_bytes().expect("save state") == b.save_state_bytes().expect("save state"),
        "{rom_path}: the two consoles differ at power-on"
    );
    for frame in 0..run.frames {
        let out_a = play_frame(&mut a, system, frame, rom_path);
        let out_b = play_frame(&mut b, system, frame, rom_path);
        assert_same(&out_a, &out_b, rom_path, frame, "two-instance");
    }
}

/// A state saved after `run.save_at` frames and loaded into a freshly powered-on console
/// continues, frame by frame, exactly as the console it was saved from.
pub(crate) fn assert_save_mid_run_continues_identically(
    system: SystemType,
    rom_path: &str,
    run: &DeterminismRun,
) {
    let rom = read_rom(rom_path);
    let mut uninterrupted = loaded_console(system, &rom, rom_path);
    for frame in 0..run.save_at {
        play_frame(&mut uninterrupted, system, frame, rom_path);
    }
    let saved = uninterrupted.save_state_bytes().expect("save state");
    let mut resumed = loaded_console(system, &rom, rom_path);
    resumed
        .load_state_bytes(&saved)
        .unwrap_or_else(|e| panic!("{rom_path}: load state: {e}"));
    for frame in run.save_at..run.frames {
        let out_u = play_frame(&mut uninterrupted, system, frame, rom_path);
        let out_r = play_frame(&mut resumed, system, frame, rom_path);
        assert_same(&out_u, &out_r, rom_path, frame, "save/load");
    }
}

fn read_rom(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn loaded_console(system: SystemType, rom: &[u8], name: &str) -> Console {
    let mut config = Config::default();
    config.frontend.ram_init_mode = RamInitMode::SeededRandom(RAM_SEED);
    let context = AppContext::new_with_config(config);
    let mut console = match system {
        SystemType::Nes => Console::new_nes(context),
        SystemType::Snes => Console::new_snes(context),
        other => panic!("determinism is pinned for the NES and SNES, not {other:?}"),
    };
    console
        .load_rom(rom, name)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    console
}

/// The two pad ports, as `set_button` numbers them, and the button ids each pad has.
fn pad_layout(system: SystemType) -> ([u8; 2], u8) {
    match system {
        SystemType::Nes => ([1, 2], 8),
        _ => ([0, 1], 12),
    }
}

/// The scripted presses for `frame` on both pads, one bit per button id: a fixed
/// pseudo-random pattern (splitmix64) that changes every [`FRAMES_PER_PAD_PATTERN`] frames.
fn pads_for_frame(frame: u32) -> [u16; 2] {
    let mut x = u64::from(frame / FRAMES_PER_PAD_PATTERN).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    [x as u16, (x >> 16) as u16]
}

/// What one frame leaves observable.
struct FrameOutput {
    state: Vec<u8>,
    screen_crc: u32,
    /// Stereo samples as raw bits, so the comparison is exact.
    audio: Vec<(u32, u32)>,
}

/// Apply `frame`'s pad presses, run the console to the end of the frame, and collect what
/// it produced.
fn play_frame(console: &mut Console, system: SystemType, frame: u32, name: &str) -> FrameOutput {
    let (ports, buttons) = pad_layout(system);
    for (port, pad) in ports.into_iter().zip(pads_for_frame(frame)) {
        for button in 0..buttons {
            console.set_button(port, button, pad & (1 << button) != 0);
        }
    }
    let mut ticks = 0;
    while !console.is_ready_to_render() {
        assert_ne!(
            console.run_tick(),
            0,
            "{name}: no progress in frame {frame}"
        );
        ticks += 1;
        assert!(
            ticks < MAX_TICKS_PER_FRAME,
            "{name}: frame {frame} never ended"
        );
    }
    console.clear_ready_to_render();
    let mut audio = Vec::new();
    while let Some((left, right)) = console.get_stereo_sample() {
        audio.push((left.to_bits(), right.to_bits()));
    }
    FrameOutput {
        state: console.save_state_bytes().expect("save state"),
        screen_crc: console.screen_crc32(),
        audio,
    }
}

fn assert_same(a: &FrameOutput, b: &FrameOutput, name: &str, frame: u32, run: &str) {
    if a.state != b.state {
        let at = a
            .state
            .iter()
            .zip(&b.state)
            .position(|(x, y)| x != y)
            .unwrap_or(a.state.len().min(b.state.len()));
        let context = |state: &[u8]| {
            let from = at.saturating_sub(120).min(state.len());
            String::from_utf8_lossy(&state[from..(at + 40).min(state.len())]).into_owned()
        };
        panic!(
            "{name}: {run} states differ at frame {frame}, byte {at}:\n  {}\n  {}",
            context(&a.state),
            context(&b.state)
        );
    }
    assert!(
        a.screen_crc == b.screen_crc,
        "{name}: {run} screens differ at frame {frame}"
    );
    assert!(
        a.audio == b.audio,
        "{name}: {run} audio differs at frame {frame}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_patterns_hold_for_eight_frames_then_change() {
        assert_eq!(pads_for_frame(0), pads_for_frame(7));
        assert_ne!(pads_for_frame(7), pads_for_frame(8));
    }
}
