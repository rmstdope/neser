mod apu;
mod audio;
mod cartridge;
mod cpu;
mod debugger;
mod debugger_ui;
mod eventloop;
mod gl_backend;
mod input;
mod manual_test_cartridges;
mod mem_controller;
mod nes;
mod ppu;
mod screen_buffer;
mod tracing;

struct CliFlag {
    flag: &'static str,
    help: Option<&'static str>,
}

const CLI_FLAGS: &[CliFlag] = &[
    CliFlag {
        flag: "--help",
        help: None,
    },
    CliFlag {
        flag: "-h",
        help: None,
    },
    CliFlag {
        flag: "--pal",
        help: Some("Use PAL TV system (default: NTSC)"),
    },
    CliFlag {
        flag: "--no-audio",
        help: Some("Disable audio output"),
    },
    CliFlag {
        flag: "--trace",
        help: Some("Enable CPU trace output"),
    },
    CliFlag {
        flag: "--trace-nestest",
        help: Some("Enable CPU trace output (nestest.log format)"),
    },
    CliFlag {
        flag: "--trace-ppu",
        help: Some("Enable PPU trace output"),
    },
    CliFlag {
        flag: "--trace-apu",
        help: Some("Enable APU trace output"),
    },
    CliFlag {
        flag: "--disable-pulse1",
        help: Some("Mute pulse 1 channel"),
    },
    CliFlag {
        flag: "--disable-pulse2",
        help: Some("Mute pulse 2 channel"),
    },
    CliFlag {
        flag: "--disable-triangle",
        help: Some("Mute triangle channel"),
    },
    CliFlag {
        flag: "--disable-noise",
        help: Some("Mute noise channel"),
    },
    CliFlag {
        flag: "--disable-dmc",
        help: Some("Mute DMC channel"),
    },
    CliFlag {
        flag: "--no-vsync",
        help: Some("Disable VSync (default: enabled)"),
    },
    CliFlag {
        flag: "--no-gamepads",
        help: Some("Disable gamepad/joystick support"),
    },
    CliFlag {
        flag: "--enable-debugger",
        help: Some("Open debugger windows (CPU/PPU/APU) on startup"),
    },
];

fn vsync_enabled_from_args(args: &[String]) -> bool {
    !args.iter().any(|a| a == "--no-vsync")
}

fn debugger_enabled_from_args(args: &[String]) -> bool {
    args.iter().any(|a| a == "--enable-debugger")
}

fn apply_debugger_startup_config(event_loop: &mut eventloop::EventLoop, args: &[String]) {
    if debugger_enabled_from_args(args) {
        event_loop.request_debugger_open();
    }
}

fn validate_no_unknown_args(args: &[String]) -> Result<(), String> {
    // args[0] is the program name
    for arg in args.iter().skip(1) {
        if CLI_FLAGS.iter().any(|f| f.flag == arg) {
            continue;
        }

        if arg.starts_with('-') {
            return Err(format!(
                "Unknown argument: {arg}\nTry --help for usage.",
                arg = arg
            ));
        }

        return Err(format!(
            "Unexpected positional argument: {arg}\nTry --help for usage.",
            arg = arg
        ));
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    // Show help if requested
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!("NES Emulator");
        println!("\nUsage: neser [OPTIONS]");
        println!("\nOptions:");

        for flag in CLI_FLAGS {
            if let Some(help) = flag.help {
                println!("  {:<19} {}", flag.flag, help);
            }
        }

        println!("\nExample:");
        println!("  neser --disable-pulse2 --disable-triangle    # Only pulse1, noise, and DMC");
        return Ok(());
    }

    if let Err(message) = validate_no_unknown_args(&args) {
        eprintln!("{message}");
        return Err(message.into());
    }

    let tv_system = if args.contains(&"--pal".to_string()) {
        nes::TvSystem::Pal
    } else {
        nes::TvSystem::Ntsc
    };
    let no_audio = args.contains(&"--no-audio".to_string());
    let vsync_enabled = vsync_enabled_from_args(&args);
    let gamepads_enabled = !args.contains(&"--no-gamepads".to_string());
    let tracing = tracing::Tracing::from_args(&args);

    // Channel enable/disable flags (default: all enabled)
    let pulse1_enabled = !args.contains(&"--disable-pulse1".to_string());
    let pulse2_enabled = !args.contains(&"--disable-pulse2".to_string());
    let triangle_enabled = !args.contains(&"--disable-triangle".to_string());
    let noise_enabled = !args.contains(&"--disable-noise".to_string());
    let dmc_enabled = !args.contains(&"--disable-dmc".to_string());

    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let mut nes_instance = nes::Nes::new(tv_system);

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let audio = if no_audio {
        None
    } else {
        let audio = audio::NesAudio::new(&sdl_context, 44100)?;
        let actual_rate = audio.actual_sample_rate() as f32;
        nes_instance.apu.borrow_mut().set_sample_rate(actual_rate);
        Some(audio)
    };

    let mut event_loop = eventloop::EventLoop::new(
        false,
        tv_system,
        4.0,
        1.0,
        vsync_enabled,
        audio,
        gamepads_enabled,
    )?;

    // Palette display requiring only scanline-based palette changes,
    // intended to demonstrate the full palette even on less advanced emulators
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/palette.nes")?;

    // Simple display of any chosen color full-screen
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/color_test.nes")?;

    // Load game cartridge
    // let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let rom_data = std::fs::read("roms/games/Balloon_fight.nes")?;
    // let rom_path = "roms/games/donkey kong.nes";
    let rom_path = "roms/games/zelda.nes";

    // Manual testing of Blargg
    // let rom_path = "roms/blargg/dmc_tests/buffer_retained.nes";

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse1_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse2_only_nrom_128();
    // let rom_data = manual_test_cartridges::noise_only_nrom_128();

    let cart = cartridge::Cartridge::load_from_file(rom_path)?;
    nes_instance.insert_cartridge(cart);

    nes_instance.reset(false);

    // Apply channel enable/disable settings
    {
        let mut apu = nes_instance.apu.borrow_mut();
        apu.set_pulse1_enabled(pulse1_enabled);
        apu.set_pulse2_enabled(pulse2_enabled);
        apu.set_triangle_enabled(triangle_enabled);
        apu.set_noise_enabled(noise_enabled);
        apu.set_dmc_enabled(dmc_enabled);
    }

    let run_result = event_loop.run(&mut nes_instance, tracing);
    // Best-effort save on clean shutdown (Escape/Quit).
    if run_result.is_ok() {
        if let Err(e) = nes_instance.memory.borrow().save_ram() {
            eprintln!("Warning: failed to save RAM: {}", e);
        }
    }

    run_result.map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_vsync_enabled_by_default() {
        let args = vec!["neser".to_string()];
        assert!(vsync_enabled_from_args(&args));
    }

    #[test]
    fn test_no_vsync_flag_disables_vsync() {
        let args = vec!["neser".to_string(), "--no-vsync".to_string()];
        assert!(!vsync_enabled_from_args(&args));
    }

    #[test]
    fn test_unknown_argument_causes_error() {
        let args = vec![
            "neser".to_string(),
            "--definitely-not-a-real-flag".to_string(),
        ];
        assert!(validate_no_unknown_args(&args).is_err());
    }

    #[test]
    fn test_no_gamepads_flag_recognized() {
        let args = vec!["neser".to_string(), "--no-gamepads".to_string()];
        assert!(validate_no_unknown_args(&args).is_ok());
    }

    #[test]
    fn test_enable_debugger_flag_is_accepted() {
        let args = vec!["neser".to_string(), "--enable-debugger".to_string()];
        assert!(validate_no_unknown_args(&args).is_ok());
    }

    #[test]
    fn test_debugger_enabled_from_args() {
        let args = vec!["neser".to_string(), "--enable-debugger".to_string()];
        assert!(debugger_enabled_from_args(&args));

        let args = vec!["neser".to_string()];
        assert!(!debugger_enabled_from_args(&args));
    }

    #[test]
    #[serial]
    fn test_enable_debugger_requests_open_and_pauses_on_start() {
        let args = vec!["neser".to_string(), "--enable-debugger".to_string()];

        let mut event_loop = crate::eventloop::EventLoop::new(
            true,
            crate::nes::TvSystem::Ntsc,
            1.0,
            1.0,
            true,
            None,
            false,
        )
        .unwrap();

        apply_debugger_startup_config(&mut event_loop, &args);

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }
}
