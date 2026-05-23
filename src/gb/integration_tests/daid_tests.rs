//! Integration tests for the daid GB/GBC test ROMs from GBEmulatorShootout.
//!
//! The suite is vendored in `roms/gb/automated_tests/daid/`. Active tests run
//! each scoped ROM for a fixed frame count and compare the screen-buffer CRC
//! against a visually reviewed baseline. Set `NESER_CAPTURE_SCREEN=1` to write
//! screenshots to `target/daid-captures/` while baselines are being reviewed.

use std::path::Path;

use super::helpers::{
    load_cgb_rom_with_model, load_gb_rom_with_model, run_frames_and_crc, run_one_frame,
    save_screen_png,
};
use crate::gb::bus::GbBus;
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

const DAID_DIR: &str = "roms/gb/automated_tests/daid";
const DEFAULT_FRAMES: u32 = 500;
const DAID_PPU_SCANLINE_BGP_DMG_REFERENCE_CRCS: &[u32] = &[0x2299_88DA, 0x15BE_B1C2, 0x9858_EF48];

macro_rules! daid_dmg_case {
    ($(#[$meta:meta])* $test_name:ident, $capture_name:literal, $rom_name:literal, $model:expr, $expected_crc:expr) => {
        #[test]
        $(#[$meta])*
        fn $test_name() {
            let mut gb = load_gb_rom_with_model(&rom_path($rom_name), $model);
            let crc = run_case_and_crc(&mut gb, DEFAULT_FRAMES, $capture_name);
            assert_daid_crc($capture_name, DEFAULT_FRAMES, crc, $expected_crc);
        }
    };
}

macro_rules! daid_dmg_any_crc_case {
    ($(#[$meta:meta])* $test_name:ident, $capture_name:literal, $rom_name:literal, $model:expr, $expected_crcs:expr) => {
        #[test]
        $(#[$meta])*
        fn $test_name() {
            let mut gb = load_gb_rom_with_model(&rom_path($rom_name), $model);
            let crc = run_case_and_crc(&mut gb, DEFAULT_FRAMES, $capture_name);
            assert_daid_crc_in($capture_name, DEFAULT_FRAMES, crc, $expected_crcs);
        }
    };
}

macro_rules! daid_cgb_case {
    ($(#[$meta:meta])* $test_name:ident, $capture_name:literal, $rom_name:literal, $model:expr, $expected_crc:expr) => {
        #[test]
        $(#[$meta])*
        fn $test_name() {
            let mut gb = load_cgb_rom_with_model(&rom_path($rom_name), $model);
            let crc = run_case_and_crc(&mut gb, DEFAULT_FRAMES, $capture_name);
            assert_daid_crc($capture_name, DEFAULT_FRAMES, crc, $expected_crc);
        }
    };
}

fn rom_path(rom_name: &str) -> String {
    format!("{DAID_DIR}/{rom_name}")
}

fn run_case_and_crc<B: GbBus>(gb: &mut Gb<B>, frames: u32, capture_name: &str) -> u32 {
    let crc = run_frames_and_crc(gb, frames);
    capture_screen_if_requested(gb, capture_name, crc);
    crc
}

fn capture_screen_if_requested<B: GbBus>(gb: &Gb<B>, capture_name: &str, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let dir = Path::new("target/daid-captures");
    std::fs::create_dir_all(dir).expect("create daid capture directory");
    let path = dir.join(format!("{capture_name}.png"));
    save_screen_png(gb, path.to_str().expect("valid daid capture path"));
    println!("[daid] {capture_name}: CRC={crc:#010X}, PNG saved to {path:?}");
}

fn assert_daid_crc(capture_name: &str, frames: u32, crc: u32, expected_crc: u32) {
    assert_eq!(
        crc, expected_crc,
        "{capture_name} frame {frames} CRC mismatch: got {crc:#010X}, expected {expected_crc:#010X}"
    );
}

fn assert_daid_crc_in(capture_name: &str, frames: u32, crc: u32, expected_crcs: &[u32]) {
    let expected_hex: Vec<String> = expected_crcs.iter().map(|c| format!("{c:#010X}")).collect();
    assert!(
        expected_crcs.contains(&crc),
        "{capture_name} frame {frames} CRC mismatch: got {crc:#010X}, expected one of {expected_hex:?}"
    );
}

fn run_screenshot_helper<B: GbBus>(gb: &mut Gb<B>, frames: u32, capture_name: &str) {
    for _ in 0..frames {
        run_one_frame(gb);
    }
    let crc = gb.cpu.bus.ppu().screen_buffer().crc32();
    capture_screen_if_requested(gb, capture_name, crc);
}

#[test]
#[ignore = "screenshot helper - run manually with NESER_CAPTURE_SCREEN=1 for baseline review"]
fn capture_all_daid_screenshots() {
    for (capture_name, rom_name, model) in DMG_CAPTURE_CASES {
        let mut gb = load_gb_rom_with_model(&rom_path(rom_name), *model);
        run_screenshot_helper(&mut gb, DEFAULT_FRAMES, capture_name);
    }

    for (capture_name, rom_name, model) in CGB_CAPTURE_CASES {
        let mut gb = load_cgb_rom_with_model(&rom_path(rom_name), *model);
        run_screenshot_helper(&mut gb, DEFAULT_FRAMES, capture_name);
    }
}

const DMG_CAPTURE_CASES: &[(&str, &str, DmgModel)] = &[
    (
        "ppu_scanline_bgp_dmg0",
        "ppu_scanline_bgp.gb",
        DmgModel::Dmg0,
    ),
    (
        "ppu_scanline_bgp_dmga",
        "ppu_scanline_bgp.gb",
        DmgModel::DmgA,
    ),
    (
        "ppu_scanline_bgp_dmgb",
        "ppu_scanline_bgp.gb",
        DmgModel::DmgB,
    ),
    (
        "ppu_scanline_bgp_dmgc",
        "ppu_scanline_bgp.gb",
        DmgModel::DmgC,
    ),
    ("rom_and_ram_dmg0", "rom_and_ram.gb", DmgModel::Dmg0),
    ("rom_and_ram_dmga", "rom_and_ram.gb", DmgModel::DmgA),
    ("rom_and_ram_dmgb", "rom_and_ram.gb", DmgModel::DmgB),
    ("rom_and_ram_dmgc", "rom_and_ram.gb", DmgModel::DmgC),
    ("stop_instr_dmg0", "stop_instr.gb", DmgModel::Dmg0),
    ("stop_instr_dmga", "stop_instr.gb", DmgModel::DmgA),
    ("stop_instr_dmgb", "stop_instr.gb", DmgModel::DmgB),
    ("stop_instr_dmgc", "stop_instr.gb", DmgModel::DmgC),
];

const CGB_CAPTURE_CASES: &[(&str, &str, CgbModel)] = &[
    (
        "ppu_scanline_bgp_cgbe",
        "ppu_scanline_bgp.gb",
        CgbModel::CgbE,
    ),
    (
        "speed_switch_timing_div_cgbe",
        "speed_switch_timing_div.gbc",
        CgbModel::CgbE,
    ),
    (
        "speed_switch_timing_ly_cgbe",
        "speed_switch_timing_ly.gbc",
        CgbModel::CgbE,
    ),
    (
        "speed_switch_timing_stat_cgbe",
        "speed_switch_timing_stat.gbc",
        CgbModel::CgbE,
    ),
    ("stop_instr_cgbe", "stop_instr.gb", CgbModel::CgbE),
    (
        "stop_instr_gbc_mode3_cgbe",
        "stop_instr_gbc_mode3.gb",
        CgbModel::CgbE,
    ),
];

daid_dmg_any_crc_case!(
    test_ppu_scanline_bgp_dmg0_matches_reviewed_crc,
    "ppu_scanline_bgp_dmg0",
    "ppu_scanline_bgp.gb",
    DmgModel::Dmg0,
    DAID_PPU_SCANLINE_BGP_DMG_REFERENCE_CRCS
);
daid_dmg_any_crc_case!(
    test_ppu_scanline_bgp_dmga_matches_reviewed_crc,
    "ppu_scanline_bgp_dmga",
    "ppu_scanline_bgp.gb",
    DmgModel::DmgA,
    DAID_PPU_SCANLINE_BGP_DMG_REFERENCE_CRCS
);
daid_dmg_any_crc_case!(
    test_ppu_scanline_bgp_dmgb_matches_reviewed_crc,
    "ppu_scanline_bgp_dmgb",
    "ppu_scanline_bgp.gb",
    DmgModel::DmgB,
    DAID_PPU_SCANLINE_BGP_DMG_REFERENCE_CRCS
);
daid_dmg_any_crc_case!(
    test_ppu_scanline_bgp_dmgc_matches_reviewed_crc,
    "ppu_scanline_bgp_dmgc",
    "ppu_scanline_bgp.gb",
    DmgModel::DmgC,
    DAID_PPU_SCANLINE_BGP_DMG_REFERENCE_CRCS
);

daid_cgb_case!(
    test_ppu_scanline_bgp_cgbe_matches_reviewed_crc,
    "ppu_scanline_bgp_cgbe",
    "ppu_scanline_bgp.gb",
    CgbModel::CgbE,
    0x80FE_B937
);

daid_dmg_case!(
    test_rom_and_ram_dmg0_matches_reviewed_crc,
    "rom_and_ram_dmg0",
    "rom_and_ram.gb",
    DmgModel::Dmg0,
    0xCC7C_BA86
);
daid_dmg_case!(
    test_rom_and_ram_dmga_matches_reviewed_crc,
    "rom_and_ram_dmga",
    "rom_and_ram.gb",
    DmgModel::DmgA,
    0x239E_C786
);
daid_dmg_case!(
    test_rom_and_ram_dmgb_matches_reviewed_crc,
    "rom_and_ram_dmgb",
    "rom_and_ram.gb",
    DmgModel::DmgB,
    0x239E_C786
);
daid_dmg_case!(
    test_rom_and_ram_dmgc_matches_reviewed_crc,
    "rom_and_ram_dmgc",
    "rom_and_ram.gb",
    DmgModel::DmgC,
    0x239E_C786
);

daid_cgb_case!(
    test_speed_switch_timing_div_cgbe_matches_reviewed_crc,
    "speed_switch_timing_div_cgbe",
    "speed_switch_timing_div.gbc",
    CgbModel::CgbE,
    0xA4B6_C5D8
);
daid_cgb_case!(
    test_speed_switch_timing_ly_cgbe_matches_reviewed_crc,
    "speed_switch_timing_ly_cgbe",
    "speed_switch_timing_ly.gbc",
    CgbModel::CgbE,
    0xC162_BEA6
);
daid_cgb_case!(
    test_speed_switch_timing_stat_cgbe_matches_reviewed_crc,
    "speed_switch_timing_stat_cgbe",
    "speed_switch_timing_stat.gbc",
    CgbModel::CgbE,
    0xD956_3C8E
);

daid_dmg_case!(
    #[ignore = "known daid mismatch: STOP with LCD on should leave blank DMG reference output (#2613)"]
    test_stop_instr_dmg0_matches_reviewed_crc,
    "stop_instr_dmg0",
    "stop_instr.gb",
    DmgModel::Dmg0,
    0x7D75_95AD
);
daid_dmg_case!(
    #[ignore = "known daid mismatch: STOP with LCD on should leave blank DMG reference output (#2613)"]
    test_stop_instr_dmga_matches_reviewed_crc,
    "stop_instr_dmga",
    "stop_instr.gb",
    DmgModel::DmgA,
    0x6763_B6A5
);
daid_dmg_case!(
    #[ignore = "known daid mismatch: STOP with LCD on should leave blank DMG reference output (#2613)"]
    test_stop_instr_dmgb_matches_reviewed_crc,
    "stop_instr_dmgb",
    "stop_instr.gb",
    DmgModel::DmgB,
    0x6763_B6A5
);
daid_dmg_case!(
    #[ignore = "known daid mismatch: STOP with LCD on should leave blank DMG reference output (#2613)"]
    test_stop_instr_dmgc_matches_reviewed_crc,
    "stop_instr_dmgc",
    "stop_instr.gb",
    DmgModel::DmgC,
    0x6763_B6A5
);
daid_cgb_case!(
    #[ignore = "known daid mismatch: STOP with LCD on should blank the CGB reference output (#2613)"]
    test_stop_instr_cgbe_matches_reviewed_crc,
    "stop_instr_cgbe",
    "stop_instr.gb",
    CgbModel::CgbE,
    0x7D75_95AD
);
daid_cgb_case!(
    test_stop_instr_gbc_mode3_cgbe_matches_reviewed_crc,
    "stop_instr_gbc_mode3_cgbe",
    "stop_instr_gbc_mode3.gb",
    CgbModel::CgbE,
    0x4D1E_1139
);

#[cfg(test)]
mod tests {
    use super::super::helpers::decoded_png_rgb_crc;
    use super::*;
    use std::path::PathBuf;

    struct ReferencePngCase {
        name: &'static str,
        path: &'static str,
        expected_crc: u32,
    }

    const REFERENCE_PNG_CASES: &[ReferencePngCase] = &[
        ReferencePngCase {
            name: "ppu_scanline_bgp_0_dmg",
            path: "ppu_scanline_bgp_0.dmg.png",
            expected_crc: 0x2299_88DA,
        },
        ReferencePngCase {
            name: "ppu_scanline_bgp_1_dmg",
            path: "ppu_scanline_bgp_1.dmg.png",
            expected_crc: 0x15BE_B1C2,
        },
        ReferencePngCase {
            name: "ppu_scanline_bgp_2_dmg",
            path: "ppu_scanline_bgp_2.dmg.png",
            expected_crc: 0x9858_EF48,
        },
        ReferencePngCase {
            name: "ppu_scanline_bgp_gbc",
            path: "ppu_scanline_bgp.gbc.png",
            expected_crc: 0x80FE_B937,
        },
        ReferencePngCase {
            name: "speed_switch_timing_div",
            path: "speed_switch_timing_div.png",
            expected_crc: 0xA4B6_C5D8,
        },
        ReferencePngCase {
            name: "speed_switch_timing_ly",
            path: "speed_switch_timing_ly.png",
            expected_crc: 0xC162_BEA6,
        },
        ReferencePngCase {
            name: "speed_switch_timing_stat",
            path: "speed_switch_timing_stat.png",
            expected_crc: 0xD956_3C8E,
        },
        ReferencePngCase {
            name: "stop_instr_dmg",
            path: "stop_instr.dmg.png",
            expected_crc: 0x811B_B2FB,
        },
        ReferencePngCase {
            name: "stop_instr_gbc",
            path: "stop_instr.gbc.png",
            expected_crc: 0x252F_710F,
        },
        ReferencePngCase {
            name: "stop_instr_gbc_mode3",
            path: "stop_instr_gbc_mode3.png",
            expected_crc: 0x4D1E_1139,
        },
    ];

    #[test]
    fn expected_crc_constants_match_reference_pngs() {
        let mut mismatches = Vec::new();

        for case in REFERENCE_PNG_CASES {
            let path = PathBuf::from(DAID_DIR).join(case.path);
            let png_crc = decoded_png_rgb_crc(&path);
            if case.expected_crc != png_crc {
                mismatches.push(format!(
                    "{}: code={:#010X}, png={:#010X}",
                    case.name, case.expected_crc, png_crc
                ));
            }
        }

        assert!(
            mismatches.is_empty(),
            "daid reference PNG CRC mismatch(es):\n{}",
            mismatches.join("\n")
        );
    }
}
