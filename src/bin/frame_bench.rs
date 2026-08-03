use neser::nes::cartridge::Cartridge;
use neser::nes::console::{Config, Nes};
use neser::platform::app_context::AppContext;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = args.next().expect("Usage: frame_bench <rom_path> [frames]");
    let num_frames: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(600);

    let rom_data = std::fs::read(&rom_path).expect("Failed to read ROM file");
    let app_context = Rc::new(RefCell::new(AppContext::new_with_config(Config::default())));
    let mut nes = Nes::new(app_context.clone());
    let cart = Cartridge::load_from_file(&rom_data, &rom_path, Some(nes.rom_db()))
        .expect("Failed to load ROM");
    let rom_timing = cart.rom_timing_mode();
    app_context
        .borrow_mut()
        .config_mut()
        .apply_rom_timing_mode(rom_timing);
    nes = Nes::new(app_context.clone());
    nes.insert_cartridge(cart);
    nes.reset(false);

    for _ in 0..60 {
        run_one_frame(&mut nes);
    }

    let start = Instant::now();
    for _ in 0..num_frames {
        run_one_frame(&mut nes);
    }
    let elapsed = start.elapsed();

    let total_ms = elapsed.as_secs_f64() * 1000.0;
    let per_frame_ms = total_ms / num_frames as f64;
    let fps = num_frames as f64 / elapsed.as_secs_f64();

    println!("ROM: {rom_path}");
    println!("Frames: {num_frames}");
    println!("Total: {total_ms:.1}ms");
    println!("Per frame: {per_frame_ms:.3}ms");
    println!("FPS: {fps:.1}");

    println!("\nStability check (5 runs of {num_frames} frames):");
    for i in 0..5 {
        let run_start = Instant::now();
        for _ in 0..num_frames {
            run_one_frame(&mut nes);
        }
        let run_elapsed = run_start.elapsed();
        let per_frame = run_elapsed.as_secs_f64() * 1000.0 / num_frames as f64;
        println!("  Run {}: {per_frame:.3}ms/frame", i + 1);
    }
}

fn run_one_frame(nes: &mut Nes) {
    while !nes.is_ready_to_render() && !nes.cpu_ref().is_halted() {
        nes.run_cpu_tick();
        while nes.sample_ready() {
            nes.get_sample();
        }
    }
    nes.clear_ready_to_render();
}
