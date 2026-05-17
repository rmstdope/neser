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

    /// Encode `ORR Rd, Rn, #imm8 ROR (rot*2)` (ARM, unconditional).
    const fn arm_orr_imm_rot(rd: u32, rn: u32, imm8: u32, rot: u32) -> u32 {
        0xE380_0000 | (rn << 16) | (rd << 12) | ((rot & 0xF) << 8) | (imm8 & 0xFF)
    }

    /// Build a sequence of ARM instructions to load a 32-bit constant into
    /// a register using MOV + ORR with rotated immediates.
    fn arm_load_const(rd: u32, value: u32) -> Vec<u32> {
        let mut instrs = Vec::new();
        let byte0 = value & 0xFF;
        let byte1 = (value >> 8) & 0xFF;
        let byte2 = (value >> 16) & 0xFF;
        let byte3 = (value >> 24) & 0xFF;

        // MOV Rd, #byte0 (no rotation)
        instrs.push(arm_mov_imm(rd, byte0));
        // ORR Rd, Rd, #byte1 ROR 24 (= byte1 << 8)
        if byte1 != 0 {
            instrs.push(arm_orr_imm_rot(rd, rd, byte1, 12));
        }
        // ORR Rd, Rd, #byte2 ROR 16 (= byte2 << 16)
        if byte2 != 0 {
            instrs.push(arm_orr_imm_rot(rd, rd, byte2, 8));
        }
        // ORR Rd, Rd, #byte3 ROR 8 (= byte3 << 24)
        if byte3 != 0 {
            instrs.push(arm_orr_imm_rot(rd, rd, byte3, 4));
        }
        instrs
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

    // ---------------------------------------------------------------
    // SWI 0x09: ArcTan tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_arctan_zero() {
        // ArcTan(0) → r0=0, r1=0, r3=0xA2F9
        let code = &[arm_mov_imm(0, 0), arm_swi(0x09), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 200_000);

        assert_eq!(gba.cpu_reg(0), 0, "ArcTan(0) result");
        assert_eq!(gba.cpu_reg(1) as i32, 0, "ArcTan(0) intermediate a");
        assert_eq!(gba.cpu_reg(3), 0xA2F9, "ArcTan(0) coefficient b");
    }

    #[test]
    fn bios_arctan_quarter() {
        // ArcTan(0x4000) → r0=0x2000, r1=0xFFFFC000, r3=0x8000
        // 0x4000 = 1.0 in s1.14 format → atan(1.0) = π/4
        let mut code = arm_load_const(0, 0x4000);
        code.push(arm_swi(0x09));
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 200_000);

        assert_eq!(gba.cpu_reg(0), 0x2000, "ArcTan(0x4000) result");
        assert_eq!(gba.cpu_reg(1), 0xFFFFC000, "ArcTan(0x4000) intermediate a");
        assert_eq!(gba.cpu_reg(3), 0x8000, "ArcTan(0x4000) coefficient b");
    }

    // ---------------------------------------------------------------
    // SWI 0x0A: ArcTan2 tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_arctan2_zero_zero() {
        // ArcTan2(0, 0) → r0=0, r3=0x170
        let code = &[
            arm_mov_imm(0, 0),
            arm_mov_imm(1, 0),
            arm_swi(0x0A),
            ARM_IDLE,
        ];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 200_000);

        assert_eq!(gba.cpu_reg(0), 0, "ArcTan2(0,0) angle");
        assert_eq!(gba.cpu_reg(3), 0x170, "ArcTan2(0,0) r3 clobber");
    }

    #[test]
    fn bios_arctan2_equal_positive() {
        // ArcTan2(0x4000, 0x4000) → r0=0x2000 (45°), r3=0x170
        let mut code = arm_load_const(0, 0x4000);
        code.extend(arm_load_const(1, 0x4000));
        code.push(arm_swi(0x0A));
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 200_000);

        assert_eq!(gba.cpu_reg(0), 0x2000, "ArcTan2(0x4000,0x4000) angle");
        assert_eq!(gba.cpu_reg(3), 0x170, "ArcTan2(0x4000,0x4000) r3");
    }

    #[test]
    fn bios_arctan2_negative_x_zero_y() {
        // ArcTan2(0xFFFF0000, 0) → r0=0x8000 (180°), r3=0x170
        let mut code = arm_load_const(0, 0xFFFF0000);
        code.push(arm_mov_imm(1, 0));
        code.push(arm_swi(0x0A));
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 200_000);

        assert_eq!(gba.cpu_reg(0), 0x8000, "ArcTan2(neg,0) angle");
        assert_eq!(gba.cpu_reg(3), 0x170, "ArcTan2(neg,0) r3");
    }

    // ---------------------------------------------------------------
    // SWI 0x0B: CpuSet tests
    // ---------------------------------------------------------------

    /// Helper: boot BIOS, write data to memory, run code, return Gba.
    fn boot_and_setup_memory(arm_code: &[u32], mem_setup: &[(u32, &[u8])]) -> Gba {
        let mut gba = boot_with_embedded_bios(arm_code);
        for &(addr, data) in mem_setup {
            for (i, &byte) in data.iter().enumerate() {
                gba.bus_mut().write8(addr + i as u32, byte);
            }
        }
        gba
    }

    #[test]
    fn bios_cpu_set_copies_halfwords() {
        // CpuSet(src=0x02000100, dst=0x02000200, count=4, 16-bit copy)
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_mov_imm(2, 4)); // count=4, bit24=0, bit26=0 → 16-bit copy
        code.push(arm_swi(0x0B));
        code.push(ARM_IDLE);

        // Source data: 4 halfwords (written as little-endian bytes)
        let halfwords: [u16; 4] = [0x1234, 0x5678, 0x9ABC, 0xDEF0];
        let src_data: Vec<u8> = halfwords.iter().flat_map(|v| v.to_le_bytes()).collect();

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        for (i, &expected) in halfwords.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read16(dst_addr + (i as u32) * 2),
                expected,
                "CpuSet halfword {i}"
            );
        }
    }

    #[test]
    fn bios_cpu_set_copies_words() {
        // CpuSet(src=0x02000100, dst=0x02000200, count=2, 32-bit copy)
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        // count=2 | bit26 (32-bit mode)
        let mut r2_code = arm_load_const(2, 2 | (1 << 26));
        code.append(&mut r2_code);
        code.push(arm_swi(0x0B));
        code.push(ARM_IDLE);

        // Source data: 2 words
        let mut src_data = Vec::new();
        src_data.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        src_data.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read32(dst_addr), 0xDEAD_BEEF, "CpuSet word 0");
        assert_eq!(
            gba.bus_mut().read32(dst_addr + 4),
            0xCAFE_BABE,
            "CpuSet word 1"
        );
    }

    #[test]
    fn bios_cpu_set_fill_mode() {
        // CpuSet fill: replicate first source word, 32-bit, count=4
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        // count=4 | bit24 (fill) | bit26 (32-bit)
        let mut r2_code = arm_load_const(2, 4 | (1 << 24) | (1 << 26));
        code.append(&mut r2_code);
        code.push(arm_swi(0x0B));
        code.push(ARM_IDLE);

        let src_data = 0xA5A5_A5A5u32.to_le_bytes();
        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        for i in 0u32..4 {
            assert_eq!(
                gba.bus_mut().read32(dst_addr + i * 4),
                0xA5A5_A5A5,
                "CpuSet fill word {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // SWI 0x0C: CpuFastSet tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_cpu_fast_set_copies_words() {
        // CpuFastSet: copy 8 words from src to dst
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_mov_imm(2, 8)); // count=8, copy mode
        code.push(arm_swi(0x0C));
        code.push(ARM_IDLE);

        // Source data: 8 sequential words
        let mut src_data = Vec::new();
        for i in 0u32..8 {
            src_data.extend_from_slice(&(0x1000_0000 + i).to_le_bytes());
        }

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        for i in 0u32..8 {
            assert_eq!(
                gba.bus_mut().read32(dst_addr + i * 4),
                0x1000_0000 + i,
                "CpuFastSet word {i}"
            );
        }
    }

    #[test]
    fn bios_cpu_fast_set_fill_mode() {
        // CpuFastSet fill: replicate first word, count=3 rounds up to 8
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        // count=3 | bit24 (fill). Rounds up to 8.
        let mut r2_code = arm_load_const(2, 3 | (1 << 24));
        code.append(&mut r2_code);
        code.push(arm_swi(0x0C));
        code.push(ARM_IDLE);

        let src_data = 0xBEEF_CAFEu32.to_le_bytes();
        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        for i in 0u32..8 {
            assert_eq!(
                gba.bus_mut().read32(dst_addr + i * 4),
                0xBEEF_CAFE,
                "CpuFastSet fill word {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // SWI 0x0E: BgAffineSet tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_bg_affine_set_identity() {
        // BgAffineSet with no rotation, scale=1.0 (0x100)
        // Source struct (20 bytes):
        //   cx=0, cy=0, disp_cx=0, disp_cy=0, scale_x=0x100, scale_y=0x100, angle=0
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        // r0=src, r1=dst, r2=1 (one calculation)
        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_mov_imm(2, 1));
        code.push(arm_swi(0x0E));
        code.push(ARM_IDLE);

        // Build source struct: 20 bytes
        let mut src_data = Vec::new();
        src_data.extend_from_slice(&0i32.to_le_bytes()); // cx (s32)
        src_data.extend_from_slice(&0i32.to_le_bytes()); // cy (s32)
        src_data.extend_from_slice(&0i16.to_le_bytes()); // disp_cx (s16)
        src_data.extend_from_slice(&0i16.to_le_bytes()); // disp_cy (s16)
        src_data.extend_from_slice(&0x0100i16.to_le_bytes()); // scale_x (s16, 1.0)
        src_data.extend_from_slice(&0x0100i16.to_le_bytes()); // scale_y (s16, 1.0)
        src_data.extend_from_slice(&0u16.to_le_bytes()); // angle=0

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 1_000_000);

        // Expected output (16 bytes):
        //   pa=0x0100 (1.0), pb=0, pc=0, pd=0x0100 (1.0), x0=0, y0=0
        let pa = gba.bus_mut().read16(dst_addr) as i16;
        let pb = gba.bus_mut().read16(dst_addr + 2) as i16;
        let pc = gba.bus_mut().read16(dst_addr + 4) as i16;
        let pd = gba.bus_mut().read16(dst_addr + 6) as i16;
        let x0 = gba.bus_mut().read32(dst_addr + 8) as i32;
        let y0 = gba.bus_mut().read32(dst_addr + 12) as i32;

        assert_eq!(pa, 0x0100, "pa should be 1.0 (0x100)");
        assert_eq!(pb, 0, "pb should be 0");
        assert_eq!(pc, 0, "pc should be 0");
        assert_eq!(pd, 0x0100, "pd should be 1.0 (0x100)");
        assert_eq!(x0, 0, "x0 should be 0");
        assert_eq!(y0, 0, "y0 should be 0");
    }

    // ---------------------------------------------------------------
    // SWI 0x0F: ObjAffineSet tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_obj_affine_set_identity() {
        // ObjAffineSet with no rotation, scale=1.0 (0x100), stride=2
        // Source struct (6 bytes): scale_x=0x100, scale_y=0x100, angle=0
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        // r0=src, r1=dst, r2=1 (one calculation), r3=2 (stride, contiguous)
        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_mov_imm(2, 1));
        code.push(arm_mov_imm(3, 2));
        code.push(arm_swi(0x0F));
        code.push(ARM_IDLE);

        // Build source struct: 6 bytes
        let mut src_data = Vec::new();
        src_data.extend_from_slice(&0x0100i16.to_le_bytes()); // scale_x (1.0)
        src_data.extend_from_slice(&0x0100i16.to_le_bytes()); // scale_y (1.0)
        src_data.extend_from_slice(&0u16.to_le_bytes()); // angle=0

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 1_000_000);

        // Expected output: PA=0x0100, PB=0, PC=0, PD=0x0100
        // With stride=2, values at dst+0, dst+2, dst+4, dst+6
        let pa = gba.bus_mut().read16(dst_addr) as i16;
        let pb = gba.bus_mut().read16(dst_addr + 2) as i16;
        let pc = gba.bus_mut().read16(dst_addr + 4) as i16;
        let pd = gba.bus_mut().read16(dst_addr + 6) as i16;

        assert_eq!(pa, 0x0100, "PA should be 1.0 (0x100)");
        assert_eq!(pb, 0, "PB should be 0");
        assert_eq!(pc, 0, "PC should be 0");
        assert_eq!(pd, 0x0100, "PD should be 1.0 (0x100)");
    }

    // ---------------------------------------------------------------
    // SWI 0x10: BitUnPack tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_bit_unpack_1bpp_to_4bpp() {
        // BitUnPack: expand 1bpp data to 4bpp with offset=1
        // Source: 1 byte = 0b1011_0001 (0xB1)
        // Bits from LSB: bit0=1, bit1=0, bit2=0, bit3=0, bit4=1, bit5=1, bit6=0, bit7=1
        // Each bit expands to a 4-bit nibble; offset=1 is added to non-zero source values:
        //   bit=1 → 1+1=2, bit=0 → 0
        // Nibbles packed LSB-first: 2,0,0,0,2,2,0,2 → 0x2022_0002
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;
        let info_addr: u32 = 0x0200_0300;

        // r0=src, r1=dst, r2=info
        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.extend(arm_load_const(2, info_addr));
        code.push(arm_swi(0x10));
        code.push(ARM_IDLE);

        // Source: 1 byte
        let src_data: Vec<u8> = vec![0b1011_0001];

        // Info struct (8 bytes):
        //   u16 src_length = 1
        //   u8  src_width  = 1 (1bpp)
        //   u8  dst_width  = 4 (4bpp)
        //   u32 data_offset = 1 (add 1 to non-zero units)
        let mut info_data = Vec::new();
        info_data.extend_from_slice(&1u16.to_le_bytes()); // src_length
        info_data.push(1); // src_width
        info_data.push(4); // dst_width
        info_data.extend_from_slice(&1u32.to_le_bytes()); // data_offset (no zero flag)

        let mut gba =
            boot_and_setup_memory(&code, &[(src_addr, &src_data), (info_addr, &info_data)]);
        run_until_idle(&mut gba, 500_000);

        let result = gba.bus_mut().read32(dst_addr);
        assert_eq!(result, 0x2022_0002, "BitUnPack 1bpp→4bpp with offset=1");
    }
}
