//! Black-box coverage for issue #2962: the two vendored absindx SA-1 conformance ROMs
//! (`roms/snes/automated_tests/snes_test_roms/absindx/`), verified with the same
//! human-approved screen-CRC golden methodology as the blargg/gilyon suites. Both ROMs are
//! documented (and separately confirmed) to behave unreliably on Mesen2, so the goldens are
//! navigator-approved captures of NESER's own rendering (`NESER_CAPTURE_SCREEN=1`), not
//! Mesen2 cross-checks.
//!
//! - `SA1RamProtectionTest.sfc` runs 222 sub-tests of I-RAM/BW-RAM write protection,
//!   mirroring, and reboot semantics across both CPUs, then renders `Result  Passed` with a
//!   register dump. All 222 sub-tests pass; the golden captures that PASSED screen.
//! - `SA1VersionCodeTest.sfc` dumps the `$2300-$2310` register block as read by *both* CPUs
//!   (each register read twice with different residual bus bytes, `$23` and `$AA`, so open-bus
//!   registers are identified by echoing those residuals) and renders it with the version
//!   code. Its result line reads `Failed` **even on real hardware**: disassembly shows its
//!   `CheckResult` unconditionally takes the failed path (the pass path at `$9EAB` is
//!   unreferenced dead code) -- deliberate, since the SA-1's true version-code value is
//!   unknown (fullsnes: "Existing value(s) are unknown"; bsnes: `$230E` "does not actually
//!   exist on real hardware ... always returns open bus"). The golden therefore captures the
//!   hardware-accurate register-dump screen, `Failed` line included.
//!
//! Both ROMs settle into static result screens (SNES CPU in its `WAI` idle loop, SA-1 parked
//! in `SA1TestFinished_InfLoop`), so a fixed-frame CRC is stable.

use crate::snes::integration_tests::rom_runner::{RunConfig, assert_rom_screen_crc};

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/absindx";

#[test]
fn sa1_ram_protection_test_passes() {
    assert_rom_screen_crc(
        ROOT,
        "SA1RamProtectionTest.sfc",
        "sa1_absindx_tests",
        150,
        0xD652_1ACE,
        RunConfig::new(400_000_000, 0),
    );
}

#[test]
fn sa1_version_code_test_matches_approved_register_dump() {
    // Golden re-approved for #2944: general-purpose DMA now advances the bus per
    // transferred byte, so the ROM's SA-1 H/V counter latch happens at a later,
    // hardware-plausible scan position. Verified pixel-level against the previously
    // approved capture (reproduced bit-exactly by no-op'ing `dma_tick`): only the
    // HCRL/HCRH/VCRL rows of the dump changed (H $00A2 -> $0145, V $3B -> $09);
    // every other register value is identical.
    assert_rom_screen_crc(
        ROOT,
        "SA1VersionCodeTest.sfc",
        "sa1_absindx_tests",
        150,
        0x28BE_AB0E,
        RunConfig::new(400_000_000, 0),
    );
}
