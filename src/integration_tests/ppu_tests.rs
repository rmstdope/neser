#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, TvSystem};
    use crate::input::Button;
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

    fn load_oamtest3_nes() -> Nes {
        let rom_path = "roms/automated_tests/oamtest3/oam3.nes";
        let rom_data = fs::read(rom_path).expect("oam3 ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("oam3 ROM should parse");

        let mut nes = Nes::new(Config::default());
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes
    }

    fn run_frames(nes: &mut Nes, frame_counter: &mut u32, frames: u32) {
        run_nes_for_frames(nes, frames);
        *frame_counter += frames;
    }

    fn tap_button(nes: &mut Nes, frame_counter: &mut u32, button: Button) {
        nes.set_button(1, button, true);
        run_frames(nes, frame_counter, 1);
        nes.set_button(1, button, false);
        run_frames(nes, frame_counter, 2);
    }

    fn tap_button_many(nes: &mut Nes, frame_counter: &mut u32, button: Button, times: usize) {
        for _ in 0..times {
            tap_button(nes, frame_counter, button);
        }
    }

    fn move_to_count_low_nibble(nes: &mut Nes, frame_counter: &mut u32) {
        tap_button(nes, frame_counter, Button::Right);
    }

    fn move_to_payload_start_from_count_high(nes: &mut Nes, frame_counter: &mut u32) {
        tap_button_many(nes, frame_counter, Button::Right, 2);
    }

    fn move_to_payload_start_from_count_low(nes: &mut Nes, frame_counter: &mut u32) {
        tap_button(nes, frame_counter, Button::Right);
    }

    fn set_count_to_14_from_default(nes: &mut Nes, frame_counter: &mut u32) {
        move_to_count_low_nibble(nes, frame_counter);
        tap_button_many(nes, frame_counter, Button::Up, 7);
    }

    fn set_nibble_from_zero_and_advance(nes: &mut Nes, frame_counter: &mut u32, nibble: u8) {
        if nibble > 0 {
            tap_button_many(nes, frame_counter, Button::Up, nibble as usize);
        }
        tap_button(nes, frame_counter, Button::Right);
    }

    fn set_byte_from_zero_and_advance(nes: &mut Nes, frame_counter: &mut u32, byte: u8) {
        set_nibble_from_zero_and_advance(nes, frame_counter, (byte >> 4) & 0x0F);
        set_nibble_from_zero_and_advance(nes, frame_counter, byte & 0x0F);
    }

    fn set_sprite_discriminator_payload_from_zero(nes: &mut Nes, frame_counter: &mut u32) {
        // Use 14 bytes to stay within oam3's editable/upload range and avoid cursor wrap into count.
        // This still keeps distinct tile/attribute/X signatures for visual discrimination.
        // 50 00 00 30 70 01 00 30 90 0E 00 A0 B0 0F
        let payload: [u8; 14] = [
            0x50, 0x00, 0x00, 0x30, 0x70, 0x01, 0x00, 0x30, 0x90, 0x0E, 0x00, 0xA0, 0xB0, 0x0F,
        ];

        for value in payload {
            set_byte_from_zero_and_advance(nes, frame_counter, value);
        }
    }

    fn write_png(path: &Path, rgb: &[u8], width: u32, height: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("checkpoint artifact directory should be created");
        }
        let file = fs::File::create(path).expect("checkpoint image file should be created");
        let mut writer = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(&mut writer, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut png_writer = encoder
            .write_header()
            .expect("checkpoint PNG header should be written");
        png_writer
            .write_image_data(rgb)
            .expect("checkpoint PNG image data should be written");
        drop(png_writer);
        writer
            .flush()
            .expect("checkpoint PNG buffer should be flushed");
    }

    fn collect_checkpoint(
        nes: &Nes,
        frame_counter: u32,
        name: &'static str,
        capture_baseline: bool,
        baseline_dir: &Path,
        checkpoints: &mut Vec<(&'static str, u32)>,
    ) {
        let screen = nes.get_screen_buffer();
        let crc = screen.crc32();
        let rgb = if capture_baseline {
            Some(screen.snapshot())
        } else {
            None
        };
        drop(screen);

        if capture_baseline {
            println!(
                "[oam3-checkpoint] {} frame={} crc=0x{:08X}",
                name, frame_counter, crc
            );
            if let Some(rgb) = rgb {
                let checkpoint_path =
                    baseline_dir.join(format!("{}_f{:04}.png", name, frame_counter));
                write_png(&checkpoint_path, &rgb, 256, 240);
            }
        }

        checkpoints.push((name, crc));
    }

    fn run_oam3_phase_a(capture_baseline: bool, baseline_dir: &Path) -> Vec<(&'static str, u32)> {
        let mut nes = load_oamtest3_nes();
        let mut frame_counter = 0u32;
        let mut checkpoints = Vec::new();

        run_frames(&mut nes, &mut frame_counter, 90);

        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "A1_count_07",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        move_to_payload_start_from_count_high(&mut nes, &mut frame_counter);
        set_sprite_discriminator_payload_from_zero(&mut nes, &mut frame_counter);
        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "A2_payload_mutation",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        checkpoints
    }

    fn run_oam3_phase_b(capture_baseline: bool, baseline_dir: &Path) -> Vec<(&'static str, u32)> {
        let mut nes = load_oamtest3_nes();
        let mut frame_counter = 0u32;
        let mut checkpoints = Vec::new();

        run_frames(&mut nes, &mut frame_counter, 90);
        set_count_to_14_from_default(&mut nes, &mut frame_counter);

        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "B1_count_14",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        move_to_payload_start_from_count_low(&mut nes, &mut frame_counter);
        set_sprite_discriminator_payload_from_zero(&mut nes, &mut frame_counter);
        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "B2_payload_mutation",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        checkpoints
    }

    fn run_oam3_transition(
        capture_baseline: bool,
        baseline_dir: &Path,
    ) -> Vec<(&'static str, u32)> {
        let mut nes = load_oamtest3_nes();
        let mut frame_counter = 0u32;
        let mut checkpoints = Vec::new();

        run_frames(&mut nes, &mut frame_counter, 90);
        set_count_to_14_from_default(&mut nes, &mut frame_counter);

        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "T1_before_14_to_7",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        tap_button_many(&mut nes, &mut frame_counter, Button::Down, 7);
        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "T2_after_14_to_7",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        move_to_payload_start_from_count_low(&mut nes, &mut frame_counter);
        set_sprite_discriminator_payload_from_zero(&mut nes, &mut frame_counter);
        run_frames(&mut nes, &mut frame_counter, 5);
        collect_checkpoint(
            &nes,
            frame_counter,
            "T3_post_transition_mutation",
            capture_baseline,
            baseline_dir,
            &mut checkpoints,
        );

        checkpoints
    }

    #[test]
    fn test_oamtest3_scripted_input_crc_checkpoints() {
        let capture_baseline = std::env::var_os("NESER_OAM3_CAPTURE_BASELINE").is_some();
        let baseline_dir = PathBuf::from("target/oamtest3_checkpoints");

        let mut actual = Vec::<(&'static str, u32)>::new();
        actual.extend(run_oam3_phase_a(capture_baseline, &baseline_dir));
        actual.extend(run_oam3_phase_b(capture_baseline, &baseline_dir));
        actual.extend(run_oam3_transition(capture_baseline, &baseline_dir));

        // A1 - One sprite in upper left corner. The leftmost part of a sprite in the upper right coner
        // A2 - The sprite from the upper left coner has moved a bit to the right and a bit down. The other sprite has moved down.
        // B1 - One sprite in the upper left corner.
        // B2 - The sprite has moved 75% to the right and 60% down and changed character.
        // T1 - Same as B1.
        // T2 - Same as A1.
        // T3 - Same as A2, but also a third sprite in the upper left corner.
        let expected = [
            ("A1_count_07", 0x7184_BC66),
            ("A2_payload_mutation", 0xE16F_41C6),
            ("B1_count_14", 0x1A42_F1E6),
            ("B2_payload_mutation", 0xBB0E_6B3E),
            ("T1_before_14_to_7", 0x1A42_F1E6),
            ("T2_after_14_to_7", 0x9318_29C2),
            ("T3_post_transition_mutation", 0x144D_7EEB),
        ];

        if capture_baseline {
            println!(
                "[oam3-checkpoint] generated baseline artifacts in target/oamtest3_checkpoints"
            );
        }

        assert_eq!(
            actual.len(),
            expected.len(),
            "unexpected number of oam3 checkpoints"
        );

        assert_eq!(
            actual, expected,
            "oam3 checkpoint CRC mismatch; actual table: {:?}",
            actual
        );
    }

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
