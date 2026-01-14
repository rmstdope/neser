#[cfg(test)]
mod tests {
    use crate::cartridge::Cartridge;
    use crate::manual_test_cartridges;
    use crate::nes::{Nes, TvSystem};

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

    #[test]
    fn test_run_nes_for_frames_returns_rgb_buffer() {
        let rom_data = manual_test_cartridges::triangle_only_nrom_128();
        let cartridge = Cartridge::new(&rom_data).expect("ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let frame = run_nes_for_frames(&mut nes, 2);

        let expected_len = (TvSystem::Ntsc.screen_width() * TvSystem::Ntsc.screen_height() * 3) as usize;
        assert_eq!(frame.len(), expected_len);
    }
}
