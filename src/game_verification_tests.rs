#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::cartridge::Cartridge;
    use crate::golden_screenshots::{
        GoldenScreenshotPolicy, assert_matches_golden_screenshot_byte_exact,
        ensure_golden_screenshot, golden_screenshot_path_for_rom,
    };
    use crate::manual_test_cartridges;
    use crate::nes::{Nes, TvSystem};

    const FRAMES_TO_RUN: u32 = 60 * 10;

    fn snapshot_screen_buffer_rgb(nes: &Nes) -> Vec<u8> {
        let screen_buffer = nes.get_screen_buffer();
        let expected_len = (screen_buffer.width() * screen_buffer.height() * 3) as usize;

        let mut buffer = vec![0u8; expected_len];
        screen_buffer.copy_buffer(&mut buffer);
        buffer
    }

    fn run_nes_for_frames(nes: &mut Nes, frames: u32) -> Vec<u8> {
        if frames == 0 {
            return snapshot_screen_buffer_rgb(nes);
        }

        // Safety guard: avoid hanging the test suite if something goes wrong.
        // This is deliberately generous; on a healthy emulator we should hit `frames`
        // worth of `ready_to_render` well before this.
        let max_ticks: u64 = 200_000_000;

        let mut frames_completed = 0u32;
        let mut ticks = 0u64;

        while frames_completed < frames {
            nes.run_cpu_tick();
            ticks += 1;
            if ticks > max_ticks {
                panic!(
                    "Timed out running {} frames (only reached {})",
                    frames, frames_completed
                );
            }

            // Drain side channels to avoid unbounded growth.
            while nes.sample_ready() {
                nes.get_sample();
            }

            if nes.is_ready_to_render() {
                frames_completed += 1;
                nes.clear_ready_to_render();
            }
        }

        snapshot_screen_buffer_rgb(nes)
    }

    fn collect_game_roms(games_dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut roms = Vec::new();

        let entries = std::fs::read_dir(games_dir).map_err(|e| {
            format!(
                "Failed to read games directory {}: {e}",
                games_dir.display()
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                format!(
                    "Failed to read directory entry in {}: {e}",
                    games_dir.display()
                )
            })?;
            let path = entry.path();

            let is_nes = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nes"));

            if is_nes {
                roms.push(path);
            }
        }

        roms.sort();
        Ok(roms)
    }

    fn golden_policy_from_env() -> GoldenScreenshotPolicy {
        match std::env::var("NESER_GOLDEN").as_deref() {
            Ok("accept") => GoldenScreenshotPolicy::AutoAccept,
            Ok("reject") => GoldenScreenshotPolicy::AutoReject,
            _ => GoldenScreenshotPolicy::AutoReject,
        }
    }

    fn run_rom_for_frames(rom_path: &Path, frames: u32) -> Result<(Vec<u8>, u32, u32), String> {
        let rom_data = std::fs::read(rom_path)
            .map_err(|e| format!("Failed to load ROM {}: {e}", rom_path.display()))?;
        let cartridge = Cartridge::new(&rom_data)
            .map_err(|e| format!("Failed to parse ROM {}: {e}", rom_path.display()))?;

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let rgb = run_nes_for_frames(&mut nes, frames);
        Ok((
            rgb,
            TvSystem::Ntsc.screen_width(),
            TvSystem::Ntsc.screen_height(),
        ))
    }

    #[test]
    fn test_run_nes_for_frames_returns_rgb_buffer() {
        let rom_data = manual_test_cartridges::triangle_only_nrom_128();
        let cartridge = Cartridge::new(&rom_data).expect("ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let frame = run_nes_for_frames(&mut nes, 2);

        let expected_len =
            (TvSystem::Ntsc.screen_width() * TvSystem::Ntsc.screen_height() * 3) as usize;
        assert_eq!(frame.len(), expected_len);
    }

    #[test]
    fn test_collect_game_roms_filters_and_sorts() {
        use std::time::{SystemTime, UNIX_EPOCH};

        fn unique_temp_dir() -> PathBuf {
            let mut base = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            base.push(format!(
                "neser-game-verification-{}-{}",
                std::process::id(),
                nanos
            ));
            base
        }

        let root = unique_temp_dir();
        let games_dir = root.join("roms").join("games");
        std::fs::create_dir_all(&games_dir).expect("create games dir");

        std::fs::write(games_dir.join("b.nes"), b"rom").unwrap();
        std::fs::write(games_dir.join("a.NES"), b"rom").unwrap();
        std::fs::write(games_dir.join("not_a_rom.txt"), b"x").unwrap();

        let roms = collect_game_roms(&games_dir).expect("collect roms");

        assert_eq!(roms.len(), 2);
        assert!(roms[0].ends_with("a.NES"));
        assert!(roms[1].ends_with("b.nes"));
    }

    /// Manual game verification test:
    /// - Iterates all `roms/games/*.nes`
    /// - Runs each ROM for exactly 10 seconds (600 frames)
    /// - If a golden PNG exists in `roms/games/golden_screenshots`, compares byte-exact
    /// - If missing, fails unless `NESER_GOLDEN=accept` is set, in which case it writes the golden
    #[test]
    #[ignore]
    fn test_verify_games_golden_screenshots() {
        let games_dir = Path::new("roms/games");
        let roms = collect_game_roms(games_dir).expect("collect roms");
        assert!(!roms.is_empty(), "No ROMs found in {}", games_dir.display());

        let policy = golden_policy_from_env();

        let total = roms.len();
        for (idx, rom_path) in roms.into_iter().enumerate() {
            println!(
                "[game verification] ({}/{}) {}",
                idx + 1,
                total,
                rom_path.display()
            );
            let (rgb, width, height) = run_rom_for_frames(&rom_path, FRAMES_TO_RUN)
                .unwrap_or_else(|e| panic!("{}: {e}", rom_path.display()));

            let golden_path = golden_screenshot_path_for_rom(&rom_path);

            if golden_path.exists() {
                assert_matches_golden_screenshot_byte_exact(&rom_path, &rgb, width, height)
                    .unwrap_or_else(|e| panic!("{}: {e}", rom_path.display()));
                println!(
                    "[game verification] PASS - Screenshot matched saved file: {}",
                    golden_path.display()
                );
            } else if policy == GoldenScreenshotPolicy::AutoAccept {
                println!(
                    "[game verification] writing golden to {}",
                    golden_path.display()
                );
                ensure_golden_screenshot(&rom_path, &rgb, width, height, policy)
                    .unwrap_or_else(|e| panic!("{}: {e}", rom_path.display()));
            } else {
                panic!(
                    "Missing golden screenshot for {}. Expected {}.\n\
Set NESER_GOLDEN=accept to write goldens.",
                    rom_path.display(),
                    golden_path.display()
                );
            }
        }
    }
}
