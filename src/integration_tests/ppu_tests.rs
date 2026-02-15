#[cfg(test)]
mod tests {
    use std::fs;

    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, TvSystem};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::{setup_rom_console_test, setup_rom_console_test_with_ram_init, setup_rom_test};

    fn capture_scanline_rgb(nes: &Nes, y: u32) -> Vec<(u8, u8, u8)> {
        let screen_buffer = nes.get_screen_buffer();
        (0..TvSystem::Ntsc.screen_width())
            .map(|x| screen_buffer.get_pixel(x, y))
            .collect()
    }

    fn count_contiguous_white_pixels(
        line: &[(u8, u8, u8)],
        white: (u8, u8, u8),
    ) -> Vec<(usize, usize)> {
        let mut runs = Vec::new();
        let mut in_run = false;
        let mut run_start = 0;

        for (i, &pixel) in line.iter().enumerate() {
            if pixel == white {
                if !in_run {
                    in_run = true;
                    run_start = i;
                }
            } else if in_run {
                runs.push((run_start, i - 1));
                in_run = false;
            }
        }

        if in_run {
            runs.push((run_start, line.len() - 1));
        }

        runs
    }

    fn matches_white_run(
        line: &[(u8, u8, u8)],
        start_x: usize,
        end_x: usize,
        white: (u8, u8, u8),
        black: (u8, u8, u8),
    ) -> bool {
        if start_x > end_x || end_x >= line.len() {
            return false;
        }

        if start_x > 0 && line[start_x - 1] != black {
            return false;
        }

        if end_x + 1 < line.len() && line[end_x + 1] != black {
            return false;
        }

        line[start_x..=end_x].iter().all(|&pixel| pixel == white)
    }

    // blargg_ppu_tests_2005.09.15b
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_palette_ram,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/palette_ram.nes",
        "$01"
    );
    setup_rom_console_test_with_ram_init!(
        test_blargg_ppu_tests_2005_09_15b_power_up_palette,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/power_up_palette.nes",
        "$01",
        crate::console::RamInitMode::Random
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_sprite_ram,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/sprite_ram.nes",
        "$01"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_vbl_clear_time,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes",
        "$01"
    );
    setup_rom_console_test!(
        test_blargg_ppu_tests_2005_09_15b_vram_access,
        "roms/automated_tests/blargg_ppu_tests_2005.09.15b/vram_access.nes",
        "$01"
    );

    // TODO full_palette

    // TODO misc_oam_tests

    // nmi_sync
    #[test]
    fn test_nmi_sync_demo_ntsc() {
        let rom_path = "roms/automated_tests/nmi_sync/demo_ntsc.nes";
        let rom_data = fs::read(rom_path).expect("demo_ntsc ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("demo_ntsc ROM should parse");

        let mut nes = Nes::new(Config::default());
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        const WARMUP_FRAMES: u32 = 25;
        run_nes_for_frames(&mut nes, WARMUP_FRAMES);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_a = capture_scanline_rgb(&nes, 121);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_b = capture_scanline_rgb(&nes, 121);
        let upper_line = capture_scanline_rgb(&nes, 119);
        let lower_line = capture_scanline_rgb(&nes, 123);

        let white = Nes::lookup_system_palette(0x30);
        let black = Nes::lookup_system_palette(0x0D);

        // Math upper and lower lines
        assert!(matches_white_run(&upper_line, 80, 103, white, black));
        assert!(matches_white_run(&lower_line, 80, 103, white, black));

        let a_80 = matches_white_run(&line_frame_a, 80, 103, white, black);
        let a_81 = matches_white_run(&line_frame_a, 81, 103, white, black);
        let b_80 = matches_white_run(&line_frame_b, 80, 103, white, black);
        let b_81 = matches_white_run(&line_frame_b, 81, 103, white, black);

        assert!(
            (a_80 && b_81) || (a_81 && b_80),
            "expected scanline 124 to alternate between white runs at x=80..103 and x=81..103, but got {:?} and {:?}",
            line_frame_a,
            line_frame_b
        );
    }

    #[test]
    fn test_nmi_sync_demo_pal() {
        let rom_path = "roms/automated_tests/nmi_sync/demo_pal.nes";
        let rom_data = fs::read(rom_path).expect("demo_pal ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("demo_pal ROM should parse");

        let config = Config {
            tv_system: crate::console::TvSystem::Pal,
            ..Default::default()
        };
        let mut nes = Nes::new(config);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        const WARMUP_FRAMES: u32 = 25;
        run_nes_for_frames(&mut nes, WARMUP_FRAMES);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_a = capture_scanline_rgb(&nes, 121);

        run_nes_for_frames(&mut nes, 1);
        let line_frame_b = capture_scanline_rgb(&nes, 121);
        let upper_line = capture_scanline_rgb(&nes, 119);
        let lower_line = capture_scanline_rgb(&nes, 123);

        let white = Nes::lookup_system_palette(0x30);

        //  Find all white runs in both frames
        let runs_a = count_contiguous_white_pixels(&line_frame_a, white);
        let runs_b = count_contiguous_white_pixels(&line_frame_b, white);
        let upper_run = count_contiguous_white_pixels(&upper_line, white);
        let lower_run = count_contiguous_white_pixels(&lower_line, white);

        // Math upper and lower lines
        assert_eq!(upper_run[0], (82, 105));
        assert_eq!(lower_run[0], (84, 105));

        //  Middle line on the two frames should start somewhere between upper and lower
        assert!(runs_a[0].0 >= 82 && runs_a[0].0 <= 84);
        assert!(runs_b[0].0 >= 82 && runs_b[0].0 <= 84);
        // and end on the same pixel as upper and lower
        assert_eq!(runs_a[0].1, 105);
        assert_eq!(runs_b[0].1, 105);
    }

    // oam_read
    setup_rom_test!(test_oam_read, "roms/automated_tests/oam_read/oam_read.nes");

    // oam_stress
    setup_rom_test!(
        test_oam_stress,
        "roms/automated_tests/oam_stress/oam_stress.nes"
    );

    // TODO oamtest3

    // ppu_open_bus
    setup_rom_test!(
        test_ppu_open_bus,
        "roms/automated_tests/ppu_open_bus/ppu_open_bus.nes"
    );

    // ppu_read_buffer
    setup_rom_test!(
        test_ppu_read_buffer,
        "roms/automated_tests/ppu_read_buffer/test_ppu_read_buffer.nes"
    );

    // ppu_sprite_hit
    setup_rom_test!(
        test_sprite_hit,
        "roms/automated_tests/ppu_sprite_hit/ppu_sprite_hit.nes"
    );
    setup_rom_test!(
        test_sprite_hit_01,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_sprite_hit_02,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/02-alignment.nes"
    );
    setup_rom_test!(
        test_sprite_hit_03,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/03-corners.nes"
    );
    setup_rom_test!(
        test_sprite_hit_04,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/04-flip.nes"
    );
    setup_rom_test!(
        test_sprite_hit_05,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/05-left_clip.nes"
    );
    setup_rom_test!(
        test_sprite_hit_06,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/06-right_edge.nes"
    );
    setup_rom_test!(
        test_sprite_hit_07,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/07-screen_bottom.nes"
    );
    setup_rom_test!(
        test_sprite_hit_08,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/08-double_height.nes"
    );
    setup_rom_test!(
        test_sprite_hit_09,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/09-timing.nes"
    );
    setup_rom_test!(
        test_sprite_hit_10,
        "roms/automated_tests/ppu_sprite_hit/rom_singles/10-timing_order.nes"
    );

    // ppu_sprite_overflow
    setup_rom_test!(
        test_sprite_overflow,
        "roms/automated_tests/ppu_sprite_overflow/ppu_sprite_overflow.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_01,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_02,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/02-details.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_03,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/03-timing.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_04,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/04-obscure.nes"
    );
    setup_rom_test!(
        test_sprite_overflow_05,
        "roms/automated_tests/ppu_sprite_overflow/rom_singles/05-emulator.nes"
    );

    // ppu_vbl_nmi
    setup_rom_test!(
        test_ppu_vbl_nmi,
        "roms/automated_tests/ppu_vbl_nmi/ppu_vbl_nmi.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_01,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/01-vbl_basics.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_02,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_03,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_04,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/04-nmi_control.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_05,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/05-nmi_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_06,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/06-suppression.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_07,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_08,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_09,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes"
    );
    setup_rom_test!(
        test_ppu_vbl_nmi_10,
        "roms/automated_tests/ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes"
    );

    // TODO scanline-a1
}
