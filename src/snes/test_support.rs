//! Shared construction helpers for SNES tests.
//!
//! Every SNES test that builds a console goes through here so the power-on RAM
//! pattern is pinned in exactly one place.

use crate::platform::app_context::AppContext;
use crate::platform::config::{Config, RamInitMode};

/// A [`Config`] with the power-on RAM pattern pinned to
/// [`RamInitMode::Zero`], for tests that need a deterministic console.
///
/// `Config::default()` resolves `ram_init_mode` to [`RamInitMode::Random`] on
/// native targets (it is [`RamInitMode::Zero`] only on wasm), and since #3128
/// the SNES core honours that setting for WRAM, VRAM, CGRAM, OAM, ARAM and SA-1
/// I-RAM. A test built straight from `Config::default()` would therefore see a
/// different machine on every run, and every committed screen CRC would be
/// measuring the RNG. Tests that deliberately exercise a non-zero mode set the
/// field themselves after calling this.
pub(crate) fn snes_test_config() -> Config {
    let mut config = Config::default();
    config.frontend.ram_init_mode = RamInitMode::Zero;
    config
}

/// [`snes_test_config`] wrapped in an [`AppContext`], for the common case of
/// `Snes::new(...)` with no other configuration.
pub(crate) fn snes_test_app_context() -> AppContext {
    AppContext::new_with_config(snes_test_config())
}
