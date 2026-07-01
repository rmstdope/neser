use std::cell::RefCell;

use crate::gb::debugging::control::GbDebuggerController;
use crate::platform::audio::{EmulatorAudio, normalize_nes_sample};
use crate::platform::debugging::Tracing;
use crate::platform::emulator::{Console, Emulator};
use crate::{frontends::native::NativeAudio, nes::debugging::control::DebuggerController};

pub(crate) struct NativeFrameRunner;

impl NativeFrameRunner {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn debugger_paused(
        &self,
        console: &Console,
        debugger_controller: &DebuggerController,
        gb_debugger_controller: &GbDebuggerController,
    ) -> bool {
        match console {
            Console::Nes(_) => debugger_controller.is_paused(),
            Console::GameBoy(_) => gb_debugger_controller.is_paused(),
            Console::GameBoyAdvance(_) | Console::Snes(_) => false,
        }
    }

    pub(crate) fn debugger_open(
        &self,
        console: &Console,
        debugger_controller: &DebuggerController,
        gb_debugger_controller: &GbDebuggerController,
    ) -> bool {
        match console {
            Console::Nes(_) => debugger_controller.is_debugger_open(),
            Console::GameBoy(_) => gb_debugger_controller.is_debugger_open(),
            Console::GameBoyAdvance(_) | Console::Snes(_) => false,
        }
    }

    pub(crate) fn run_frame(
        &self,
        console: &mut Console,
        tracing: &Tracing,
        debugger_controller: &mut DebuggerController,
        gb_debugger_controller: &mut GbDebuggerController,
        audio_cell: &RefCell<Option<NativeAudio>>,
    ) {
        if let Some(nes) = console.as_nes_mut() {
            debugger_controller.run_frame(nes, tracing, &mut |nes| {
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while nes.sample_ready() {
                        if let Some(sample) = nes.get_sample() {
                            audio.queue_sample(normalize_nes_sample(sample));
                        }
                    }
                }
            });
        } else if let Some(gb) = console.as_gameboy_mut() {
            gb.run_frame_with_debugger(gb_debugger_controller, audio_cell);
        } else if let Some(gba) = console.as_gba_mut() {
            while !gba.is_ready_to_render() {
                let _ = gba.run_tick();
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while gba.sample_ready() {
                        if let Some((left, right)) = gba.get_stereo_sample() {
                            audio.queue_stereo_sample(left, right);
                        }
                    }
                }
            }
        } else if let Some(snes) = console.as_snes_mut() {
            while !snes.is_ready_to_render() {
                let _ = snes.run_tick();
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while snes.sample_ready() {
                        if let Some(sample) = snes.get_sample() {
                            audio.queue_sample(sample);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_context::{AppContext, IntoSharedAppContext};

    fn make_console(system: &str) -> Console {
        let context = AppContext::new().into_shared();
        match system {
            "nes" => Console::new_nes(context),
            "gb" => Console::new_gameboy(context),
            "gba" => Console::new_gba(context),
            "snes" => Console::new_snes(context),
            _ => unreachable!(),
        }
    }

    #[test]
    fn debugger_state_routes_to_supported_console_controllers() {
        let runner = NativeFrameRunner::new();
        let console_nes = make_console("nes");
        let console_gb = make_console("gb");
        let console_gba = make_console("gba");
        let console_snes = make_console("snes");
        let debugger_controller = DebuggerController::new(&[], true);
        let gb_debugger_controller = GbDebuggerController::new(&[], true);

        assert!(runner.debugger_paused(
            &console_nes,
            &debugger_controller,
            &gb_debugger_controller
        ));
        assert!(runner.debugger_open(&console_nes, &debugger_controller, &gb_debugger_controller));

        assert!(runner.debugger_paused(&console_gb, &debugger_controller, &gb_debugger_controller));
        assert!(runner.debugger_open(&console_gb, &debugger_controller, &gb_debugger_controller));

        assert!(!runner.debugger_paused(
            &console_gba,
            &debugger_controller,
            &gb_debugger_controller
        ));
        assert!(!runner.debugger_open(&console_gba, &debugger_controller, &gb_debugger_controller));
        assert!(!runner.debugger_paused(
            &console_snes,
            &debugger_controller,
            &gb_debugger_controller
        ));
        assert!(!runner.debugger_open(
            &console_snes,
            &debugger_controller,
            &gb_debugger_controller
        ));
    }
}
