#[cfg(test)]
pub(crate) mod tests {
    use crate::input::{Button, ControllerType, SnesButton};
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::{Config, ExpansionPort, HardwareMode, Nes, NesConfig, RamInitMode};
    use crate::nes::integration_tests::rom_test_runner::tests::run_nes_for_frames;

    /// Controller configuration for a test scenario.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub(crate) struct ControllerConfig {
        pub port1: ControllerType,
        pub port2: ControllerType,
        pub hardware_mode: Option<HardwareMode>,
        pub expansion_port: Option<ExpansionPort>,
    }

    #[allow(dead_code)]
    impl ControllerConfig {
        pub fn to_config(&self) -> Config {
            let mut config = Config {
                nes: NesConfig {
                    ram_init_mode: RamInitMode::Zero,
                    controller_port1: self.port1,
                    controller_port2: self.port2,
                    controller_port1_explicit: true,
                    controller_port2_explicit: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            if let Some(hw_mode) = self.hardware_mode {
                config.nes.hardware_mode = hw_mode;
                config.nes.hardware_mode_explicit = true;
            }
            if let Some(exp_port) = self.expansion_port {
                config.nes.expansion_port = exp_port;
                config.nes.expansion_port_explicit = true;
            }
            config
        }

        pub fn joypad_port1() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Joypad,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn snes_controller_port1() -> Self {
            Self {
                port1: ControllerType::SnesController,
                port2: ControllerType::SnesController,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn snes_mouse_port1() -> Self {
            Self {
                port1: ControllerType::SnesMouse,
                port2: ControllerType::SnesMouse,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn zapper() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Zapper,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn famicom_joypad() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Joypad,
                hardware_mode: Some(HardwareMode::Famicom),
                expansion_port: None,
            }
        }

        pub fn arkanoid() -> Self {
            Self {
                port1: ControllerType::Arkanoid,
                port2: ControllerType::Joypad,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn arkanoid_port2() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Arkanoid,
                hardware_mode: None,
                expansion_port: None,
            }
        }

        pub fn arkanoid_famicom_expansion() -> Self {
            Self {
                port1: ControllerType::Joypad,
                port2: ControllerType::Joypad,
                hardware_mode: Some(HardwareMode::Famicom),
                expansion_port: Some(ExpansionPort::ArkanoidFamicom),
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

    /// Result of running a ROM test harness.
    #[derive(Debug, Clone)]
    pub(crate) struct RomTestResult {
        pub captures: Vec<FrameCapture>,
    }

    /// Run any ROM with the given config and frame script, using a custom
    /// tile-to-char function for nametable text conversion.
    pub(crate) fn run_rom_with_script(
        rom_path: &str,
        config: &Config,
        script: &[ScriptEntry],
        total_frames: u32,
        capture_interval: u32,
        tile_to_char: impl Fn(u8) -> char,
    ) -> RomTestResult {
        let rom_data =
            std::fs::read(rom_path).unwrap_or_else(|_| panic!("{rom_path} ROM should be readable"));
        let cartridge =
            Cartridge::load_from_file(&rom_data, rom_path, crate::app_context::AppContext::new())
                .unwrap_or_else(|_| panic!("{rom_path} ROM should parse successfully"));

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            config.clone(),
        ));
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
                let nametable_text = nametable_raw
                    .chunks(32)
                    .map(|chunk| {
                        chunk
                            .iter()
                            .map(|&b| tile_to_char(b))
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

        RomTestResult { captures }
    }
}
