//! The NES core is deterministic (nr-ayk5.1): two consoles fed the same ROM and pad presses
//! agree every frame, and a save loaded mid-run into a fresh console continues identically.
//! See [`crate::platform::determinism`].

use crate::platform::determinism::{
    DeterminismRun, assert_save_mid_run_continues_identically, assert_two_instances_agree,
};
use crate::platform::emulator::SystemType;

const RUN: DeterminismRun = DeterminismRun {
    frames: 120,
    save_at: 60,
};

macro_rules! determinism_tests {
    ($($name:ident: $path:expr,)*) => {
        mod two_instances_agree_every_frame {
            use super::*;
            $(#[test]
            fn $name() {
                assert_two_instances_agree(SystemType::Nes, $path, &RUN);
            })*
        }
        mod save_mid_run_continues_identically {
            use super::*;
            $(#[test]
            fn $name() {
                assert_save_mid_run_continues_identically(SystemType::Nes, $path, &RUN);
            })*
        }
    };
}

determinism_tests! {
    allpads: "roms/nes/automated_tests/allpads-r9/allpads.nes",
    ruder: "roms/nes/automated_tests/ruder-0.03/ruder.nes",
    mapper28_test28: "roms/nes/automated_tests/test28-0.04/test28.nes",
    ntsc_torture: "roms/nes/rainwarrior/ntsc_torture.nes",
    dmc_dma: "roms/nes/automated_tests/apu_mixer/dmc.nes",
    mmc3_scanline_timing: "roms/nes/automated_tests/mmc3_test_2/rom_singles/4-scanline_timing.nes",
    mmc5test: "roms/nes/automated_tests/mmc5test_v2/mmc5test.nes",
    vrc6test: "roms/nes/automated_tests/vrc6test/vrc6test24.nes",
    fme7acktest: "roms/nes/automated_tests/fme7acktest-r1/fme7acktest.nes",
    namco163_verification: "roms/nes/automated_tests/mapper_verification/bin/m019.0.nes",
}
