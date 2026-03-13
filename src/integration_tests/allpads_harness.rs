#[cfg(test)]
pub(crate) mod tests {
    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, RamInitMode};
    use crate::input::{Button, ControllerType};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;

    const ALLPADS_ROM_PATH: &str = "roms/automated_tests/allpads-r9/allpads.nes";

    /// Controller configuration for a test scenario.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub(crate) struct ControllerConfig {
        pub port1: ControllerType,
        pub port2: ControllerType,
    }

    #[allow(dead_code)]
    impl ControllerConfig {
        pub fn joypad_port1() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Joypad,
            }
        }

        pub fn zapper() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Zapper,
            }
        }

        pub fn arkanoid() -> Self {
            Self {
                port1: ControllerType::Arkanoid,
                port2: ControllerType::Joypad,
            }
        }
    }

    /// A single input action to apply at a specific frame.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub(crate) enum InputAction {
        /// Press or release a joypad button on a port.
        Button {
            port: u8,
            button: Button,
            pressed: bool,
        },
        /// Set the mouse X position (for Arkanoid/Zapper).
        MouseX(u8),
        /// Set the mouse Y position (for Zapper).
        MouseY(u8),
        /// Set mouse left button (for Arkanoid trigger / Zapper trigger).
        MouseButton(bool),
    }

    /// A scripted input entry: apply actions at a specific frame.
    #[derive(Debug, Clone)]
    pub(crate) struct ScriptEntry {
        pub frame: u32,
        pub actions: Vec<InputAction>,
    }

    /// Captured output from a single frame snapshot.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub(crate) struct FrameCapture {
        pub frame: u32,
        pub nametable_text: String,
    }

    /// Result of running the allpads harness.
    #[derive(Debug, Clone)]
    pub(crate) struct AllpadsResult {
        pub captures: Vec<FrameCapture>,
    }

    /// Run `allpads.nes` with the given controller configuration and frame script.
    ///
    /// The script is a list of `ScriptEntry` items, each specifying a frame number
    /// and a set of input actions to apply at that frame. The harness runs
    /// `total_frames` frames, capturing nametable text at each `capture_interval` frame.
    ///
    /// # Arguments
    /// * `controller_config` - Controller types for port 1 and port 2
    /// * `script` - Scripted input actions sorted by frame number
    /// * `total_frames` - Total number of frames to run
    /// * `capture_interval` - Capture nametable text every N frames (0 = only at end)
    pub(crate) fn run_allpads(
        controller_config: &ControllerConfig,
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
    ) -> AllpadsResult {
        let rom_data = std::fs::read(ALLPADS_ROM_PATH).expect("allpads.nes ROM should be readable");
        let cartridge = Cartridge::load_from_file(
            &rom_data,
            ALLPADS_ROM_PATH,
            crate::app_context::AppContext::new(),
        )
        .expect("allpads.nes ROM should parse successfully");

        let config = Config {
            ram_init_mode: RamInitMode::Zero,
            controller_port1: controller_config.port1,
            controller_port2: controller_config.port2,
            controller_port1_explicit: true,
            controller_port2_explicit: true,
            ..Default::default()
        };

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let mut captures = Vec::new();
        let mut script_idx = 0;

        for frame in 1..=total_frames {
            // Apply any scripted actions for this frame
            while script_idx < script.len() && script[script_idx].frame == frame {
                for action in &script[script_idx].actions {
                    match action {
                        InputAction::Button {
                            port,
                            button,
                            pressed,
                        } => {
                            nes.set_button(*port, *button, *pressed);
                        }
                        InputAction::MouseX(pos) => {
                            nes.set_mouse_x_position(*pos);
                        }
                        InputAction::MouseY(pos) => {
                            nes.set_mouse_y_position(*pos);
                        }
                        InputAction::MouseButton(pressed) => {
                            nes.set_mouse_left_button(*pressed);
                        }
                    }
                }
                script_idx += 1;
            }

            // Run one frame
            run_nes_for_frames(&mut nes, 1);

            // Capture nametable text at the requested interval or at the final frame
            let should_capture = if capture_interval > 0 {
                frame % capture_interval == 0
            } else {
                frame == total_frames
            };

            if should_capture {
                let base_addr = nes.base_nametable_addr();
                let raw_text = nes.read_nametable_text(base_addr, 32 * 30);
                let nametable_text = raw_text
                    .as_bytes()
                    .chunks(32)
                    .map(|chunk| String::from_utf8_lossy(chunk).trim_end().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");

                captures.push(FrameCapture {
                    frame,
                    nametable_text,
                });
            }
        }

        AllpadsResult { captures }
    }
}
