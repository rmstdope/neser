//! Headless frame-capture runner.
//!
//! Loads a ROM, runs it for a fixed number of frames with no window or audio,
//! writes the final frame as a PNG, and exits. The point is reproducibility:
//! given the same ROM and frame count the bytes on disk must be identical every
//! time, so this path takes no input, renders no UI, and never sleeps to pace
//! itself against a display.

use crate::platform::app_context::SharedAppContext;
use crate::platform::config::HeadlessCapture;
use crate::platform::emulator::Console;
use crate::platform::png_utils::write_rgb_png;
use crate::platform::rom_loader::load_console;

/// Ticks one frame may consume before the emulator is declared stuck.
///
/// A hang guard rather than a tight budget: it only has to sit far above a
/// real frame, so that a ROM which never renders fails instead of spinning
/// forever in an unattended script. The most tick-hungry system measured is
/// the GBA at roughly 3e5 ticks per frame, leaving well over an order of
/// magnitude of headroom; the others are in the 1e4–4e4 range. Re-measure
/// before lowering this, or when adding a system.
const MAX_TICKS_PER_FRAME: u64 = 10_000_000;

/// The slice of [`crate::platform::emulator::Emulator`] the frame loop needs.
///
/// Narrow on purpose: it lets the loop's stuck-emulator guard be tested with a
/// three-method double rather than a full 22-method `Emulator` stub.
pub(crate) trait FrameStepper {
    fn run_tick(&mut self) -> u8;
    fn is_ready_to_render(&self) -> bool;
    fn clear_ready_to_render(&mut self);
}

// Each body calls `Console`'s *inherent* method of the same name, which wins
// over the trait method in path syntax. Written as `self.run_tick()` these
// would resolve back to the trait and recurse forever.
impl FrameStepper for Console {
    fn run_tick(&mut self) -> u8 {
        Console::run_tick(self)
    }

    fn is_ready_to_render(&self) -> bool {
        Console::is_ready_to_render(self)
    }

    fn clear_ready_to_render(&mut self) {
        Console::clear_ready_to_render(self)
    }
}

/// Run a headless capture if the configuration asks for one.
///
/// Returns `Ok(true)` when a capture ran and the caller should exit, and
/// `Ok(false)` when no capture was requested and startup should continue.
///
/// The decision lives here rather than in `main.rs` so it can be tested: the
/// binary's own dispatch is not reachable from the library test suite, so
/// keeping it to a single call keeps the untested surface to a minimum.
pub fn run_if_requested(app_context: &SharedAppContext) -> Result<bool, String> {
    // Cloned out of the borrow before running, because the capture itself
    // borrows the context mutably to raise its cartridge-load toast.
    let (capture, rom_path) = {
        let context = app_context.borrow();
        let frontend = &context.config().frontend;
        (frontend.headless_capture.clone(), frontend.rom_path.clone())
    };

    let Some(capture) = capture else {
        return Ok(false);
    };

    let rom_path = rom_path.ok_or_else(|| "--headless requires a ROM path".to_string())?;
    run(app_context, &rom_path, &capture)?;

    Ok(true)
}

/// Run `rom_path` for `capture.frames` frames and write the last one to
/// `capture.output`.
pub fn run(
    app_context: &SharedAppContext,
    rom_path: &str,
    capture: &HeadlessCapture,
) -> Result<(), String> {
    let mut console = load_console(app_context, rom_path)?;
    console.reset(false);

    advance_frames(&mut console, capture.frames)?;

    let rgb = console.screen_snapshot();
    write_rgb_png(
        &capture.output,
        &rgb,
        console.screen_width(),
        console.screen_height(),
    )
    .map_err(|err| {
        format!(
            "Failed to write capture {}: {err}",
            capture.output.display()
        )
    })
}

/// Advance `stepper` by exactly `frames` fully rendered frames.
///
/// Returns an error rather than looping forever if a frame never completes:
/// either the emulator stops reporting progress (`run_tick` returns 0) or it
/// exceeds [`MAX_TICKS_PER_FRAME`] without signalling a frame.
fn advance_frames(stepper: &mut impl FrameStepper, frames: u32) -> Result<(), String> {
    for frame in 0..frames {
        let mut ticks = 0u64;
        while !stepper.is_ready_to_render() {
            if stepper.run_tick() == 0 {
                return Err(format!(
                    "Emulator stopped making progress during frame {}",
                    frame + 1
                ));
            }
            ticks += 1;
            if ticks >= MAX_TICKS_PER_FRAME {
                return Err(format!(
                    "Emulator did not finish frame {} within {MAX_TICKS_PER_FRAME} ticks",
                    frame + 1
                ));
            }
        }
        // Clearing here is what makes the next iteration wait for a *new*
        // frame instead of seeing this one still flagged as ready.
        stepper.clear_ready_to_render();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::{Config, RamInitMode};
    use crate::platform::app_context::AppContext;
    use crate::platform::config::{DEFAULT_CAPTURE_FRAMES, FrontendConfig};
    use crate::platform::test_roms::minimal_nes_rom;
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use tempfile::TempDir;

    fn make_app_context() -> SharedAppContext {
        let config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        Rc::new(RefCell::new(AppContext::new_with_config(config)))
    }

    fn write_nes_rom(dir: &TempDir) -> String {
        let path = dir.path().join("game.nes");
        std::fs::write(&path, minimal_nes_rom(false)).expect("write ROM fixture");
        path.to_string_lossy().into_owned()
    }

    fn capture_to(path: &Path, frames: u32) -> HeadlessCapture {
        HeadlessCapture {
            frames,
            output: path.to_path_buf(),
        }
    }

    /// Decode a PNG from disk into `(width, height, rgb_bytes)`.
    fn decode_png(path: &Path) -> (u32, u32, Vec<u8>) {
        let file = std::fs::File::open(path).expect("written PNG should be readable");
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = decoder.read_info().expect("PNG header should decode");
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .expect("PNG image data should decode");
        buffer.truncate(info.buffer_size());
        (info.width, info.height, buffer)
    }

    /// A stepper that renders a frame every `ticks_per_frame` ticks.
    ///
    /// Two failure modes can be dialled in: `stall_after_frames` makes it stop
    /// signalling frames once that many have completed, while still consuming
    /// ticks (the tick-budget path), and `tick_return` of 0 makes it report no
    /// progress at all (the zero-tick path).
    struct FakeStepper {
        ticks_per_frame: u64,
        ticks_this_frame: u64,
        frames_completed: u32,
        stall_after_frames: Option<u32>,
        tick_return: u8,
    }

    impl FakeStepper {
        fn new(ticks_per_frame: u64) -> Self {
            Self {
                ticks_per_frame,
                ticks_this_frame: 0,
                frames_completed: 0,
                stall_after_frames: None,
                tick_return: 1,
            }
        }

        fn stalling_after(frames: u32) -> Self {
            Self {
                stall_after_frames: Some(frames),
                ..Self::new(10)
            }
        }

        fn returning_zero_ticks() -> Self {
            Self {
                tick_return: 0,
                ..Self::new(10)
            }
        }

        fn stalled(&self) -> bool {
            self.stall_after_frames
                .is_some_and(|limit| self.frames_completed >= limit)
        }
    }

    impl FrameStepper for FakeStepper {
        fn run_tick(&mut self) -> u8 {
            if !self.stalled() {
                self.ticks_this_frame += 1;
            }
            self.tick_return
        }

        fn is_ready_to_render(&self) -> bool {
            !self.stalled() && self.ticks_this_frame >= self.ticks_per_frame
        }

        fn clear_ready_to_render(&mut self) {
            self.ticks_this_frame = 0;
            self.frames_completed += 1;
        }
    }

    // --- frame advancing ---

    #[test]
    fn advance_frames_completes_exactly_the_requested_frame_count() {
        // Given a stepper that renders a frame every 10 ticks
        let mut stepper = FakeStepper::new(10);

        // When three frames are requested
        advance_frames(&mut stepper, 3).expect("three frames should complete");

        // Then exactly three frames were finished, not two or four
        assert_eq!(stepper.frames_completed, 3);
    }

    #[test]
    fn advance_frames_leaves_no_half_finished_frame() {
        // Given a stepper mid-frame after the requested frames complete
        let mut stepper = FakeStepper::new(10);

        // When two frames are requested
        advance_frames(&mut stepper, 2).expect("two frames should complete");

        // Then the loop stopped on a frame boundary, so the captured image is a
        // fully rendered frame rather than a partially drawn one
        assert_eq!(stepper.ticks_this_frame, 0);
    }

    #[test]
    fn advance_frames_reports_an_emulator_that_stops_rendering() {
        // Given a stepper that renders one frame and then never renders again
        let mut stepper = FakeStepper::stalling_after(1);

        // When more frames are requested than it will ever produce
        let error = advance_frames(&mut stepper, 2).expect_err("stall should be reported");

        // Then it gives up rather than spinning forever
        assert!(
            error.contains("frame"),
            "expected the stalled frame to be named in {error:?}"
        );
    }

    #[test]
    fn advance_frames_reports_an_emulator_that_stops_ticking() {
        // Given a stepper whose ticks consume no cycles at all
        let mut stepper = FakeStepper::returning_zero_ticks();

        // When a frame is requested
        let error = advance_frames(&mut stepper, 1).expect_err("no progress should be reported");

        // Then the lack of progress is reported instead of looping forever
        assert!(
            error.contains("frame"),
            "expected the stalled frame to be named in {error:?}"
        );
    }

    // --- dispatch ---

    /// An app context whose config requests `capture` for `rom_path`.
    fn app_context_requesting(
        rom_path: Option<&str>,
        capture: Option<HeadlessCapture>,
    ) -> SharedAppContext {
        let config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                rom_path: rom_path.map(str::to_string),
                headless_capture: capture,
                ..Default::default()
            },
            ..Default::default()
        };
        Rc::new(RefCell::new(AppContext::new_with_config(config)))
    }

    #[test]
    fn run_if_requested_does_nothing_when_no_capture_is_configured() {
        // Given a configuration with a ROM but no capture
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let context = app_context_requesting(Some(&rom), None);

        // When dispatch runs
        let ran = run_if_requested(&context).expect("no capture should not fail");

        // Then it reports that startup should continue
        assert!(!ran, "no capture was requested");
    }

    #[test]
    fn run_if_requested_runs_the_capture_and_reports_it_ran() {
        // Given a configuration requesting a capture
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let output = temp.path().join("shot.png");
        let context = app_context_requesting(Some(&rom), Some(capture_to(&output, 1)));

        // When dispatch runs
        let ran = run_if_requested(&context).expect("capture should succeed");

        // Then it captured and told the caller to exit rather than open a window
        assert!(ran, "a capture was requested");
        assert!(output.exists(), "expected {} to exist", output.display());
    }

    #[test]
    fn run_if_requested_reports_a_missing_rom_path() {
        // Given a capture configured without a ROM path.
        //
        // Config validation already rejects this, so reaching it means the two
        // have drifted apart; failing loudly beats capturing nothing silently.
        let temp = TempDir::new().expect("create temp dir");
        let output = temp.path().join("shot.png");
        let context = app_context_requesting(None, Some(capture_to(&output, 1)));

        // When dispatch runs
        let error = run_if_requested(&context).expect_err("missing ROM should be reported");

        // Then the failure names what is missing
        assert!(error.contains("ROM"), "expected the ROM named in {error:?}");
    }

    #[test]
    fn run_if_requested_propagates_a_capture_failure() {
        // Given a capture pointing at a ROM that does not exist
        let temp = TempDir::new().expect("create temp dir");
        let missing = temp.path().join("nope.nes");
        let output = temp.path().join("shot.png");
        let context = app_context_requesting(
            Some(&missing.to_string_lossy()),
            Some(capture_to(&output, 1)),
        );

        // When dispatch runs
        let error = run_if_requested(&context).expect_err("failure should propagate");

        // Then the error surfaces instead of being swallowed into Ok(true),
        // which would let the binary exit 0 having captured nothing
        assert!(
            error.contains("nope.nes"),
            "expected the ROM path in {error:?}"
        );
    }

    // --- end-to-end capture ---

    #[test]
    fn run_writes_a_png_with_the_console_dimensions() {
        // Given a minimal NES ROM
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let output = temp.path().join("shot.png");

        // When a single frame is captured
        run(&make_app_context(), &rom, &capture_to(&output, 1)).expect("capture should succeed");

        // Then the PNG matches the NES resolution
        let (width, height, rgb) = decode_png(&output);
        assert_eq!((width, height), (256, 240));
        assert_eq!(rgb.len(), 256 * 240 * 3);
    }

    #[test]
    fn run_honours_the_requested_frame_count() {
        // Given a ROM that has not drawn yet at frame 2 but has by frame 10.
        //
        // The synthetic ROMs render an identical screen at every frame count,
        // so they cannot tell a honoured `--frames` from an ignored one; this
        // needs a ROM that actually changes. Verified by mutation: hard-coding
        // the `advance_frames` call in `run` to 1 passes every other test in
        // this module and fails only this one.
        let temp = TempDir::new().expect("create temp dir");
        let early = temp.path().join("early.png");
        let late = temp.path().join("late.png");
        let rom = "roms/nes/rainwarrior/color_test.nes";

        // When the same ROM is captured at two different frame counts
        run(&make_app_context(), rom, &capture_to(&early, 2)).expect("early capture");
        run(&make_app_context(), rom, &capture_to(&late, 10)).expect("late capture");

        // Then the images differ, proving the frame count reached the emulator
        assert_ne!(
            std::fs::read(&early).expect("read early"),
            std::fs::read(&late).expect("read late"),
            "captures at 2 and 10 frames should differ",
        );
    }

    // --- per-system capture ---
    //
    // The runner asks the console for its dimensions, so a capture that assumed
    // one system's resolution would silently produce wrong-sized images for the
    // others. Expected values are the documented hardware resolutions rather
    // than the crate's own constants, which would make the assertion circular.

    /// Capture `rom` for [`DEFAULT_CAPTURE_FRAMES`] frames and return the PNG's
    /// `(width, height)`.
    ///
    /// Also asserts the image is not a single flat colour. Dimensions alone
    /// would pass on an all-black frame, which is what a silently failed load
    /// or a capture that never advanced a frame would produce.
    ///
    /// The frame count matters: dmg-acid2 still renders a blank screen at 30
    /// frames and only draws by 60, so a shorter capture would assert against
    /// nothing. 60 is also the default `--frames`, so these exercise what a
    /// user gets by default.
    fn captured_dimensions(rom: &str, temp: &TempDir) -> (u32, u32) {
        let output = temp.path().join("shot.png");
        run(
            &make_app_context(),
            rom,
            &capture_to(&output, DEFAULT_CAPTURE_FRAMES),
        )
        .unwrap_or_else(|err| panic!("capture of {rom} should succeed: {err}"));

        let (width, height, rgb) = decode_png(&output);
        assert_eq!(
            rgb.len() as u32,
            width * height * 3,
            "RGB buffer should match the header dimensions"
        );

        // A linear scan rather than collecting every pixel into a set: this
        // only needs "is any pixel different from the first", which
        // short-circuits on the second pixel for a rendered frame instead of
        // hashing ~60k entries per test.
        let mut pixels = rgb.chunks_exact(3);
        let first = pixels.next().expect("capture should not be empty");
        assert!(
            pixels.any(|pixel| pixel != first),
            "{rom} captured a single flat colour, so nothing was rendered"
        );

        (width, height)
    }

    #[test]
    fn captures_a_nes_rom_at_256x240() {
        let temp = TempDir::new().expect("create temp dir");
        let dimensions = captured_dimensions("roms/nes/rainwarrior/color_test.nes", &temp);
        assert_eq!(dimensions, (256, 240));
    }

    #[test]
    fn captures_a_game_boy_rom_at_160x144() {
        let temp = TempDir::new().expect("create temp dir");
        let dimensions = captured_dimensions("roms/gb/automated_tests/acid/dmg-acid2.gb", &temp);
        assert_eq!(dimensions, (160, 144));
    }

    #[test]
    fn captures_a_gba_rom_at_240x160() {
        let temp = TempDir::new().expect("create temp dir");
        let dimensions = captured_dimensions(
            "roms/gba/automated_tests/armwrestler/armwrestler.gba",
            &temp,
        );
        assert_eq!(dimensions, (240, 160));
    }

    #[test]
    fn captures_an_snes_rom_at_256x224() {
        let temp = TempDir::new().expect("create temp dir");
        let dimensions =
            captured_dimensions("roms/snes/automated_tests/blargg_apu/test_speed.smc", &temp);
        assert_eq!(dimensions, (256, 224));
    }

    #[test]
    fn run_is_deterministic_across_runs() {
        // Given the same ROM and frame count captured twice
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let first = temp.path().join("first.png");
        let second = temp.path().join("second.png");

        // When both captures run
        run(&make_app_context(), &rom, &capture_to(&first, 3)).expect("first capture");
        run(&make_app_context(), &rom, &capture_to(&second, 3)).expect("second capture");

        // Then the bytes are identical -- the whole point of the mode is that a
        // script can diff captures across builds
        assert_eq!(
            std::fs::read(&first).expect("read first"),
            std::fs::read(&second).expect("read second"),
        );
    }

    #[test]
    fn run_creates_missing_output_directories() {
        // Given an output path whose parent directories do not exist
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let output = temp.path().join("nested").join("shot.png");

        // When the capture runs
        run(&make_app_context(), &rom, &capture_to(&output, 1)).expect("capture should succeed");

        // Then the directories were created rather than the run failing
        assert!(output.exists(), "expected {} to exist", output.display());
    }

    #[test]
    fn run_reports_a_missing_rom_file() {
        // Given a ROM path that does not exist
        let temp = TempDir::new().expect("create temp dir");
        let missing = temp.path().join("nope.nes");
        let output = temp.path().join("shot.png");

        // When the capture runs
        let error = run(
            &make_app_context(),
            &missing.to_string_lossy(),
            &capture_to(&output, 1),
        )
        .expect_err("missing ROM should be reported");

        // Then the failure names the ROM, and nothing was written
        assert!(
            error.contains("nope.nes"),
            "expected the ROM path in {error:?}"
        );
        assert!(!output.exists(), "no PNG should be written on failure");
    }

    #[test]
    fn run_reports_an_unwritable_output_path() {
        // Given an output path whose parent is a regular file
        let temp = TempDir::new().expect("create temp dir");
        let rom = write_nes_rom(&temp);
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("write blocker");
        let output = blocker.join("shot.png");

        // When the capture runs
        let error = run(&make_app_context(), &rom, &capture_to(&output, 1))
            .expect_err("unwritable output should be reported");

        // Then the write failure is surfaced rather than panicking
        assert!(
            error.contains("shot.png") || error.contains("blocker"),
            "expected the output path in {error:?}"
        );
    }
}
