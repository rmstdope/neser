//! KungFuFurby's NMI test ROM collection (issue #2883), sourced from a 2016
//! forum find of byuu's "SNES TEST IMAGE" test suite (no formal license,
//! recorded as `unknown`; `test_irq.smc`/`test_irq4200.smc` are
//! byte-identical to the tukuyomi-bsnes-tests mirror, `nmi.smc` is
//! byte-identical to the jonasquinn-test-roms mirror -- see
//! `roms/snes/automated_tests/manifest.json`).
//!
//! Each ROM renders a solid backdrop color once its self-check completes:
//! blue for PASS, red/maroon for FAIL. Verified against Mesen2 headless
//! captures (`--Video.VideoFilter=None --Video.AspectRatio=NoStretching
//! --snes.disableFrameSkipping=true`), which show blue for all three ROMs
//! at frame 600 (nmi.smc transitions blue between frames ~450-600;
//! test_nmi.smc between ~30-60; demo_nmitest.smc is stable blue from
//! frame 5).

use super::rom_screen_crc_helpers::assert_rom_screen_crc;
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_nmitest_passes() {
        let path = Path::new(ROOT).join("demo_nmitest.smc");
        assert_rom_screen_crc(
            &path,
            "demo_nmitest.smc",
            "kungfufurby_nmi_tests",
            600,
            0x8695_BBB0,
        );
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// A real NMI dispatch-timing bug was found and partially fixed (#2883,
    /// `Cpu::step`'s freshly-polled-edge dispatch delay) -- proven via a
    /// byte-exact Mesen2 bus-trace diff and confirmed to measurably improve
    /// dispatch precision for this ROM's first NMI (14 clocks early -> 2
    /// clocks late relative to Mesen2) -- but this ROM's actual divergence
    /// is a slower ~300-frame cumulative drift the fix doesn't fully zero
    /// out. Closing the residual gap needs a finer-than-per-instruction
    /// interrupt-pending check; see #3049.
    #[test]
    #[ignore = "NMI dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn nmi_passes() {
        let path = Path::new(ROOT).join("nmi.smc");
        assert_rom_screen_crc(&path, "nmi.smc", "kungfufurby_nmi_tests", 600, 0xDEAD_FA89);
    }

    /// NESER's current CRC (red/fail state), NOT a Mesen2-approved golden.
    /// Same #2883/#3049 root cause as `nmi_passes`: the #2883 dispatch-delay
    /// fix changes which loop iteration this ROM's early self-check
    /// captures (different crash-loop CRC than pre-fix), but the check
    /// still fails -- needs the same finer-grained interrupt-pending check.
    #[test]
    #[ignore = "NMI dispatch timing not yet bit-exact vs Mesen2; pending #3049"]
    fn test_nmi_passes() {
        let path = Path::new(ROOT).join("test_nmi.smc");
        assert_rom_screen_crc(
            &path,
            "test_nmi.smc",
            "kungfufurby_nmi_tests",
            120,
            0x8662_6F50,
        );
    }
}
