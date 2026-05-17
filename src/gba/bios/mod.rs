//! Embedded open-source GBA BIOS.
//!
//! The pre-built BIOS binary is included at compile time via `include_bytes!`.
//! This eliminates the need for users to provide a proprietary BIOS dump.
//!
//! The BIOS source lives in `src/gba/bios/bios.s` and can be rebuilt with
//! `make` in the `src/gba/bios/` directory (requires `arm-none-eabi` toolchain).

/// The embedded open-source GBA BIOS binary (16384 bytes).
pub const EMBEDDED_BIOS: &[u8; 16384] = include_bytes!("bios.bin");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::Gba;
    use crate::gba::bus::memory::BIOS_SIZE;
    use crate::gba::cartridge::header::{
        COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
    };
    use crate::gba::cpu::bus::Bus;
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config;
    use crate::platform::emulator::Emulator;

    // ---------------------------------------------------------------
    // Binary validation tests
    // ---------------------------------------------------------------

    #[test]
    fn embedded_bios_has_correct_size() {
        assert_eq!(EMBEDDED_BIOS.len(), BIOS_SIZE);
    }

    #[test]
    fn embedded_bios_has_valid_reset_vector() {
        let first_word = u32::from_le_bytes([
            EMBEDDED_BIOS[0],
            EMBEDDED_BIOS[1],
            EMBEDDED_BIOS[2],
            EMBEDDED_BIOS[3],
        ]);
        assert_eq!(first_word >> 24, 0xEA, "reset vector should be a branch");
    }

    #[test]
    fn embedded_bios_has_valid_swi_vector() {
        let swi_word = u32::from_le_bytes([
            EMBEDDED_BIOS[0x08],
            EMBEDDED_BIOS[0x09],
            EMBEDDED_BIOS[0x0A],
            EMBEDDED_BIOS[0x0B],
        ]);
        assert_eq!(swi_word >> 24, 0xEA, "SWI vector should be a branch");
    }

    #[test]
    fn embedded_bios_has_valid_irq_vector() {
        let irq_word = u32::from_le_bytes([
            EMBEDDED_BIOS[0x18],
            EMBEDDED_BIOS[0x19],
            EMBEDDED_BIOS[0x1A],
            EMBEDDED_BIOS[0x1B],
        ]);
        assert_eq!(irq_word >> 24, 0xEA, "IRQ vector should be a branch");
    }

    // ---------------------------------------------------------------
    // Helpers for BIOS functional tests
    // ---------------------------------------------------------------

    /// Build a minimal valid GBA ROM that contains custom ARM code at 0x08000000.
    /// The code slice is placed at the cartridge entry point.
    fn make_test_rom(arm_code: &[u32]) -> Vec<u8> {
        // Minimum ROM size must include the header area (0xC0 bytes).
        let code_bytes = arm_code.len() * 4;
        let rom_size = (0xC0 + code_bytes).max(0x100);
        let mut rom = vec![0u8; rom_size];

        // Place the ARM code at offset 0 (maps to 0x08000000).
        for (i, &word) in arm_code.iter().enumerate() {
            let offset = i * 4;
            rom[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }

        // Fix up the required header fields for cartridge validation.
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    /// Create a GBA emulator with the embedded open-source BIOS loaded,
    /// HLE disabled, and the given test ROM inserted. Returns the Gba
    /// instance ready to run from the cartridge entry point.
    fn boot_with_embedded_bios(arm_code: &[u32]) -> Gba {
        let mut config = Config::default();
        // Use a guaranteed-nonexistent path so Gba::new() falls back to
        // the embedded BIOS.
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        // HLE is already disabled by Gba::new() when using embedded BIOS,
        // but set explicitly for clarity in tests.
        gba.set_hle_swi(false);

        // Load test ROM
        let rom = make_test_rom(arm_code);
        gba.load_rom(&rom, "bios-test.gba")
            .expect("test ROM should load with embedded BIOS");

        // Run the boot sequence (reset handler) until PC reaches cartridge
        // entry point at 0x08000000. The BIOS sets up stacks and jumps.
        let mut cycles = 0u64;
        while cycles < 10_000 {
            let pc = gba.cpu_pc();
            if pc >= 0x0800_0000 {
                break;
            }
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
        }
        assert!(
            gba.cpu_pc() >= 0x0800_0000,
            "BIOS should boot to cartridge entry point, got PC={:#010X}",
            gba.cpu_pc()
        );

        gba
    }

    /// Run the emulator until PC reaches an `idle: b idle` (branch-to-self)
    /// instruction, or until the cycle limit is hit.
    fn run_until_idle(gba: &mut Gba, max_cycles: u64) {
        let mut cycles = 0u64;
        let mut last_pc = None;
        while cycles < max_cycles {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;

            let pc = gba.cpu_pc();
            if Some(pc) == last_pc {
                // Likely stuck in branch-to-self (idle loop)
                break;
            }
            last_pc = Some(pc);
        }
    }

    // ARM instruction encodings for test programs
    // Branch-to-self (idle loop): B .
    const ARM_IDLE: u32 = 0xEAFF_FFFE;

    /// Encode `MOV Rd, #imm8` (ARM, unconditional).
    const fn arm_mov_imm(rd: u32, imm8: u32) -> u32 {
        0xE3A0_0000 | (rd << 12) | (imm8 & 0xFF)
    }

    /// Encode `SWI #imm` (ARM, unconditional). GBA uses bits 23:16 for SWI number.
    const fn arm_swi(swi_num: u32) -> u32 {
        0xEF00_0000 | ((swi_num & 0xFF) << 16)
    }

    /// Encode `MVN Rd, #imm8` (bitwise NOT immediate).
    const fn arm_mvn_imm(rd: u32, imm8: u32) -> u32 {
        0xE3E0_0000 | (rd << 12) | (imm8 & 0xFF)
    }

    /// Encode `MOV Rd, #imm8 ROR (rot*2)` (ARM, unconditional).
    const fn arm_mov_imm_rot(rd: u32, imm8: u32, rot: u32) -> u32 {
        0xE3A0_0000 | (rd << 12) | ((rot & 0xF) << 8) | (imm8 & 0xFF)
    }

    // ---------------------------------------------------------------
    // SWI 0x06: Div tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_div_positive_values() {
        // 7 / 3 = quotient 2, remainder 1, abs(quotient) = 2
        let code = &[
            arm_mov_imm(0, 7), // r0 = 7 (numerator)
            arm_mov_imm(1, 3), // r1 = 3 (denominator)
            arm_swi(0x06),     // SWI Div
            ARM_IDLE,          // idle loop
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 2, "quotient of 7/3");
        assert_eq!(gba.cpu_reg(1), 1, "remainder of 7/3");
        assert_eq!(gba.cpu_reg(3), 2, "abs(quotient) of 7/3");
    }

    #[test]
    fn bios_div_negative_numerator() {
        // -7 / 3 = quotient -2, remainder -1, abs(quotient) = 2
        // ARM encoding: MVN r0, #6 gives r0 = ~6 = -7 (0xFFFFFFF9)
        let code = &[
            arm_mvn_imm(0, 6), // r0 = -7
            arm_mov_imm(1, 3), // r1 = 3
            arm_swi(0x06),     // SWI Div
            ARM_IDLE,
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0) as i32, -2, "quotient of -7/3");
        assert_eq!(gba.cpu_reg(1) as i32, -1, "remainder of -7/3");
        assert_eq!(gba.cpu_reg(3), 2, "abs(quotient) of -7/3");
    }

    #[test]
    fn bios_div_exact() {
        // 10 / 5 = quotient 2, remainder 0
        let code = &[
            arm_mov_imm(0, 10),
            arm_mov_imm(1, 5),
            arm_swi(0x06),
            ARM_IDLE,
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 2, "quotient of 10/5");
        assert_eq!(gba.cpu_reg(1), 0, "remainder of 10/5");
        assert_eq!(gba.cpu_reg(3), 2, "abs(quotient) of 10/5");
    }

    #[test]
    fn bios_div_large_dividend_does_not_hang() {
        // 0x80000000 / 1 = 0x80000000 (tests overflow guard in shift loop)
        // MOV r0, #0x80 ROR 2 = 0x80000000 (as signed: -2147483648)
        let code = &[
            arm_mov_imm_rot(0, 0x02, 1), // r0 = 0x80000000
            arm_mov_imm(1, 1),           // r1 = 1
            arm_swi(0x06),               // SWI Div
            ARM_IDLE,
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 500_000);

        // -2147483648 / 1 = quotient -2147483648, remainder 0
        assert_eq!(
            gba.cpu_reg(0) as i32,
            -2_147_483_648i32,
            "quotient of 0x80000000/1"
        );
        assert_eq!(gba.cpu_reg(1), 0, "remainder of 0x80000000/1");
    }

    // ---------------------------------------------------------------
    // SWI 0x07: DivArm tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_div_arm_swaps_operands() {
        // DivArm: r0=denominator, r1=numerator → 7/3
        let code = &[
            arm_mov_imm(0, 3), // r0 = 3 (denominator for DivArm)
            arm_mov_imm(1, 7), // r1 = 7 (numerator for DivArm)
            arm_swi(0x07),     // SWI DivArm
            ARM_IDLE,
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 2, "quotient of 7/3 via DivArm");
        assert_eq!(gba.cpu_reg(1), 1, "remainder of 7/3 via DivArm");
        assert_eq!(gba.cpu_reg(3), 2, "abs(quotient) of 7/3 via DivArm");
    }

    // ---------------------------------------------------------------
    // SWI 0x08: Sqrt tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_sqrt_perfect_square() {
        // sqrt(16) = 4
        let code = &[arm_mov_imm(0, 16), arm_swi(0x08), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 4, "sqrt(16)");
    }

    #[test]
    fn bios_sqrt_non_perfect() {
        // sqrt(10) = 3 (floor)
        let code = &[arm_mov_imm(0, 10), arm_swi(0x08), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 3, "floor(sqrt(10))");
    }

    #[test]
    fn bios_sqrt_zero() {
        let code = &[arm_mov_imm(0, 0), arm_swi(0x08), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 0, "sqrt(0)");
    }

    #[test]
    fn bios_sqrt_one() {
        let code = &[arm_mov_imm(0, 1), arm_swi(0x08), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(gba.cpu_reg(0), 1, "sqrt(1)");
    }

    // ---------------------------------------------------------------
    // SWI 0x0D: BiosChecksum tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_checksum_returns_identifier() {
        let code = &[arm_swi(0x0D), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        // Our open-source BIOS returns "NESE" (0x4E455345)
        assert_eq!(
            gba.cpu_reg(0),
            0x4E45_5345,
            "BiosChecksum should return open-source identifier"
        );
    }

    // ---------------------------------------------------------------
    // Boot sequence tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_boot_sets_postflg() {
        let code = &[ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        // POSTFLG at 0x04000300 should be 1 after boot
        let postflg = gba.bus_mut().read8(0x04000300);
        assert_eq!(postflg, 1, "POSTFLG should be 1 after BIOS boot");
    }
}
