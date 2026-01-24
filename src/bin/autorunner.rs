use neser::cartridge::Cartridge;
use neser::console::{Config, Nes, ParseResult};
use neser::input::Button;
use neser::integration_tests::autorun::{
    AUTORUN_VERSION, AutorunFile, AutorunFrame, autorun_path_for_rom, crc32, load_autorun_file,
    save_autorun_file,
};
use neser::sdl_frontend::SdlGlBackend;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashMap;
use std::fs;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct ProgressBar {
    total: usize,
    width: usize,
    last_frame: usize,
    last_update: Instant,
}

impl ProgressBar {
    const MIN_TOTAL_FRAMES: usize = 1;
    const UPDATE_FRAME_INTERVAL: usize = 60;
    const UPDATE_TIME_INTERVAL: Duration = Duration::from_millis(100);

    fn new(total: usize) -> Self {
        Self {
            total: total.max(Self::MIN_TOTAL_FRAMES),
            width: 40,
            last_frame: 0,
            last_update: Instant::now(),
        }
    }

    fn update(&mut self, frame: usize) {
        let now = Instant::now();
        let force = frame == 0
            || frame == self.total
            || frame.saturating_sub(self.last_frame) >= Self::UPDATE_FRAME_INTERVAL;
        if !force && now.duration_since(self.last_update) < Self::UPDATE_TIME_INTERVAL {
            return;
        }
        self.last_frame = frame;
        self.last_update = now;
        let bar = self.format_bar(frame);
        print!("\r{bar}");
        let _ = stdout().flush();
    }

    fn format_bar(&self, frame: usize) -> String {
        let clamped_frame = frame.min(self.total);
        let ratio = clamped_frame as f32 / self.total as f32;
        let filled = (ratio * self.width as f32).round() as usize;
        let empty = self.width.saturating_sub(filled);
        let percent = (ratio * 100.0).round() as u32;
        let (elapsed, total) = format_time_pair(clamped_frame, self.total);
        format!(
            "[{}{}] {:3}% ({} / {})",
            "#".repeat(filled),
            "-".repeat(empty),
            percent,
            elapsed,
            total
        )
    }

    fn finish(&mut self) {
        self.update(self.total);
        println!();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Record,
    Playback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitReason {
    Continue,
    UserRequested,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let Some((mode, rom_path, config, headless, overwrite_recording, extend)) = parse_args(&args)?
    else {
        return Ok(());
    };

    let mut nes = Nes::new(config.tv_system);
    let cart = Cartridge::load_from_file(&rom_path)?;
    nes.insert_cartridge(cart);
    nes.reset(false);

    let autorun_path = autorun_path_for_rom(&rom_path);
    let mut state = RunnerState::new_with_extend(mode, autorun_path, overwrite_recording, extend)?;

    if headless {
        run_headless_loop(&mut nes, mode, &mut state)?;
    } else {
        let sdl_context = sdl2::init()?;
        let mut event_pump = sdl_context.event_pump()?;
        let mut gl_backend = SdlGlBackend::new(&sdl_context, &config)?;
        state.init_gamepads(&sdl_context)?;

        run_loop(&mut nes, &mut gl_backend, &mut event_pump, mode, &mut state)?;
    }

    Ok(())
}

type ParseArgsResult = Result<Option<(Mode, PathBuf, Config, bool, bool, bool)>, String>;

fn parse_args(args: &[String]) -> ParseArgsResult {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(None);
    }

    let mode = if args.iter().any(|arg| arg == "--record") {
        Mode::Record
    } else if args.iter().any(|arg| arg == "--playback") {
        Mode::Playback
    } else {
        return Err("Missing --record or --playback argument".to_string());
    };

    let headless = args.iter().any(|arg| arg == "--headless");
    let overwrite_recording = args.iter().any(|arg| arg == "--overwrite-recording");
    let extend = args.iter().any(|arg| arg == "--extend");
    if headless && mode == Mode::Record {
        return Err("Headless mode is only supported for --playback".to_string());
    }
    if extend && mode != Mode::Record {
        return Err("--extend is only supported for --record".to_string());
    }

    let filtered_args: Vec<String> = args
        .iter()
        .filter(|arg| {
            *arg != "--record"
                && *arg != "--playback"
                && *arg != "--headless"
                && *arg != "--overwrite-recording"
                && *arg != "--extend"
        })
        .cloned()
        .collect();

    let config = match Config::new(&filtered_args)? {
        ParseResult::Config(config) => config,
        ParseResult::Help => {
            print_help();
            return Ok(None);
        }
    };

    let rom_path = config
        .rom_path
        .clone()
        .ok_or_else(|| "Missing ROM path argument".to_string())?;

    Ok(Some((
        mode,
        PathBuf::from(rom_path),
        config,
        headless,
        overwrite_recording,
        extend,
    )))
}

fn print_help() {
    println!("NESER autorunner");
    println!("\nUsage: autorunner [--record | --playback] [OPTIONS] <ROM>");
    println!("\nOptions:");
    println!("  --record           Record joypad input to <ROM>.autorun");
    println!("  --playback         Play back joypad input from <ROM>.autorun");
    println!("  --headless         Run playback without a window (requires --playback)");
    println!("  --overwrite-recording  Replace existing autorun recording");
    println!("  --extend           Extend an existing autorun recording");
    println!("  --pal              Use PAL TV system (default: NTSC)");
    println!("  --no-vsync         Disable VSync (default: enabled)");
    println!("  --window-height <N>  Window height in pixels (default: 960)");
    println!("  --fullscreen       Run emulator in fullscreen mode");
    println!("  --display <N>      Select display index for fullscreen");
    println!("  --filter <path>    Specify shader preset path");
}

struct RunnerState {
    autorun: AutorunFile,
    autorun_path: PathBuf,
    controller_player_map: HashMap<u32, u8>,
    _controllers: Vec<sdl2::controller::GameController>,
    frame_index: usize,
    extending_playback: bool,
}

impl RunnerState {
    fn new(mode: Mode, autorun_path: PathBuf, overwrite_recording: bool) -> Result<Self, String> {
        let autorun = match mode {
            Mode::Record => {
                if autorun_path.exists() {
                    if overwrite_recording {
                        fs::remove_file(&autorun_path).map_err(|e| {
                            format!(
                                "Failed to remove existing recording {}: {e}",
                                autorun_path.display()
                            )
                        })?;
                    } else {
                        return Err(format!(
                            "Recording already exists: {} (use --overwrite-recording to replace)",
                            autorun_path.display()
                        ));
                    }
                }
                AutorunFile {
                    version: AUTORUN_VERSION,
                    frames: Vec::new(),
                    checksum: 0,
                }
            }
            Mode::Playback => load_autorun_file(&autorun_path)?,
        };

        Ok(Self {
            autorun,
            autorun_path,
            controller_player_map: HashMap::new(),
            _controllers: Vec::new(),
            frame_index: 0,
            extending_playback: false,
        })
    }

    fn new_with_extend(
        mode: Mode,
        autorun_path: PathBuf,
        overwrite_recording: bool,
        extend: bool,
    ) -> Result<Self, String> {
        if mode == Mode::Record && extend {
            if autorun_path.exists() {
                let autorun = load_autorun_file(&autorun_path)?;
                return Ok(Self {
                    autorun,
                    autorun_path,
                    controller_player_map: HashMap::new(),
                    _controllers: Vec::new(),
                    frame_index: 0,
                    extending_playback: true,
                });
            }

            return Ok(Self {
                autorun: AutorunFile {
                    version: AUTORUN_VERSION,
                    frames: Vec::new(),
                    checksum: 0,
                },
                autorun_path,
                controller_player_map: HashMap::new(),
                _controllers: Vec::new(),
                frame_index: 0,
                extending_playback: false,
            });
        }

        Self::new(mode, autorun_path, overwrite_recording)
    }

    fn init_gamepads(&mut self, sdl_context: &sdl2::Sdl) -> Result<(), String> {
        let (controllers, controller_player_map) = init_gamepads(sdl_context)?;
        self.controller_player_map = controller_player_map;
        self._controllers = controllers;
        Ok(())
    }

    fn record_frame(&mut self, player1: u8, player2: u8) {
        self.autorun.frames.push(AutorunFrame { player1, player2 });
    }

    fn next_playback_frame(&mut self) -> Option<AutorunFrame> {
        let frame = self.autorun.frames.get(self.frame_index).cloned();
        if frame.is_some() {
            self.frame_index += 1;
        }
        frame
    }
}

fn run_loop(
    nes: &mut Nes,
    gl_backend: &mut neser::sdl_frontend::SdlGlBackend,
    event_pump: &mut sdl2::EventPump,
    mode: Mode,
    state: &mut RunnerState,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut player1 = 0u8;
    let mut player2 = 0u8;
    let mut last_frame_crc = 0u32;
    let total_frames = state.autorun.frames.len();

    loop {
        let events: Vec<_> = event_pump.poll_iter().collect();
        for event in events {
            gl_backend.handle_event(&event);
            if handle_event(nes, mode, state, &mut player1, &mut player2, event)?
                == ExitReason::UserRequested
            {
                finalize_run(mode, state, last_frame_crc, nes, true)?;
                return Ok(());
            }
        }

        match mode {
            Mode::Record => {
                process_record_mode(nes, state, &mut player1, &mut player2);
            }
            Mode::Playback => {
                if let Some(frame) = state.next_playback_frame() {
                    apply_buttons(nes, frame.player1, frame.player2);
                    player1 = frame.player1;
                    player2 = frame.player2;
                } else {
                    finalize_run(mode, state, last_frame_crc, nes, false)?;
                    return Ok(());
                }
            }
        }

        while !nes.is_ready_to_render() {
            nes.run_cpu_tick();
            while nes.sample_ready() {
                nes.get_sample();
            }
        }

        nes.clear_ready_to_render();
        last_frame_crc = frame_checksum(nes);

        let overlay_text = match mode {
            Mode::Playback => Some(playback_overlay_text(state.frame_index, total_frames)),
            Mode::Record => {
                let current_frames = if state.extending_playback {
                    state.frame_index
                } else {
                    state.autorun.frames.len()
                };
                if state.extending_playback {
                    Some(record_overlay_text_with_total(
                        current_frames,
                        total_frames,
                        true,
                    ))
                } else {
                    Some(record_overlay_text_with_mode(current_frames, false))
                }
            }
        };
        let overlay_blink_red = matches!(mode, Mode::Record)
            && state.extending_playback
            && extend_playback_blink_red(state.frame_index, total_frames);
        let _ = gl_backend.render(nes, false, overlay_text.as_deref(), overlay_blink_red);
    }
}

fn run_headless_loop(
    nes: &mut Nes,
    mode: Mode,
    state: &mut RunnerState,
) -> Result<(), Box<dyn std::error::Error>> {
    let total_frames = state.autorun.frames.len();
    let mut progress = ProgressBar::new(total_frames);
    let mut last_frame_crc = 0u32;

    progress.update(0);

    loop {
        if let Some(frame) = state.next_playback_frame() {
            apply_buttons(nes, frame.player1, frame.player2);
        } else {
            progress.finish();
            finalize_run(mode, state, last_frame_crc, nes, false)?;
            return Ok(());
        }

        while !nes.is_ready_to_render() {
            nes.run_cpu_tick();
            while nes.sample_ready() {
                nes.get_sample();
            }
        }

        nes.clear_ready_to_render();
        last_frame_crc = frame_checksum(nes);
        progress.update(state.frame_index);
    }
}

fn handle_event(
    nes: &mut Nes,
    mode: Mode,
    state: &mut RunnerState,
    player1: &mut u8,
    player2: &mut u8,
    event: Event,
) -> Result<ExitReason, Box<dyn std::error::Error>> {
    match event {
        Event::Quit { .. } => return Ok(ExitReason::UserRequested),
        Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return Ok(ExitReason::UserRequested),
        Event::KeyDown {
            keycode: Some(keycode),
            repeat: false,
            ..
        } => {
            if mode == Mode::Record
                && let Some(button) = map_key_to_button(keycode)
            {
                if state.extending_playback {
                    update_input_state(1, button, true, player1, player2);
                } else {
                    apply_button_change(nes, player1, player2, button, true);
                }
            }
        }
        Event::KeyUp {
            keycode: Some(keycode),
            ..
        } => {
            if mode == Mode::Record
                && let Some(button) = map_key_to_button(keycode)
            {
                if state.extending_playback {
                    update_input_state(1, button, false, player1, player2);
                } else {
                    apply_button_change(nes, player1, player2, button, false);
                }
            }
        }
        Event::ControllerButtonDown { which, button, .. } => {
            if mode == Mode::Record
                && let Some(player) = state.controller_player_map.get(&which).copied()
                && let Some(nes_button) = map_controller_button(button)
            {
                if state.extending_playback {
                    update_input_state(player, nes_button, true, player1, player2);
                } else {
                    set_button_state(nes, player, nes_button, true, player1, player2);
                }
            }
        }
        Event::ControllerButtonUp { which, button, .. } => {
            if mode == Mode::Record
                && let Some(player) = state.controller_player_map.get(&which).copied()
                && let Some(nes_button) = map_controller_button(button)
            {
                if state.extending_playback {
                    update_input_state(player, nes_button, false, player1, player2);
                } else {
                    set_button_state(nes, player, nes_button, false, player1, player2);
                }
            }
        }
        _ => {}
    }

    Ok(ExitReason::Continue)
}

fn map_key_to_button(keycode: Keycode) -> Option<Button> {
    match keycode {
        Keycode::W => Some(Button::Up),
        Keycode::S => Some(Button::Down),
        Keycode::A => Some(Button::Left),
        Keycode::D => Some(Button::Right),
        Keycode::G => Some(Button::B),
        Keycode::F => Some(Button::A),
        Keycode::R => Some(Button::Select),
        Keycode::T => Some(Button::Start),
        _ => None,
    }
}

fn map_controller_button(button: sdl2::controller::Button) -> Option<Button> {
    match button {
        sdl2::controller::Button::DPadUp => Some(Button::Up),
        sdl2::controller::Button::DPadDown => Some(Button::Down),
        sdl2::controller::Button::DPadLeft => Some(Button::Left),
        sdl2::controller::Button::DPadRight => Some(Button::Right),
        sdl2::controller::Button::A => Some(Button::A),
        sdl2::controller::Button::B => Some(Button::B),
        sdl2::controller::Button::X => Some(Button::A),
        sdl2::controller::Button::Y => Some(Button::B),
        sdl2::controller::Button::Back => Some(Button::Select),
        sdl2::controller::Button::Start => Some(Button::Start),
        _ => None,
    }
}

fn init_gamepads(
    sdl_context: &sdl2::Sdl,
) -> Result<(Vec<sdl2::controller::GameController>, HashMap<u32, u8>), String> {
    let game_controller_subsystem = sdl_context.game_controller()?;
    let num = game_controller_subsystem
        .load_mappings("gamecontrollerdb.txt")
        .unwrap_or(0);
    println!("Loaded {} game controller mappings", num);

    let available = game_controller_subsystem
        .num_joysticks()
        .map_err(|e| format!("Failed to enumerate joysticks: {}", e))?;

    let mut controllers = Vec::new();
    let mut controller_player_map = HashMap::new();

    for id in 0..available.min(2) {
        if !game_controller_subsystem.is_game_controller(id) {
            continue;
        }

        match game_controller_subsystem.open(id) {
            Ok(controller) => {
                let instance_id = controller.instance_id();
                let player_num = (controllers.len() + 1) as u8;
                controller_player_map.insert(instance_id, player_num);
                controllers.push(controller);
            }
            Err(e) => {
                println!("Failed to open controller {}: {}", id, e);
            }
        }
    }

    Ok((controllers, controller_player_map))
}

fn format_time_pair(current_frames: usize, total_frames: usize) -> (String, String) {
    const FPS: usize = 60;
    let current_secs = current_frames / FPS;
    let total_secs = total_frames / FPS;
    (format_mm_ss(current_secs), format_mm_ss(total_secs))
}

fn format_mm_ss(seconds: usize) -> String {
    let minutes = seconds / 60;
    let secs = seconds % 60;
    format!("{minutes:02}:{secs:02}")
}

fn playback_overlay_text(current_frames: usize, total_frames: usize) -> String {
    let (elapsed, total) = format_time_pair(current_frames, total_frames);
    format!("Playback\n{elapsed} / {total}")
}

#[cfg(test)]
fn record_overlay_text(current_frames: usize) -> String {
    record_overlay_text_with_mode(current_frames, false)
}

fn record_overlay_text_with_mode(current_frames: usize, playback: bool) -> String {
    let (elapsed, _) = format_time_pair(current_frames, current_frames);
    let label = if playback { "Playback" } else { "Recording" };
    format!("{label}\n{elapsed} / {elapsed}")
}

fn record_overlay_text_with_total(
    current_frames: usize,
    total_frames: usize,
    playback: bool,
) -> String {
    let (elapsed, total) = format_time_pair(current_frames, total_frames);
    let label = if playback { "Playback" } else { "Recording" };
    format!("{label}\n{elapsed} / {total}")
}

fn extend_playback_blink_red(current_frames: usize, total_frames: usize) -> bool {
    const BLINK_WINDOW_FRAMES: usize = 60 * 3;
    const BLINK_HALF_PERIOD_FRAMES: usize = 15;
    let frames_remaining = total_frames.saturating_sub(current_frames);
    if frames_remaining > BLINK_WINDOW_FRAMES {
        return false;
    }
    (current_frames / BLINK_HALF_PERIOD_FRAMES).is_multiple_of(2)
}

fn apply_button_change(
    nes: &mut Nes,
    player1: &mut u8,
    player2: &mut u8,
    button: Button,
    pressed: bool,
) {
    set_button_state(nes, 1, button, pressed, player1, player2);
}

fn set_button_state(
    nes: &mut Nes,
    player: u8,
    button: Button,
    pressed: bool,
    player1: &mut u8,
    player2: &mut u8,
) {
    nes.set_button(player, button, pressed);
    update_input_state(player, button, pressed, player1, player2);
}

fn update_input_state(
    player: u8,
    button: Button,
    pressed: bool,
    player1: &mut u8,
    player2: &mut u8,
) {
    let mask = 1u8 << button as u8;
    let state = if player == 1 { player1 } else { player2 };
    if pressed {
        *state |= mask;
    } else {
        *state &= !mask;
    }
}

fn apply_buttons(nes: &mut Nes, player1: u8, player2: u8) {
    for button in all_buttons() {
        let mask = 1u8 << button as u8;
        nes.set_button(1, button, player1 & mask != 0);
        nes.set_button(2, button, player2 & mask != 0);
    }
}

fn process_record_mode(nes: &mut Nes, state: &mut RunnerState, player1: &mut u8, player2: &mut u8) {
    if state.extending_playback {
        if let Some(frame) = state.next_playback_frame() {
            apply_buttons(nes, frame.player1, frame.player2);
        } else {
            state.extending_playback = false;
            clear_joypad_state(nes);
            apply_buttons(nes, *player1, *player2);
            state.record_frame(*player1, *player2);
        }
    } else {
        state.record_frame(*player1, *player2);
    }
}

fn clear_joypad_state(nes: &mut Nes) {
    for button in all_buttons() {
        nes.set_button(1, button, false);
        nes.set_button(2, button, false);
    }
}

fn all_buttons() -> [Button; 8] {
    [
        Button::A,
        Button::B,
        Button::Select,
        Button::Start,
        Button::Up,
        Button::Down,
        Button::Left,
        Button::Right,
    ]
}

fn frame_checksum(nes: &Nes) -> u32 {
    let screen_buffer = nes.get_screen_buffer();
    let mut buffer = vec![0u8; 256 * 240 * 3];
    screen_buffer.copy_buffer(&mut buffer);
    crc32(&buffer)
}

fn finalize_run(
    mode: Mode,
    state: &mut RunnerState,
    last_frame_crc: u32,
    _nes: &Nes,
    interrupted_by_user: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        Mode::Record => {
            state.autorun.checksum = last_frame_crc;
            save_autorun_file(&state.autorun_path, &state.autorun).map_err(|e| e.to_string())?;
            println!("Autorun recorded to {}", state.autorun_path.display());
        }
        Mode::Playback => {
            if interrupted_by_user {
                println!("Playback interrupted by user");
                return Ok(());
            }
            if state.autorun.checksum != last_frame_crc {
                return Err(format!(
                    "Checksum mismatch: expected {:08X} got {:08X}",
                    state.autorun.checksum, last_frame_crc
                )
                .into());
            }
            println!("Autorun checksum matched {:08X}", last_frame_crc);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neser::console::TvSystem;
    use tempfile::tempdir;

    fn parse_args_for_test(args: &[&str]) -> Result<(Mode, PathBuf, Config), String> {
        let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        match parse_args(&args)? {
            Some((mode, rom_path, config, _headless, _overwrite, _extend)) => {
                Ok((mode, rom_path, config))
            }
            None => Err("Expected arguments to return config".to_string()),
        }
    }

    #[test]
    fn test_progress_bar_formats_time() {
        let progress = ProgressBar::new(120 * 60);
        let text = progress.format_bar(90 * 60);

        assert!(text.contains("01:30"));
    }

    #[test]
    fn test_playback_overlay_formats_time_pair() {
        let text = playback_overlay_text(90 * 60, 120 * 60);

        assert_eq!(text, "Playback\n01:30 / 02:00");
    }

    #[test]
    fn test_record_overlay_formats_time_pair() {
        let text = record_overlay_text(90 * 60);

        assert_eq!(text, "Recording\n01:30 / 01:30");
    }

    #[test]
    fn test_parse_args_allows_headless_playback() {
        let (mode, rom_path, _config) =
            parse_args_for_test(&["autorunner", "--playback", "--headless", "roms/test.nes"])
                .unwrap();
        assert_eq!(mode, Mode::Playback);
        assert_eq!(rom_path, PathBuf::from("roms/test.nes"));
    }

    #[test]
    fn test_parse_args_rejects_headless_record() {
        let args = vec![
            "autorunner".to_string(),
            "--record".to_string(),
            "--headless".to_string(),
            "roms/test.nes".to_string(),
        ];
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_recording_errors_when_existing_without_overwrite() {
        let temp = tempdir().expect("temp dir");
        let rom_path = temp.path().join("test.nes");
        let autorun_path = autorun_path_for_rom(&rom_path);
        fs::write(&autorun_path, b"existing").expect("write autorun");

        let result = RunnerState::new(Mode::Record, autorun_path, false);
        match result {
            Ok(_) => panic!("should error without overwrite"),
            Err(err) => assert!(err.contains("overwrite")),
        }
    }

    #[test]
    fn test_recording_overwrites_existing_when_flag_set() {
        let temp = tempdir().expect("temp dir");
        let rom_path = temp.path().join("test.nes");
        let autorun_path = autorun_path_for_rom(&rom_path);
        fs::write(&autorun_path, b"existing").expect("write autorun");

        let state = RunnerState::new(Mode::Record, autorun_path.clone(), true)
            .expect("should allow overwrite");
        assert!(!autorun_path.exists());
        assert!(state.autorun.frames.is_empty());
    }

    #[test]
    fn test_extend_flag_no_existing_recording_behaves_like_record() {
        let temp = tempdir().expect("temp dir");
        let rom_path = temp.path().join("test.nes");
        let autorun_path = autorun_path_for_rom(&rom_path);

        let result = RunnerState::new_with_extend(Mode::Record, autorun_path.clone(), false, true);
        match result {
            Ok(state) => {
                assert!(state.autorun.frames.is_empty());
                assert!(!state.extending_playback);
            }
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn test_extend_flag_existing_recording_starts_with_playback() {
        let temp = tempdir().expect("temp dir");
        let rom_path = temp.path().join("test.nes");
        let autorun_path = autorun_path_for_rom(&rom_path);
        let autorun = AutorunFile {
            version: AUTORUN_VERSION,
            frames: vec![AutorunFrame {
                player1: 1,
                player2: 2,
            }],
            checksum: 0,
        };
        save_autorun_file(&autorun_path, &autorun).expect("write autorun");

        let result = RunnerState::new_with_extend(Mode::Record, autorun_path.clone(), false, true);
        match result {
            Ok(state) => {
                assert_eq!(state.autorun.frames.len(), 1);
                assert!(state.extending_playback);
            }
            Err(err) => panic!("unexpected error: {err}"),
        }
    }

    #[test]
    fn test_extend_overlay_label_switches_between_playback_and_recording() {
        let playback_text = record_overlay_text_with_mode(0, true);
        assert!(playback_text.starts_with("Playback\n"));

        let recording_text = record_overlay_text_with_mode(0, false);
        assert!(recording_text.starts_with("Recording\n"));
    }

    #[test]
    fn test_extend_playback_overlay_uses_total_recording_length() {
        let text = record_overlay_text_with_total(30 * 60, 120 * 60, true);
        assert_eq!(text, "Playback\n00:30 / 02:00");
    }

    #[test]
    fn test_extend_transition_applies_live_inputs() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        apply_buttons(&mut nes, 1u8 << Button::A as u8, 0);

        let mut state = RunnerState {
            autorun: AutorunFile {
                version: AUTORUN_VERSION,
                frames: Vec::new(),
                checksum: 0,
            },
            autorun_path: PathBuf::from("test.autorun"),
            controller_player_map: HashMap::new(),
            _controllers: Vec::new(),
            frame_index: 0,
            extending_playback: true,
        };
        let mut player1 = 1u8 << Button::B as u8;
        let mut player2 = 0u8;

        process_record_mode(&mut nes, &mut state, &mut player1, &mut player2);

        let state_after = nes.save_state();
        assert_eq!(
            state_after.bus.joypad1.button_states,
            1u8 << Button::B as u8
        );
        assert_eq!(state_after.bus.joypad2.button_states, 0);
        assert_eq!(player1, 1u8 << Button::B as u8);
        assert_eq!(player2, 0);
    }

    #[test]
    fn test_extend_transition_records_live_input() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut state = RunnerState {
            autorun: AutorunFile {
                version: AUTORUN_VERSION,
                frames: Vec::new(),
                checksum: 0,
            },
            autorun_path: PathBuf::from("test.autorun"),
            controller_player_map: HashMap::new(),
            _controllers: Vec::new(),
            frame_index: 0,
            extending_playback: true,
        };
        let mut player1 = 1u8 << Button::A as u8;
        let mut player2 = 1u8 << Button::B as u8;

        process_record_mode(&mut nes, &mut state, &mut player1, &mut player2);

        assert!(!state.extending_playback);
        assert_eq!(player1, 1u8 << Button::A as u8);
        assert_eq!(player2, 1u8 << Button::B as u8);
        assert_eq!(state.autorun.frames.len(), 1);
        let frame = &state.autorun.frames[0];
        assert_eq!(frame.player1, 1u8 << Button::A as u8);
        assert_eq!(frame.player2, 1u8 << Button::B as u8);
    }

    #[test]
    fn test_extend_playback_blink_active_only_in_last_three_seconds() {
        assert!(!extend_playback_blink_red(0, 500));
        assert!(extend_playback_blink_red(0, 180));
    }

    #[test]
    fn test_extend_playback_blink_toggles() {
        assert!(extend_playback_blink_red(0, 180));
        assert!(!extend_playback_blink_red(15, 180));
    }
}
