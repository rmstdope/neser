//! The SNES core is deterministic (nr-ayk5.1): two consoles fed the same ROM and pad presses
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
                assert_two_instances_agree(SystemType::Snes, $path, &RUN);
            })*
        }
        mod save_mid_run_continues_identically {
            use super::*;
            $(#[test]
            fn $name() {
                assert_save_mid_run_continues_identically(SystemType::Snes, $path, &RUN);
            })*
        }
    };
}

determinism_tests! {
    tellinglys_pads: "roms/snes/automated_tests/snes_test_roms/tepples-tellinglys/tellinglys.sfc",
    mode7_rotzoom: "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-Mode7/RotZoom/RotZoom.sfc",
    mode7_starwars: "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-Mode7/StarWars/StarWars.sfc",
    spc_dsp: "roms/snes/automated_tests/blargg_apu/spc_dsp6.sfc",
    super_fx: "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CHIP-GSU-GSUTest/MOVES/GSUMOVES.sfc",
    sa1: "roms/snes/automated_tests/snes_test_roms/absindx/SA1VersionCodeTest.sfc",
    cx4: "roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/cx4test/cx4test.sfc",
}
