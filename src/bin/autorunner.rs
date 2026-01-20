use neser::autorun::{
    AUTORUN_VERSION, AutorunFile, AutorunFrame, autorun_path_for_rom, crc32, load_autorun_file,
    save_autorun_file,
};
use neser::cartridge::Cartridge;
use neser::config::{Config, ParseResult};
use neser::input::Button;
use neser::nes::Nes;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashMap;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let Some((mode, rom_path, config, headless)) = parse_args(&args)? else {
        return Ok(());
    };

    let mut nes = Nes::new(config.tv_system);
    let cart = Cartridge::load_from_file(&rom_path)?;
    nes.insert_cartridge(cart);
    nes.reset(false);

    let autorun_path = autorun_path_for_rom(&rom_path);
    let mut state = RunnerState::new(mode, autorun_path)?;

    if headless {
        run_headless_loop(&mut nes, mode, &mut state)?;
    } else {
        let sdl_context = sdl2::init()?;
        let mut event_pump = sdl_context.event_pump()?;
        let mut gl_backend = neser::rendering::GlBackend::new(&sdl_context, &config)?;
        state.init_gamepads(&sdl_context)?;

        run_loop(&mut nes, &mut gl_backend, &mut event_pump, mode, &mut state)?;
    }

    Ok(())
}

fn parse_args(args: &[String]) -> Result<Option<(Mode, PathBuf, Config, bool)>, String> {
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
    if headless && mode == Mode::Record {
        return Err("Headless mode is only supported for --playback".to_string());
    }

    let filtered_args: Vec<String> = args
        .iter()
        .filter(|arg| *arg != "--record" && *arg != "--playback" && *arg != "--headless")
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

    Ok(Some((mode, PathBuf::from(rom_path), config, headless)))
}

fn print_help() {
    println!("NESER autorunner");
    println!("\nUsage: autorunner [--record | --playback] [OPTIONS] <ROM>");
    println!("\nOptions:");
    println!("  --record           Record joypad input to <ROM>.autorun");
    println!("  --playback         Play back joypad input from <ROM>.autorun");
    println!("  --headless         Run playback without a window (requires --playback)");
    println!("  --pal              Use PAL TV system (default: NTSC)");
    println!("  --no-vsync         Disable VSync (default: enabled)");
    println!("  --video-scale <N>  Window scaling factor (default: 4.0)");
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
}

impl RunnerState {
    fn new(mode: Mode, autorun_path: PathBuf) -> Result<Self, String> {
        let autorun = match mode {
            Mode::Record => AutorunFile {
                version: AUTORUN_VERSION,
                frames: Vec::new(),
                checksum: 0,
            },
            Mode::Playback => load_autorun_file(&autorun_path)?,
        };

        Ok(Self {
            autorun,
            autorun_path,
            controller_player_map: HashMap::new(),
            _controllers: Vec::new(),
            frame_index: 0,
        })
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
    gl_backend: &mut neser::rendering::GlBackend,
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
            if handle_event(nes, mode, state, &mut player1, &mut player2, event)? {
                finalize_run(mode, state, last_frame_crc, nes)?;
                return Ok(());
            }
        }

        match mode {
            Mode::Record => {
                state.record_frame(player1, player2);
            }
            Mode::Playback => {
                if let Some(frame) = state.next_playback_frame() {
                    apply_buttons(nes, frame.player1, frame.player2);
                    player1 = frame.player1;
                    player2 = frame.player2;
                } else {
                    finalize_run(mode, state, last_frame_crc, nes)?;
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

        let overlay_text = if mode == Mode::Playback {
            Some(playback_overlay_text(state.frame_index, total_frames))
        } else {
            None
        };
        let _ = gl_backend.render(nes, false, overlay_text.as_deref());
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
            finalize_run(mode, state, last_frame_crc, nes)?;
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
) -> Result<bool, Box<dyn std::error::Error>> {
    match event {
        Event::Quit { .. } => return Ok(true),
        Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return Ok(true),
        Event::KeyDown {
            keycode: Some(keycode),
            repeat: false,
            ..
        } => {
            if mode == Mode::Record {
                if let Some(button) = map_key_to_button(keycode) {
                    apply_button_change(nes, player1, player2, button, true);
                }
            }
        }
        Event::KeyUp {
            keycode: Some(keycode),
            ..
        } => {
            if mode == Mode::Record {
                if let Some(button) = map_key_to_button(keycode) {
                    apply_button_change(nes, player1, player2, button, false);
                }
            }
        }
        Event::ControllerButtonDown { which, button, .. } => {
            if mode == Mode::Record {
                if let Some(player) = state.controller_player_map.get(&which).copied() {
                    if let Some(nes_button) = map_controller_button(button) {
                        set_button_state(nes, player, nes_button, true, player1, player2);
                    }
                }
            }
        }
        Event::ControllerButtonUp { which, button, .. } => {
            if mode == Mode::Record {
                if let Some(player) = state.controller_player_map.get(&which).copied() {
                    if let Some(nes_button) = map_controller_button(button) {
                        set_button_state(nes, player, nes_button, false, player1, player2);
                    }
                }
            }
        }
        _ => {}
    }

    Ok(false)
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
    format!("{elapsed} / {total}")
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
) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        Mode::Record => {
            state.autorun.checksum = last_frame_crc;
            save_autorun_file(&state.autorun_path, &state.autorun).map_err(|e| format!("{e}"))?;
            println!("Autorun recorded to {}", state.autorun_path.display());
        }
        Mode::Playback => {
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

    fn parse_args_for_test(args: &[&str]) -> Result<(Mode, PathBuf, Config), String> {
        let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        match parse_args(&args)? {
            Some((mode, rom_path, config, _headless)) => Ok((mode, rom_path, config)),
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

        assert_eq!(text, "01:30 / 02:00");
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
}
