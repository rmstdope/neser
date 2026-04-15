//! Mooneye GB test ROM integration tests.
//!
//! Mooneye tests signal completion by executing `LD B,B` (opcode 0x40).
//! Pass: B=3, C=5, D=8, E=13, H=21, L=34 (Fibonacci sequence).
//! Fail: any register deviates from the pattern above.
//!
//! ROMs are located at:
//! `roms/gb/automated_tests/mts-20240926-1737-443f6e1/`

use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;

/// Outcome of running a Mooneye test ROM to completion.
#[derive(Debug, PartialEq)]
pub enum MooneyeResult {
    /// B=3, C=5, D=8, E=13, H=21, L=34 at the `LD B,B` breakpoint.
    Pass,
    /// The `LD B,B` breakpoint was hit but registers did not match the Fibonacci pattern.
    Fail {
        b: u8,
        c: u8,
        d: u8,
        e: u8,
        h: u8,
        l: u8,
    },
    /// The ROM did not hit the breakpoint within the M-cycle budget.
    Timeout,
}

/// Mooneye pass: Fibonacci register values at `LD B,B` breakpoint.
const FIBO_B: u8 = 3;
const FIBO_C: u8 = 5;
const FIBO_D: u8 = 8;
const FIBO_E: u8 = 13;
const FIBO_H: u8 = 21;
const FIBO_L: u8 = 34;

/// Generous per-test M-cycle timeout used as a safety budget to avoid hangs.
const MOONEYE_CYCLE_LIMIT: u64 = 10_000_000;

/// LD B,B opcode used as a Mooneye software breakpoint.
const LD_B_B: u8 = 0x40;

/// Load a GB ROM from `path` and return a ready-to-step `Gb<DmgBus>`.
fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("Mooneye ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart))
}

/// Step `gb` until the Mooneye breakpoint fires or `cycle_limit` M-cycles elapse.
///
/// Detects the `LD B,B` (0x40) breakpoint by peeking at the next opcode
/// before each step. For these tests, peeking at the opcode at `PC` is safe
/// because execution is in cartridge/boot ROM space in our bus implementation.
pub(crate) fn detect_mooneye_result_with_limit(
    gb: &mut Gb<DmgBus>,
    cycle_limit: u64,
) -> MooneyeResult {
    let start = gb.cycles();
    loop {
        let opcode = gb.cpu.bus.read(gb.cpu.regs.pc);
        if opcode == LD_B_B {
            let r = &gb.cpu.regs;
            if r.b == FIBO_B
                && r.c == FIBO_C
                && r.d == FIBO_D
                && r.e == FIBO_E
                && r.h == FIBO_H
                && r.l == FIBO_L
            {
                return MooneyeResult::Pass;
            } else {
                return MooneyeResult::Fail {
                    b: r.b,
                    c: r.c,
                    d: r.d,
                    e: r.e,
                    h: r.h,
                    l: r.l,
                };
            }
        }
        if gb.cycles().saturating_sub(start) >= cycle_limit {
            return MooneyeResult::Timeout;
        }
        gb.step();
    }
}

/// Step `gb` until the Mooneye breakpoint fires or the default cycle limit is reached.
pub fn detect_mooneye_result(gb: &mut Gb<DmgBus>) -> MooneyeResult {
    detect_mooneye_result_with_limit(gb, MOONEYE_CYCLE_LIMIT)
}

/// Run a Mooneye test ROM to completion and return the result.
pub fn run_mooneye_rom(path: &str) -> MooneyeResult {
    let mut gb = load_gb_rom(path);
    detect_mooneye_result(&mut gb)
}

// ============================================================================
// Helper macro to produce a single-line pass assertion.
// ============================================================================

macro_rules! assert_mooneye_pass {
    ($path:expr) => {
        let result = run_mooneye_rom($path);
        assert_eq!(
            result,
            MooneyeResult::Pass,
            "Mooneye test failed: {:?} — ROM: {}",
            result,
            $path
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

    /// Canonical Nintendo logo bitmap (48 bytes at $0104–$0133).
    ///
    /// The boot ROM compares the cartridge header logo against this reference.
    /// Test ROMs must include it or the boot ROM will hang in an infinite loop.
    const NINTENDO_LOGO: [u8; 48] = [
        0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00,
        0x0D, 0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD,
        0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB,
        0xB9, 0x33, 0x3E,
    ];

    /// Patch `rom` with a valid Nintendo logo and correct header checksum.
    fn fix_rom_header(rom: &mut [u8]) {
        rom[0x0104..0x0134].copy_from_slice(&NINTENDO_LOGO);
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
        let mut gb = Gb::new(DmgBus::new(cart));
        assert_eq!(detect_mooneye_result(&mut gb), MooneyeResult::Pass);
    }

    #[test]
    fn helper_detects_fail_when_registers_wrong() {
        let cart = load_cartridge(&make_fail_rom()).expect("valid test ROM");
        let mut gb = Gb::new(DmgBus::new(cart));
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
        let mut gb = Gb::new(DmgBus::new(cart));
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
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_bits_unused_hwio_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/bits/unused_hwio-GS.gb"));
}

#[test]
#[ignore = "failing: boot ROM accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_boot_div_dmg0() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_div-dmg0.gb"));
}

#[test]
#[ignore = "failing: boot ROM accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_boot_div_dmgabcmgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_div-dmgABCmgb.gb"));
}

#[test]
#[ignore = "SGB-only test — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_div_s() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_div-S.gb"));
}

#[test]
#[ignore = "SGB-only test — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_div2_s() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_div2-S.gb"));
}

#[test]
#[ignore = "failing: boot ROM accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_boot_hwio_dmg0() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_hwio-dmg0.gb"));
}

#[test]
#[ignore = "failing: boot ROM accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_boot_hwio_dmgabcmgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_hwio-dmgABCmgb.gb"));
}

#[test]
#[ignore = "SGB-only test — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_hwio_s() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_hwio-S.gb"));
}

#[test]
#[ignore = "failing: boot ROM accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_boot_regs_dmg0() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-dmg0.gb"));
}

#[test]
fn test_mooneye_acceptance_boot_regs_dmgabc() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-dmgABC.gb"));
}

#[test]
#[ignore = "MGB (Game Boy Pocket) hardware — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_regs_mgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-mgb.gb"));
}

#[test]
#[ignore = "SGB-only test — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_regs_sgb() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-sgb.gb"));
}

#[test]
#[ignore = "SGB2-only test — not applicable to DMG emulation"]
fn test_mooneye_acceptance_boot_regs_sgb2() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/boot_regs-sgb2.gb"));
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
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
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
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
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
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_oam_dma_restart() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_restart.gb"));
}

#[test]
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_oam_dma_start() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_start.gb"));
}

#[test]
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
fn test_mooneye_acceptance_oam_dma_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma_timing.gb"));
}

#[test]
fn test_mooneye_acceptance_oam_dma_basic() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/oam_dma/basic.gb"));
}

#[test]
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
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
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
fn test_mooneye_acceptance_ppu_hblank_ly_scx_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/hblank_ly_scx_timing-GS.gb"));
}

#[test]
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
fn test_mooneye_acceptance_ppu_intr_1_2_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_1_2_timing-GS.gb"));
}

#[test]
fn test_mooneye_acceptance_ppu_intr_2_0_timing() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/intr_2_0_timing.gb"));
}

#[test]
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
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
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
fn test_mooneye_acceptance_ppu_lcdon_timing_gs() {
    assert_mooneye_pass!(&format!("{BASE}/acceptance/ppu/lcdon_timing-GS.gb"));
}

#[test]
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
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
#[ignore = "GS (Game Boy + SGB) specific test — run result may differ on DMG"]
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
#[ignore = "failing: MBC1 multicart mode not implemented — tracked in #2018"]
fn test_mooneye_emulator_only_mbc1_multicart_rom_8mb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/multicart_rom_8Mb.gb"));
}

#[test]
fn test_mooneye_emulator_only_mbc1_ram_256kb() {
    assert_mooneye_pass!(&format!("{BASE}/emulator-only/mbc1/ram_256kb.gb"));
}

#[test]
#[ignore = "failing: emulation accuracy gap — tracked in #2018"]
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
