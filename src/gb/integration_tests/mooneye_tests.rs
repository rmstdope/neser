//! Mooneye GB test ROM integration tests.
//!
//! Mooneye tests signal completion by executing `LD B,B` (opcode 0x40).
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34 (Fibonacci sequence).
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/mts-20240926-1737-443f6e1/`
//!
//! # Excluded ROMs
//!
//! The following ROMs are intentionally excluded because they target GB
//! hardware variants not supported by the GB core (SGB, MGB, AGB/AGS):
//!
//! ## SGB / SGB2 (Super Game Boy)
//! - `acceptance/boot_div-S.gb`
//! - `acceptance/boot_div2-S.gb`
//! - `acceptance/boot_hwio-S.gb`
//! - `acceptance/boot_regs-sgb.gb`
//! - `acceptance/boot_regs-sgb2.gb`
//!
//! ## MGB (Game Boy Pocket)
//! - `acceptance/boot_regs-mgb.gb`
//!
//! ## AGB / AGS (Game Boy Advance)
//! - `misc/boot_div-A.gb`
//! - `misc/boot_regs-A.gb`

use std::path::Path;

use super::helpers::{
    MooneyeResult, detect_mooneye_result_with_limit, load_gb_rom, load_gb_rom_with_model,
    run_and_detect_cgb, run_and_detect_dmg, run_frames_and_crc, save_screen_png,
};
use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::console::Gb;
use crate::gb::model::{CgbModel, DmgModel};

/// Generous per-test M-cycle timeout used as a safety budget to avoid hangs.
const MOONEYE_CYCLE_LIMIT: u64 = 15_000_000;

/// Step `gb` until the Mooneye breakpoint fires or the default cycle limit is reached.
pub fn detect_mooneye_result(gb: &mut Gb<DmgBus>) -> MooneyeResult {
    detect_mooneye_result_with_limit(gb, MOONEYE_CYCLE_LIMIT)
}

// ============================================================================
// Helper macro to produce a single-line pass assertion.
// ============================================================================

macro_rules! assert_mooneye_pass {
    ($path:expr) => {
        let result = run_and_detect_dmg($path, DmgModel::DmgB, MOONEYE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye test failed: {:?} — ROM: {}",
            result,
            $path
        );
    };
}

macro_rules! assert_mooneye_pass_dmg0 {
    ($path:expr) => {
        let result = run_and_detect_dmg($path, DmgModel::Dmg0, MOONEYE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye DMG-0 test failed: {:?} — ROM: {}",
            result,
            $path
        );
    };
}

macro_rules! assert_mooneye_pass_dmg_a {
    ($path:expr) => {
        let result = run_and_detect_dmg($path, DmgModel::DmgA, MOONEYE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye DMG-A test failed: {:?} — ROM: {}",
            result,
            $path
        );
    };
}

macro_rules! assert_mooneye_pass_dmg_c {
    ($path:expr) => {
        let result = run_and_detect_dmg($path, DmgModel::DmgC, MOONEYE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye DMG-C test failed: {:?} — ROM: {}",
            result,
            $path
        );
    };
}

/// CGB test macro with model parameter.
///
/// Runs a Mooneye test ROM on CGB hardware with the specified model variant.
/// Pan Docs: CGB models differ in post-boot CPU register values and DIV initial state.
macro_rules! assert_mooneye_pass_cgb {
    ($path:expr, $model:expr) => {
        let result = run_and_detect_cgb($path, $model, MOONEYE_CYCLE_LIMIT);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye CGB test failed: {:?} — ROM: {} — model: {:?}",
            result,
            $path,
            $model
        );
    };
}

// ============================================================================
// Unit tests for the run_mooneye_rom helper itself
// ============================================================================

#[cfg(test)]
mod helper_tests {
    use super::*;
    use crate::gb::bus::DmgBus;
    use crate::gb::cartridge::load_cartridge;
    use crate::gb::console::Gb;

    /// Patch `rom` with deterministic header-logo bytes and correct header checksum.
    fn fix_rom_header(rom: &mut [u8]) {
        for (index, byte) in rom[0x0104..0x0134].iter_mut().enumerate() {
            *byte = ((index as u8).wrapping_mul(17)) ^ 0x5A;
        }
        let checksum = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = checksum;
    }

    /// Build a tiny MBC0 ROM that immediately executes `LD B,B` with Fibonacci registers set.
    ///
    /// Standard GB header layout:
    ///   $0100–$0103: NOP; JP $0150  (standard entry point)
    ///   $0104–$0133: Nintendo logo  (required for boot ROM)
    ///   $0134–$014D: title + checksum
    ///   $0150+:      test program
    fn make_pass_rom() -> Vec<u8> {
        let mut rom = vec![0x00u8; 32 * 1024]; // MBC0: 32 KB
        // Standard entry point: NOP; JP $0150
        rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]);
        // Test program at $0150 (past the header)
        let program: &[u8] = &[
            0x06, 3, // LD B, 3
            0x0E, 5, // LD C, 5
            0x16, 8, // LD D, 8
            0x1E, 13, // LD E, 13
            0x26, 21, // LD H, 21
            0x2E, 34,   // LD L, 34
            0x40, // LD B, B  (Mooneye breakpoint)
            0x76, // HALT
        ];
        rom[0x0150..0x0150 + program.len()].copy_from_slice(program);
        fix_rom_header(&mut rom);
        rom
    }

    /// Build a tiny MBC0 ROM that immediately executes `LD B,B` with WRONG registers.
    fn make_fail_rom() -> Vec<u8> {
        let mut rom = vec![0x00u8; 32 * 1024];
        rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]);
        let program: &[u8] = &[
            0x06, 0xFF, // LD B, 0xFF  (wrong — ≠ Fibonacci 3)
            0x40, // LD B, B     (Mooneye breakpoint)
            0x76, // HALT
        ];
        rom[0x0150..0x0150 + program.len()].copy_from_slice(program);
        fix_rom_header(&mut rom);
        rom
    }

    /// Build a tiny MBC0 ROM that loops forever (no LD B,B).
    fn make_timeout_rom() -> Vec<u8> {
        let mut rom = vec![0x00u8; 32 * 1024];
        rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]);
        let program: &[u8] = &[
            0xC3, 0x50, 0x01, // JP $0150  — infinite loop
        ];
        rom[0x0150..0x0150 + program.len()].copy_from_slice(program);
        fix_rom_header(&mut rom);
        rom
    }

    #[test]
    fn helper_detects_pass_when_fibonacci_registers_set() {
        let cart = load_cartridge(&make_pass_rom()).expect("valid test ROM");
        let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));
        assert_eq!(detect_mooneye_result(&mut gb), MooneyeResult::Pass);
    }

    #[test]
    fn helper_detects_fail_when_registers_wrong() {
        let cart = load_cartridge(&make_fail_rom()).expect("valid test ROM");
        let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));
        let result = detect_mooneye_result(&mut gb);
        assert!(
            matches!(result, MooneyeResult::Fail { b: 0xFF, .. }),
            "expected Fail with b=0xFF, got: {result:?}"
        );
    }

    #[test]
    fn helper_returns_timeout_when_no_breakpoint() {
        let bytes = make_timeout_rom();
        let cart = load_cartridge(&bytes).expect("valid test ROM");
        let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));
        assert_eq!(
            detect_mooneye_result_with_limit(&mut gb, 1_000),
            MooneyeResult::Timeout
        );
    }
}

// ============================================================================
// acceptance/ tests
// ============================================================================

const BASE: &str = "roms/gb/automated_tests/mts-20240926-1737-443f6e1";
const SPRITE_PRIORITY_FRAMES: u32 = 500;

fn run_mooneye_visual_case_and_crc<B: GbBus>(
    gb: &mut Gb<B>,
    frames: u32,
    capture_name: &str,
) -> u32 {
    let crc = run_frames_and_crc(gb, frames);
    capture_mooneye_screen_if_requested(gb, capture_name, crc);
    crc
}

fn capture_mooneye_screen_if_requested<B: GbBus>(gb: &Gb<B>, capture_name: &str, crc: u32) {
    if std::env::var_os("NESER_CAPTURE_SCREEN").is_none() {
        return;
    }

    let dir = Path::new("target/mooneye-captures");
    std::fs::create_dir_all(dir).expect("create mooneye capture directory");
    let path = dir.join(format!("{capture_name}.png"));
    save_screen_png(gb, path.to_str().expect("valid mooneye capture path"));
    println!("[mooneye] {capture_name}: CRC={crc:#010X}, PNG saved to {path:?}");
}

#[test]
fn test_mooneye_acceptance_add_sp_e_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/add_sp_e_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_bits_mem_oam() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/bits/mem_oam.gb"));
}

#[test]
fn test_mooneye_acceptance_bits_reg_f() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/bits/reg_f.gb"));
}

#[test]
fn test_mooneye_acceptance_bits_unused_hwio_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/bits/unused_hwio-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_div_dmg0() {
    assert_mooneye_pass_dmg0!(&format!("{BASE}/acceptance/boot_div-dmg0.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_div_dmgabcmgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_div-dmgABCmgb.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_hwio_dmg0() {
    assert_mooneye_pass_dmg0!(&format!("{BASE}/acceptance/boot_hwio-dmg0.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_hwio_dmgabcmgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_hwio-dmgABCmgb.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_regs_dmg0() {
    assert_mooneye_pass_dmg0!(&format!("{BASE}/acceptance/boot_regs-dmg0.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_regs_dmgabc() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-dmgABC.gb"));
}

#[test]
fn test_mooneye_acceptance_call_cc_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/call_cc_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_call_cc_timing2() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/call_cc_timing2.gb"));
}

#[test]
fn test_mooneye_acceptance_call_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/call_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_call_timing2() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/call_timing2.gb"));
}

#[test]
fn test_mooneye_acceptance_di_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/di_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_div_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/div_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ei_sequence() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ei_sequence.gb"));
}

#[test]
fn test_mooneye_acceptance_ei_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ei_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_halt_ime0_ei() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/halt_ime0_ei.gb"));
}

#[test]
fn test_mooneye_acceptance_halt_ime0_nointr_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/halt_ime0_nointr_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_halt_ime1_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/halt_ime1_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_halt_ime1_timing2_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/halt_ime1_timing2-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_if_ie_registers() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/if_ie_registers.gb"));
}

#[test]
fn test_mooneye_acceptance_instr_daa() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/instr/daa.gb"));
}

#[test]
fn test_mooneye_acceptance_interrupts_ie_push() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/interrupts/ie_push.gb"));
}

#[test]
fn test_mooneye_acceptance_intr_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/intr_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_jp_cc_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/jp_cc_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_jp_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/jp_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ld_hl_sp_e_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ld_hl_sp_e_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_restart() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_restart.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_start() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_start.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_basic() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma/basic.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_reg_read() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma/reg_read.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_sources_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma/sources-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_pop_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/pop_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_hblank_ly_scx_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/hblank_ly_scx_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_1_2_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_1_2_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_0_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_2_0_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_mode0_timing_sprites() {
    assert_mooneye_pass!(&format!(
        "{BASE}/acceptance/ppu/intr_2_mode0_timing_sprites.gb"
    ));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_mode0_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_2_mode0_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_mode3_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_2_mode3_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_oam_ok_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_2_oam_ok_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_lcdon_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/lcdon_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_lcdon_write_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/lcdon_write_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_stat_irq_blocking() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/stat_irq_blocking.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_stat_lyc_onoff() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/stat_lyc_onoff.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_vblank_stat_intr_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/vblank_stat_intr-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_push_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/push_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_rapid_di_ei() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/rapid_di_ei.gb"));
}

#[test]
fn test_mooneye_acceptance_ret_cc_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ret_cc_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_ret_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ret_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_reti_intr_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/reti_intr_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_reti_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/reti_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_rst_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/rst_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_serial_boot_sclk_align_dmgabcmgb() {
    assert_mooneye_pass!(&format!(
        "{BASE}/acceptance/serial/boot_sclk_align-dmgABCmgb.gb"
    ));
}

#[test]
fn test_mooneye_acceptance_timer_div_write() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/div_write.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_rapid_toggle() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/rapid_toggle.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim00_div_trigger() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim00_div_trigger.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim00() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim00.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim01_div_trigger() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim01_div_trigger.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim01() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim01.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim10_div_trigger() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim10_div_trigger.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim10() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim10.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim11_div_trigger() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim11_div_trigger.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tim11() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tim11.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tima_reload() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tima_reload.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tima_write_reloading() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tima_write_reloading.gb"));
}

#[test]
fn test_mooneye_acceptance_timer_tma_write_reloading() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/timer/tma_write_reloading.gb"));
}

// ============================================================================
// emulator-only/ tests — MBC1 (implemented)
// ============================================================================

#[test]
fn test_mooneye_emulator_only_mbc1_bits_bank1() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/bits_bank1.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_bits_bank2() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/bits_bank2.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_bits_mode() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/bits_mode.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_bits_ramg() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/bits_ramg.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_multicart_rom_8mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/multicart_rom_8Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_ram_256kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/ram_256kb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_ram_64kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/ram_64kb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_16mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_16Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_1mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_1Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_2mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_2Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_4mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_4Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_512kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_512kb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_rom_8mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/rom_8Mb.gb"));
}

// ============================================================================
// emulator-only/ tests — MBC2
// ============================================================================

#[test]
fn test_mooneye_emulator_only_mbc2_bits_ramg() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/bits_ramg.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_bits_romb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/bits_romb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_bits_unused() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/bits_unused.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_ram() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/ram.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_rom_1mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/rom_1Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_rom_2mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/rom_2Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc2_rom_512kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc2/rom_512kb.gb"));
}

// ============================================================================
// emulator-only/ tests — MBC5
// ============================================================================

#[test]
fn test_mooneye_emulator_only_mbc5_rom_16mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_16Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_1mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_1Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_2mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_2Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_32mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_32Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_4mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_4Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_512kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_512kb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_64mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_64Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc5_rom_8mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc5/rom_8Mb.gb"));
}

// ============================================================================
// misc/ tests
// ============================================================================

#[test]
fn test_mooneye_misc_bits_unused_hwio_c() {
    assert_mooneye_pass_cgb!(
        &format!("{BASE}/misc/bits/unused_hwio-C.gb"),
        CgbModel::CgbE
    );
}

#[test]
fn test_mooneye_misc_boot_div_cgb0() {
    assert_mooneye_pass_cgb!(&format!("{BASE}/misc/boot_div-cgb0.gb"), CgbModel::Cgb0);
}

#[test]
fn test_mooneye_misc_boot_div_cgbabcde() {
    assert_mooneye_pass_cgb!(&format!("{BASE}/misc/boot_div-cgbABCDE.gb"), CgbModel::CgbE);
}

#[test]
fn test_mooneye_misc_boot_hwio_c() {
    assert_mooneye_pass_cgb!(&format!("{BASE}/misc/boot_hwio-C.gb"), CgbModel::CgbE);
}

#[test]
fn test_mooneye_misc_boot_regs_cgb() {
    assert_mooneye_pass_cgb!(&format!("{BASE}/misc/boot_regs-cgb.gb"), CgbModel::CgbE);
}

#[test]
fn test_mooneye_misc_ppu_vblank_stat_intr_c() {
    assert_mooneye_pass_cgb!(
        &format!("{BASE}/misc/ppu/vblank_stat_intr-C.gb"),
        CgbModel::CgbE
    );
}

// ============================================================================
// manual-only/ visual tests
// ============================================================================

#[test]
fn test_mooneye_manual_only_sprite_priority_dmg_b_matches_reference_crc() {
    let path = format!("{BASE}/manual-only/sprite_priority.gb");
    let mut gb = load_gb_rom_with_model(&path, DmgModel::DmgB);
    let crc =
        run_mooneye_visual_case_and_crc(&mut gb, SPRITE_PRIORITY_FRAMES, "sprite_priority_dmg_b");

    const EXPECTED_CRC: u32 = 0x8ADF_6D41;
    assert_eq!(
        crc, EXPECTED_CRC,
        "sprite_priority_dmg_b frame {SPRITE_PRIORITY_FRAMES} CRC mismatch: got {crc:#010X}, expected {EXPECTED_CRC:#010X}"
    );
}

#[test]
#[ignore = "screenshot helper - run manually with NESER_CAPTURE_SCREEN=1 for baseline review"]
fn capture_mooneye_sprite_priority_screenshot() {
    let path = format!("{BASE}/manual-only/sprite_priority.gb");
    let mut gb = load_gb_rom_with_model(&path, DmgModel::DmgB);
    run_mooneye_visual_case_and_crc(&mut gb, SPRITE_PRIORITY_FRAMES, "sprite_priority_dmg_b");
}

// ============================================================================
// Per-variant boot tests — verify DMG-A and DMG-C pass the same boot tests
// that DMG-B already passes.
// ============================================================================

#[test]
fn test_mooneye_acceptance_boot_regs_dmgabc_with_dmg_a() {
    assert_mooneye_pass_dmg_a!(&format!("{BASE}/acceptance/boot_regs-dmgABC.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_regs_dmgabc_with_dmg_c() {
    assert_mooneye_pass_dmg_c!(&format!("{BASE}/acceptance/boot_regs-dmgABC.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_div_dmgabcmgb_with_dmg_a() {
    assert_mooneye_pass_dmg_a!(&format!("{BASE}/acceptance/boot_div-dmgABCmgb.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_div_dmgabcmgb_with_dmg_c() {
    assert_mooneye_pass_dmg_c!(&format!("{BASE}/acceptance/boot_div-dmgABCmgb.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_hwio_dmgabcmgb_with_dmg_a() {
    assert_mooneye_pass_dmg_a!(&format!("{BASE}/acceptance/boot_hwio-dmgABCmgb.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_hwio_dmgabcmgb_with_dmg_c() {
    assert_mooneye_pass_dmg_c!(&format!("{BASE}/acceptance/boot_hwio-dmgABCmgb.gb"));
}

/// `madness/mgb_oam_dma_halt_sprites.gb` is intentionally designed to hang
/// in HALT forever: it sets up a sprite frame, kicks off an OAM DMA from
/// `$2000`, and then executes `HALT` from HRAM with IME=0 and IE=0. With
/// no interrupts enabled, the CPU never wakes — on real hardware, the
/// "result" of the test is the visual output produced by the PPU as it
/// continues to draw while OAM is frozen mid-DMA. The ROM contains no
/// `LD B,B` pass marker, so the standard Mooneye pass detection cannot
/// be applied.
///
/// Our automated check verifies that after running long enough for the
/// HALT to be reached, the CPU is halted at the `NOP` immediately
/// following the HALT instruction in HRAM (`$FF97`), with no interrupts
/// enabled. This confirms the emulator correctly executes the HRAM test
/// payload up to and including HALT and then stays halted, matching the
/// behaviour the ROM is designed to expose.
#[test]
fn test_mooneye_madness_mgb_oam_dma_halt_sprites() {
    let path = format!("{BASE}/madness/mgb_oam_dma_halt_sprites.gb");
    let mut gb = load_gb_rom(&path);
    // Step until the CPU has entered HALT (or a generous safety budget
    // expires). The HRAM payload runs `wait_vblank` loops before HALT;
    // reaching HALT takes around 10M M-cycles in this ROM.
    let start = gb.cycles();
    let limit: u64 = MOONEYE_CYCLE_LIMIT;
    while !gb.cpu.halted && gb.cycles().saturating_sub(start) < limit {
        gb.step();
    }
    assert!(
        gb.cpu.halted,
        "expected CPU to enter HALT in HRAM payload (PC=${:04x})",
        gb.cpu.regs.pc
    );
    // After HALT executes, PC points at the NOP at $FF97 (HALT itself is at $FF96).
    assert_eq!(
        gb.cpu.regs.pc, 0xFF97,
        "CPU halted at unexpected PC: ${:04x}",
        gb.cpu.regs.pc
    );
    // IE=0 and IME=0 — no interrupt can wake the CPU, exactly as the
    // ROM intends. This confirms the test reached the designed hang.
    assert_eq!(
        gb.cpu.bus.read(0xFFFF),
        0x00,
        "expected IE=0 (ROM never enables any interrupts)"
    );
    assert!(!gb.cpu.ime, "expected IME=0 at HALT");
    assert!(
        !gb.cpu.ime_pending(),
        "expected no pending IME enable while halted"
    );

    // Step a further budget of M-cycles and verify the CPU remains
    // halted at $FF97. This catches regressions where a spurious wake
    // path (e.g., incorrect HALT exit on an unrelated condition) would
    // resume execution even though no interrupt is pending.
    for _ in 0..100_000u64 {
        gb.step();
        assert!(
            gb.cpu.halted,
            "CPU spuriously woke from HALT (PC=${:04x})",
            gb.cpu.regs.pc
        );
        assert_eq!(
            gb.cpu.regs.pc, 0xFF97,
            "PC drifted while halted: ${:04x}",
            gb.cpu.regs.pc
        );
    }
}
