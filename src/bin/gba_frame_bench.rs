use neser::gba::Gba;
use neser::platform::app_context::AppContext;
use neser::platform::config::Config;
use neser::platform::emulator::Emulator;
use neser::platform::frame_benchmark::{
    FrameBenchmarkConfig, FrameBenchmarkConfigError, FrameTimingStats,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn main() {
    if let Err(err) = run(std::env::args().collect()) {
        eprintln!("{err}");
        eprintln!("{}", usage());
        std::process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let bench_config = FrameBenchmarkConfig::parse_args(&args).map_err(config_error_message)?;
    let rom_data = std::fs::read(&bench_config.rom_path)
        .map_err(|err| format!("failed to read ROM '{}': {err}", bench_config.rom_path))?;

    let mut gba = create_loaded_gba(&rom_data, &bench_config)?;
    for _ in 0..bench_config.warmup_frames {
        run_one_frame(&mut gba);
    }

    let samples = run_measured_frames(&mut gba, bench_config.frames);
    let stats = FrameTimingStats::from_samples(&samples)
        .map_err(|err| format!("failed to compute benchmark stats: {err:?}"))?;

    print_summary("Primary run", &bench_config, &stats);

    if bench_config.stability_runs > 0 {
        println!(
            "\nStability check ({} run(s) of {} frames):",
            bench_config.stability_runs, bench_config.frames
        );
        for run_index in 1..=bench_config.stability_runs {
            let samples = if bench_config.reset_stability_runs {
                let mut stability_gba = create_loaded_gba(&rom_data, &bench_config)?;
                for _ in 0..bench_config.warmup_frames {
                    run_one_frame(&mut stability_gba);
                }
                run_measured_frames(&mut stability_gba, bench_config.frames)
            } else {
                run_measured_frames(&mut gba, bench_config.frames)
            };
            let stats = FrameTimingStats::from_samples(&samples)
                .map_err(|err| format!("failed to compute benchmark stats: {err:?}"))?;
            println!(
                "  Run {run_index}: avg={:.3}ms p95={:.3}ms max={:.3}ms fps={:.1}",
                stats.average_ms, stats.p95_ms, stats.max_ms, stats.fps
            );
        }
    }

    Ok(())
}

fn create_loaded_gba(rom_data: &[u8], bench_config: &FrameBenchmarkConfig) -> Result<Gba, String> {
    let mut gba = create_gba(bench_config.skip_gba_bios_intro);
    gba.load_rom(rom_data, &bench_config.rom_path)
        .map_err(|err| format!("failed to load GBA ROM '{}': {err}", bench_config.rom_path))?;
    gba.set_audio_sample_rate(44_100.0);
    Ok(gba)
}

fn create_gba(skip_bios_intro: bool) -> Gba {
    let mut config = Config::default();
    config.gba.bios_path = Some("embedded".to_string());
    config.gba.skip_bios_intro = skip_bios_intro;
    let app_context = Rc::new(RefCell::new(AppContext::new_with_config(config)));
    Gba::new(app_context)
}

fn run_measured_frames(gba: &mut Gba, frames: usize) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(frames);
    for _ in 0..frames {
        let start = Instant::now();
        run_one_frame(gba);
        samples.push(start.elapsed());
    }
    samples
}

fn run_one_frame(gba: &mut Gba) {
    while !gba.is_ready_to_render() {
        let _cycles = gba.run_tick();
        while gba.get_stereo_sample().is_some() {}
    }
    gba.clear_ready_to_render();
}

fn print_summary(label: &str, bench_config: &FrameBenchmarkConfig, stats: &FrameTimingStats) {
    println!("{label}");
    println!("ROM: {}", bench_config.rom_path);
    println!("Frames: {}", stats.frames);
    println!("Warmup frames: {}", bench_config.warmup_frames);
    println!("Skip GBA BIOS intro: {}", bench_config.skip_gba_bios_intro);
    println!(
        "Reset stability runs: {}",
        bench_config.reset_stability_runs
    );
    println!("Total: {:.1}ms", stats.total.as_secs_f64() * 1000.0);
    println!("Average: {:.3}ms/frame", stats.average_ms);
    println!("P50: {:.3}ms/frame", stats.p50_ms);
    println!("P95: {:.3}ms/frame", stats.p95_ms);
    println!("Max: {:.3}ms/frame", stats.max_ms);
    println!("FPS: {:.1}", stats.fps);
}

fn usage() -> &'static str {
    "Usage: gba_frame_bench <rom_path> [--frames N] [--warmup N] [--stability-runs N] [--include-bios-intro] [--continue-stability-runs]\n\
\n\
Benchmarks GBA frame execution using the embedded BIOS. By default the GBA BIOS intro is skipped,\n\
matching the Metroid Zero Mission title/intro benchmark plan. Stability runs reload the ROM by\n\
default so each run measures the same title/intro window."
}

fn config_error_message(err: FrameBenchmarkConfigError) -> String {
    match err {
        FrameBenchmarkConfigError::MissingRomPath => "missing ROM path".to_string(),
        FrameBenchmarkConfigError::MissingValue { name } => {
            format!("missing value for --{name}")
        }
        FrameBenchmarkConfigError::InvalidNumber { name, value } => {
            format!("invalid value for --{name}: '{value}'")
        }
        FrameBenchmarkConfigError::UnknownArgument(arg) => {
            format!("unknown argument: {arg}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_documents_primary_gba_benchmark_options() {
        let usage = usage();

        assert!(usage.contains("Usage: gba_frame_bench <rom_path>"));
        assert!(usage.contains("--frames"));
        assert!(usage.contains("--warmup"));
        assert!(usage.contains("--stability-runs"));
        assert!(usage.contains("--include-bios-intro"));
        assert!(usage.contains("--continue-stability-runs"));
    }
}
