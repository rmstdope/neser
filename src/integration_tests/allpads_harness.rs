#[cfg(test)]
pub(crate) mod tests {
    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, RamInitMode};
    use crate::input::{Button, ControllerType, SnesButton};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;

    const ALLPADS_ROM_PATH: &str = "roms/automated_tests/allpads-r9/allpads218.nes";

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

        pub fn snes_controller_port1() -> Self {
            Self {
                port1: ControllerType::SnesController,
                port2: ControllerType::SnesController,
            }
        }

        pub fn snes_mouse_port1() -> Self {
            Self {
                port1: ControllerType::SnesMouse,
                port2: ControllerType::SnesMouse,
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
        /// Press or release an SNES button on a port.
        SnesButton {
            port: u8,
            button: SnesButton,
            pressed: bool,
        },
        /// Set the mouse X position (for Arkanoid/Zapper).
        MouseX(u8),
        /// Set the mouse Y position (for Zapper).
        MouseY(u8),
        /// Set mouse left button (for Arkanoid trigger / Zapper trigger).
        MouseButton(bool),
        /// Set mouse right button (for Super NES mouse secondary button).
        MouseRightButton(bool),
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
        pub nametable_raw: Vec<u8>,
        pub oam_data: Vec<u8>,
    }

    /// Build a script that enters the controller test (A press+release at frames
    /// 300/305) then presses the given button at frame 400.
    #[allow(dead_code)]
    pub(crate) fn script_enter_test_and_press(button: Button) -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: false,
                }],
            },
            ScriptEntry {
                frame: 400,
                actions: vec![InputAction::Button {
                    port: 1,
                    button,
                    pressed: true,
                }],
            },
        ]
    }

    /// Build a script that enters the controller test (A press+release at frames
    /// 300/305) without pressing any additional button.
    #[allow(dead_code)]
    pub(crate) fn script_enter_test() -> Vec<ScriptEntry> {
        vec![
            ScriptEntry {
                frame: 300,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: true,
                }],
            },
            ScriptEntry {
                frame: 305,
                actions: vec![InputAction::Button {
                    port: 1,
                    button: Button::A,
                    pressed: false,
                }],
            },
        ]
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
        let rom_data =
            std::fs::read(ALLPADS_ROM_PATH).expect("allpads218.nes ROM should be readable");
        let cartridge = Cartridge::load_from_file(
            &rom_data,
            ALLPADS_ROM_PATH,
            crate::app_context::AppContext::new(),
        )
        .expect("allpads218.nes ROM should parse successfully");

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
                        InputAction::SnesButton {
                            port,
                            button,
                            pressed,
                        } => {
                            nes.set_snes_button(*port, *button, *pressed);
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
                        InputAction::MouseRightButton(pressed) => {
                            nes.set_mouse_right_button(*pressed);
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
                let nametable_raw = nes.read_nametable_raw(base_addr, 32 * 30);
                // allpads218.nes tiles are ASCII - 0x20 (tile 0x21 = 'A', tile 0x10 = '0')
                let nametable_text = nametable_raw
                    .chunks(32)
                    .map(|chunk| {
                        chunk
                            .iter()
                            .map(|&b| {
                                let ascii = b.wrapping_add(0x20);
                                if (0x20..=0x7E).contains(&ascii) {
                                    ascii as char
                                } else {
                                    ' '
                                }
                            })
                            .collect::<String>()
                            .trim_end()
                            .to_string()
                    })
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");

                let oam_data = nes.ppu().borrow().oam_snapshot();

                captures.push(FrameCapture {
                    frame,
                    nametable_text,
                    nametable_raw,
                    oam_data,
                });
            }
        }

        AllpadsResult { captures }
    }
}
