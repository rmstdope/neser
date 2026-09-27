//! What every web console binding does the same way, written once.
//!
//! `wasm_bindgen` cannot export a generic type, so each console keeps its own exported struct
//! (`WasmNes`, `WasmGb`, `WasmGba`, `WasmSnes`). Each holds a [`WebConsole`] in a field named
//! `web`, and `web_console_bindings!` generates the exported methods that only forward to it.
//! A new web-facing concept every console shares is written here once and added to that macro.
//!
//! This module does not depend on `wasm_bindgen`, so its tests run in the host build.

use crate::platform::emulator::Emulator;
use crate::platform::frontend_toasts::cartridge_load_toast_message;

/// The sample rate a console plays at from the moment a game loads, until the page sets its own.
pub const WEB_AUDIO_SAMPLE_RATE: f32 = 44_100.0;

/// One console as the web page drives it: the core, whether a game is loaded, whether sound is
/// muted, and the toasts waiting for the page.
pub struct WebConsole<E: Emulator> {
    core: E,
    audio_muted: bool,
    rom_loaded: bool,
    pending_toasts: Vec<String>,
}

impl<E: Emulator> WebConsole<E> {
    pub fn new(core: E) -> Self {
        Self {
            core,
            audio_muted: false,
            rom_loaded: false,
            pending_toasts: Vec::new(),
        }
    }

    pub fn core(&self) -> &E {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut E {
        &mut self.core
    }

    /// Whether a game loaded; false from the start of every load until it succeeds.
    pub fn rom_loaded(&self) -> bool {
        self.rom_loaded
    }

    /// Loads `rom` into the core and reports it (see [`Self::record_load`]); a loaded game plays
    /// at [`WEB_AUDIO_SAMPLE_RATE`].
    pub fn load_rom(&mut self, rom: &[u8], name: &str) -> Result<(), String> {
        self.rom_loaded = false;
        let result = self.core.load_rom(rom, name);
        if result.is_ok() {
            self.core.set_audio_sample_rate(WEB_AUDIO_SAMPLE_RATE);
        }
        self.record_load(name, result)
    }

    /// Reports a load the core has finished: what the core said while loading comes first, in
    /// the order it was raised, then whether the cartridge loaded. Returns `result`.
    pub fn record_load(&mut self, name: &str, result: Result<(), String>) -> Result<(), String> {
        self.take_core_toasts();
        self.rom_loaded = result.is_ok();
        self.push_toast(cartridge_load_toast_message(name, result.is_ok()));
        result
    }

    pub fn push_toast(&mut self, text: String) {
        self.pending_toasts.push(text);
    }

    /// Every toast waiting for the page, the core's included, oldest first; none are kept.
    pub fn drain_toasts(&mut self) -> Vec<String> {
        // The page shows only what this returns, so the core's toasts are forwarded here too.
        self.take_core_toasts();
        std::mem::take(&mut self.pending_toasts)
    }

    fn take_core_toasts(&mut self) {
        let core_toasts = self.core.app_context().borrow_mut().take_toasts();
        self.pending_toasts.extend(core_toasts);
    }

    /// F8 in the running game (see [`Emulator::f8_action`]): the corner message to show, or `""`
    /// when nothing changed.
    pub fn f8_action(&mut self) -> String {
        self.core.f8_action().unwrap_or_default()
    }

    /// Every mono sample the core has made since the last call; none while muted.
    pub fn audio_samples(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.discard_audio();
            return Vec::new();
        }
        std::iter::from_fn(|| self.core.get_sample()).collect()
    }

    /// Every stereo sample the core has made since the last call, interleaved left/right; none
    /// while muted.
    pub fn audio_samples_stereo(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.discard_audio();
            return Vec::new();
        }
        std::iter::from_fn(|| self.core.get_stereo_sample())
            .flat_map(|(left, right)| [left, right])
            .collect()
    }

    pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
        self.core.set_audio_sample_rate(sample_rate);
    }

    /// Muting also throws away the sound already made, so unmuting never plays stale audio.
    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_muted = muted;
        if muted {
            self.discard_audio();
        }
    }

    pub fn is_audio_muted(&self) -> bool {
        self.audio_muted
    }

    fn discard_audio(&mut self) {
        while self.core.get_sample().is_some() {}
    }

    pub fn reset(&mut self, soft_reset: bool) {
        self.core.reset(soft_reset);
    }

    /// Runs the core until it has a frame to show, and takes it.
    pub fn run_until_frame_ready(&mut self) {
        while !self.core.is_ready_to_render() {
            self.core.run_tick();
        }
        self.core.clear_ready_to_render();
    }
}

/// A `width` × `height` RGBA8888 frame, black and fully opaque: what a console shows with no game.
pub fn opaque_black_rgba(width: u32, height: u32) -> Vec<u8> {
    [0, 0, 0, 0xFF].repeat((width * height) as usize)
}

/// Generates the exported methods that every console binding forwards unchanged to the
/// [`WebConsole`] in its `web` field. The JavaScript names are the page's, so they stay as they are.
#[cfg(feature = "wasm")]
macro_rules! web_console_bindings {
    ($shell:ident) => {
        #[::wasm_bindgen::prelude::wasm_bindgen]
        impl $shell {
            /// Drain the toast messages waiting for the page, the core's included.
            pub fn drain_toasts(&mut self) -> Vec<::wasm_bindgen::JsValue> {
                self.web
                    .drain_toasts()
                    .into_iter()
                    .map(::wasm_bindgen::JsValue::from)
                    .collect()
            }

            /// F8 and the Palette/Colors buttons: whatever F8 does in the running game. Returns
            /// the corner message for the page to show, or `""` when nothing changed.
            pub fn cycle_palette(&mut self) -> String {
                self.web.f8_action()
            }

            /// Collect all pending mono audio samples; empty while muted. Call after each frame.
            pub fn get_audio_samples(&mut self) -> Vec<f32> {
                self.web.audio_samples()
            }

            /// Set the emulator audio output sample rate in Hz.
            pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
                self.web.set_audio_sample_rate(sample_rate);
            }

            /// Set audio mute state.
            pub fn set_audio_muted(&mut self, muted: bool) {
                self.web.set_audio_muted(muted);
            }

            /// Returns `true` if audio is currently muted.
            pub fn is_audio_muted(&self) -> bool {
                self.web.is_audio_muted()
            }
        }
    };
}
#[cfg(feature = "wasm")]
pub(crate) use web_console_bindings;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::GameBoy;
    use crate::platform::app_context::AppContext;
    use crate::platform::frontend_toasts::cartridge_load_toast_message;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn web_gb() -> WebConsole<GameBoy> {
        let app_context = Rc::new(RefCell::new(
            AppContext::new_with_config(Default::default()),
        ));
        WebConsole::new(GameBoy::new(app_context))
    }

    /// A 32 KB ROM-only cartridge with a valid header checksum; its code is all NOPs.
    fn minimal_gb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn raise_core_toast(web: &WebConsole<GameBoy>, text: &str) {
        web.core().app_context().borrow_mut().add_toast(text);
    }

    #[test]
    fn drain_toasts_forwards_the_core_toasts_once() {
        let mut web = web_gb();
        raise_core_toast(&web, "from the core");
        assert_eq!(web.drain_toasts(), ["from the core"]);
        assert!(web.drain_toasts().is_empty());
    }

    #[test]
    fn a_successful_load_says_so_after_what_the_core_said() {
        let mut web = web_gb();
        raise_core_toast(&web, "from the core");
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        assert!(web.rom_loaded());
        assert_eq!(
            web.drain_toasts(),
            [
                "from the core".to_string(),
                cartridge_load_toast_message("game.gb", true)
            ]
        );
    }

    #[test]
    fn a_failed_load_says_so_and_leaves_no_game() {
        let mut web = web_gb();
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        let _ = web.drain_toasts();
        assert!(web.load_rom(&[0u8; 16], "broken.gb").is_err());
        assert!(!web.rom_loaded());
        assert_eq!(
            web.drain_toasts(),
            [cartridge_load_toast_message("broken.gb", false)]
        );
    }

    #[test]
    fn muted_audio_returns_nothing_and_discards_what_was_queued() {
        let mut unmuted = web_gb();
        unmuted
            .load_rom(&minimal_gb_rom(), "game.gb")
            .expect("loads");
        unmuted.run_until_frame_ready();
        assert!(!unmuted.audio_samples().is_empty(), "a frame makes sound");

        let mut web = web_gb();
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        web.run_until_frame_ready();
        web.set_audio_muted(true);
        assert!(web.is_audio_muted());
        // No read while muted: muting itself must throw the queued sound away.
        web.set_audio_muted(false);
        assert!(!web.is_audio_muted());
        assert!(
            web.audio_samples().is_empty(),
            "what was queued before muting is gone"
        );
    }

    #[test]
    fn audio_samples_while_muted_are_empty() {
        let mut web = web_gb();
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        web.set_audio_muted(true);
        web.run_until_frame_ready();
        assert!(web.audio_samples().is_empty());
    }

    #[test]
    fn muted_stereo_audio_returns_nothing() {
        let mut web = web_gb();
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        web.run_until_frame_ready();
        let stereo = web.audio_samples_stereo();
        assert!(
            !stereo.is_empty() && stereo.len().is_multiple_of(2),
            "interleaved"
        );
        web.run_until_frame_ready();
        web.set_audio_muted(true);
        assert!(web.audio_samples_stereo().is_empty());
    }

    #[test]
    fn f8_without_a_game_returns_an_empty_message() {
        assert_eq!(web_gb().f8_action(), "");
    }

    #[test]
    fn run_until_frame_ready_finishes_one_frame() {
        let mut web = web_gb();
        web.load_rom(&minimal_gb_rom(), "game.gb").expect("loads");
        assert!(!web.core().sample_ready(), "nothing has run yet");
        web.run_until_frame_ready();
        assert!(web.core().sample_ready(), "a frame's worth of the core ran");
        assert!(!web.core().is_ready_to_render(), "the frame was taken");
    }

    #[test]
    fn opaque_black_rgba_is_black_and_opaque() {
        let rgba = opaque_black_rgba(3, 2);
        assert_eq!(rgba.len(), 3 * 2 * 4);
        for pixel in rgba.as_chunks::<4>().0 {
            assert_eq!(pixel, &[0, 0, 0, 0xFF]);
        }
    }
}
