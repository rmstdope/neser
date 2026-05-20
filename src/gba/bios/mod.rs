//! Embedded open-source GBA BIOS.
//!
//! The pre-built BIOS binary is included at compile time via `include_bytes!`.
//! This eliminates the need for users to provide a proprietary BIOS dump.
//!
//! The BIOS source lives in `src/gba/bios/bios.s` and can be rebuilt with
//! `make` in the `src/gba/bios/` directory (requires `arm-none-eabi` toolchain).
//!
//! ## Implemented SWIs
//!
//! | SWI   | Name              | Status |
//! |-------|-------------------|--------|
//! | 0x00  | SoftReset         | Full   |
//! | 0x01  | RegisterRamReset  | Full   |
//! | 0x02  | Halt              | Full   |
//! | 0x03  | Stop              | Stub (halts only, no deep-sleep) |
//! | 0x04  | IntrWait          | Full   |
//! | 0x05  | VBlankIntrWait    | Full   |
//! | 0x06  | Div               | Full   |
//! | 0x07  | DivArm            | Full   |
//! | 0x08  | Sqrt              | Full   |
//! | 0x09  | ArcTan            | Full   |
//! | 0x0A  | ArcTan2           | Full   |
//! | 0x0B  | CpuSet            | Full   |
//! | 0x0C  | CpuFastSet        | Full   |
//! | 0x0D  | BiosChecksum      | Full   |
//! | 0x0E  | BgAffineSet       | Full   |
//! | 0x0F  | ObjAffineSet      | Full   |
//! | 0x10  | BitUnPack          | Full   |
//! | 0x11  | LZ77UnCompWram    | Full   |
//! | 0x12  | LZ77UnCompVram    | Full   |
//! | 0x13  | HuffUnComp        | Full   |
//! | 0x14  | RLUnCompWram      | Full   |
//! | 0x15  | RLUnCompVram      | Full   |
//! | 0x16  | Diff8bitUnFilterWram  | Full |
//! | 0x17  | Diff8bitUnFilterVram  | Full |
//! | 0x18  | Diff16bitUnFilter | Full   |
//! | 0x19  | SoundBias         | Full   |
//! | 0x1A  | SoundDriverInit   | Stub — no sound mixer |
//! | 0x1B  | SoundDriverMode   | Stub — no sound mixer |
//! | 0x1C  | SoundDriverMain   | Stub — no sound mixer |
//! | 0x1D  | SoundDriverVSync  | Stub — no sound mixer |
//! | 0x1E  | SoundChannelClear | Stub — no sound mixer |
//! | 0x1F  | MidiKey2Freq      | Full (integer-only LUT, minor rounding vs official) |
//! | 0x20–0x24 | Undocumented  | Stub — rarely used by games |
//! | 0x25  | MultiBoot         | Stub — returns r0=1 (failure) |
//! | 0x26  | HardReset         | Stub — returns immediately |
//! | 0x27  | CustomHalt        | Stub — returns immediately |
//! | 0x28  | SoundDriverVSyncOff | Stub — no sound mixer |
//! | 0x29  | SoundDriverVSyncOn  | Stub — no sound mixer |
//! | 0x2A  | SoundGetJumpList  | Stub — no sound mixer |
//!
//! ## Stub limitations
//!
//! - **Sound driver SWIs (0x1A–0x1E, 0x28–0x2A)**: The GBA BIOS sound mixer is a
//!   complex DMA-driven PCM engine that games using the M4A/MusicPlayer2000 engine
//!   rely on. This BIOS does not implement it — games using hardware channels (most
//!   homebrew) are unaffected, but games using the BIOS mixer will have no sound.
//! - **MultiBoot (0x25)**: Serial link multiplayer boot requires hardware link cable
//!   emulation. Returns failure so games fall back gracefully.
//! - **MidiKey2Freq (0x1F)**: Uses integer arithmetic with a 12-entry 2^(n/12) LUT
//!   instead of the official BIOS floating-point implementation. Pitch accuracy is
//!   within ±1 semitone for extreme key values; typical game usage is unaffected.

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
    ///
    /// Sets the skip-intro flag at 0x03007FFC so the BIOS skips the
    /// logo/jingle intro and boots quickly.
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

        // Set skip-intro flag so the boot sequence skips the logo/jingle
        // and reaches the cartridge entry point quickly.
        gba.bus_mut().write8(0x03007FFC, 1);

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

    /// Encode `STR Rd, [Rn, #imm12]` (ARM, unconditional, pre-indexed, unsigned offset).
    const fn arm_str(rd: u32, rn: u32, imm12: u32) -> u32 {
        // STR: cond=1110, 01 I=0 P=1 U=1 B=0 W=0 L=0
        0xE580_0000 | (rn << 16) | (rd << 12) | (imm12 & 0xFFF)
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
    fn bios_checksum_returns_gba_checksum() {
        let code = &[arm_swi(0x0D), ARM_IDLE];

        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        assert_eq!(
            gba.cpu_reg(0),
            0xBAAE_187F,
            "BiosChecksum should match the original GBA/GBA SP BIOS checksum for game compatibility"
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
    fn bios_cpu_set_copies_halfwords_from_unaligned_source() {
        let src_addr: u32 = 0x0200_0101;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_mov_imm(2, 2));
        code.push(arm_swi(0x0B));
        code.push(ARM_IDLE);

        let src_data = 0xFEED_FACEu32.to_le_bytes();
        let mut gba = boot_and_setup_memory(&code, &[(src_addr & !1, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(
            gba.bus_mut().read32(dst_addr),
            0x00FE_00FA,
            "CpuSet halfword copy should preserve odd source byte lane"
        );
    }

    #[test]
    fn bios_cpu_set_fills_halfwords_from_unaligned_source() {
        let src_addr: u32 = 0x0200_0101;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        let mut r2_code = arm_load_const(2, 2 | (1 << 24));
        code.append(&mut r2_code);
        code.push(arm_swi(0x0B));
        code.push(ARM_IDLE);

        let src_data = 0xFEED_FACEu32.to_le_bytes();
        let mut gba = boot_and_setup_memory(&code, &[(src_addr & !1, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(
            gba.bus_mut().read32(dst_addr),
            0x00FA_00FA,
            "CpuSet halfword fill should preserve odd source byte lane"
        );
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

    // ---------------------------------------------------------------
    // SWI 0x11: LZ77UnCompWram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_lz77_wram_all_literals() {
        // LZ77 with only literal bytes, no back-references
        // Output: [0x41, 0x42, 0x43, 0x44]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x11));
        code.push(ARM_IDLE);

        // Header: type=1 (LZ77), decompressed_size=4
        let header: u32 = (4 << 8) | (1 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Flag byte: 0x00 (all 8 blocks are literal)
        src_data.push(0x00);
        // 4 literal bytes (only 4 of 8 blocks used, stops when size reached)
        src_data.extend_from_slice(&[0x41, 0x42, 0x43, 0x44]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [0x41u8, 0x42, 0x43, 0x44];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "LZ77 literal byte {i}"
            );
        }
    }

    #[test]
    fn bios_lz77_wram_back_reference() {
        // LZ77: "ABCABC" — 3 literals then back-ref copying 3 from displacement 2
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x11));
        code.push(ARM_IDLE);

        // Header: type=1, size=6
        let header: u32 = (6 << 8) | (1 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Flag: bit7..bit4 = blocks 0-3; block3 is compressed
        // bit7=0(lit), bit6=0(lit), bit5=0(lit), bit4=1(comp) → 0x10
        src_data.push(0x10);
        // 3 literal bytes
        src_data.extend_from_slice(&[0x41, 0x42, 0x43]);
        // Compressed block: count=3 (nibble=0), displacement=2
        // byte1 = ((3-3)<<4) | (2>>8) = 0x00
        // byte2 = 2 & 0xFF = 0x02
        src_data.push(0x00);
        src_data.push(0x02);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [0x41, 0x42, 0x43, 0x41, 0x42, 0x43];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "LZ77 back-ref byte {i}"
            );
        }
    }

    #[test]
    fn bios_lz77_wram_repeated_pattern() {
        // LZ77: "ABABABABAB" — 2 literals + back-ref for 8 more bytes
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x11));
        code.push(ARM_IDLE);

        // Header: type=1, size=10
        let header: u32 = (10 << 8) | (1 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Flag: bit7=0(lit A), bit6=0(lit B), bit5=1(comp) → 0x20
        src_data.push(0x20);
        // 2 literal bytes
        src_data.extend_from_slice(&[0x41, 0x42]);
        // Compressed: count=8 (nibble=5), displacement=1
        src_data.push(0x50); // ((8-3)<<4) | (1>>8)
        src_data.push(0x01); // 1 & 0xFF

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [0x41, 0x42, 0x41, 0x42, 0x41, 0x42, 0x41, 0x42, 0x41, 0x42];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "LZ77 repeat byte {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // SWI 0x12: LZ77UnCompVram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_lz77_vram_basic() {
        // Same algorithm as WRAM but buffered 16-bit writes
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x12));
        code.push(ARM_IDLE);

        // Header: type=1, size=4
        let header: u32 = (4 << 8) | (1 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.push(0x00); // all literal
        src_data.extend_from_slice(&[0x41, 0x42, 0x43, 0x44]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        // Output as halfwords: (0x42<<8)|0x41, (0x44<<8)|0x43
        assert_eq!(gba.bus_mut().read16(dst_addr), 0x4241, "LZ77 VRAM hw 0");
        assert_eq!(gba.bus_mut().read16(dst_addr + 2), 0x4443, "LZ77 VRAM hw 1");
    }

    // ---------------------------------------------------------------
    // SWI 0x13: HuffUnComp tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_huffman_8bit_two_symbols() {
        // Huffman 8-bit with minimal tree: root → left=A(0x41), right=B(0x42)
        // Output: "AABB" (4 bytes)
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x13));
        code.push(ARM_IDLE);

        // Header: data_size=8 bits, type=2, decompressed_size=4 bytes
        let header: u32 = (4 << 8) | (2 << 4) | 8;
        let mut src_data = header.to_le_bytes().to_vec();
        // Tree size: (3 bytes / 2) - 1 = 0... but mGBA uses (byte<<1)+1
        // tree_size_byte=1 → tree = 1*2+1 = 3 bytes
        src_data.push(1);
        // Root node: both children are leaves, offset=0
        // bit7=1 (left=data), bit6=1 (right=data), offset=0
        src_data.push(0xC0);
        // Left leaf (go-left=0): A = 0x41
        src_data.push(0x41);
        // Right leaf (go-right=1): B = 0x42
        src_data.push(0x42);
        // Bitstream: AABB = bits 0,0,1,1 (bit31=first)
        // bit31=0(A), bit30=0(A), bit29=1(B), bit28=1(B) → 0x30000000
        src_data.extend_from_slice(&0x30000000u32.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read8(dst_addr), 0x41, "Huffman 8bit byte 0");
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 1),
            0x41,
            "Huffman 8bit byte 1"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 2),
            0x42,
            "Huffman 8bit byte 2"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 3),
            0x42,
            "Huffman 8bit byte 3"
        );
    }

    #[test]
    fn bios_huffman_8bit_deeper_tree() {
        // Huffman 8-bit with 3-level tree:
        //       root
        //      /    \
        //     A    node1
        //          /   \
        //         B     C
        // A=0 (1 bit), B=10 (2 bits), C=11 (2 bits)
        // Output: "ABCB" (4 bytes)
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x13));
        code.push(ARM_IDLE);

        // Header: data_size=8, type=2, size=4
        let header: u32 = (4 << 8) | (2 << 4) | 8;
        let mut src_data = header.to_le_bytes().to_vec();
        // tree_size_byte=2 → tree = 2*2+1 = 5 bytes
        src_data.push(2);
        // Offset 5: root = bit7=1(left=data), bit6=0(right=not data), offset=0
        src_data.push(0x80);
        // Offset 6: leaf A = 0x41
        src_data.push(0x41);
        // Offset 7: node1 = bit7=1(left=data), bit6=1(right=data), offset=0
        src_data.push(0xC0);
        // Offset 8: leaf B = 0x42
        src_data.push(0x42);
        // Offset 9: leaf C = 0x43
        src_data.push(0x43);
        // Padding to align bitstream to 4 bytes (offset 10,11)
        src_data.push(0x00);
        src_data.push(0x00);
        // Bitstream at offset 12: ABCB = 0, 10, 11, 10
        // bit31=0(A), bit30=1 bit29=0(B), bit28=1 bit27=1(C), bit26=1 bit25=0(B)
        // = 0b01011100_00000000_00000000_00000000 = 0x5C000000
        src_data.extend_from_slice(&0x5C000000u32.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read8(dst_addr), 0x41, "Huffman deep byte 0");
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 1),
            0x42,
            "Huffman deep byte 1"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 2),
            0x43,
            "Huffman deep byte 2"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 3),
            0x42,
            "Huffman deep byte 3"
        );
    }

    #[test]
    fn bios_huffman_4bit() {
        // Huffman 4-bit: tree with 2 symbols (0x3, 0x7)
        // Output nibbles: [3,7,3,7,3,7,3,7] packed as bytes [0x73,0x73,0x73,0x73]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x13));
        code.push(ARM_IDLE);

        // Header: data_size=4, type=2, size=4 bytes
        let header: u32 = (4 << 8) | (2 << 4) | 4;
        let mut src_data = header.to_le_bytes().to_vec();
        // tree_size_byte=1 → tree = 3 bytes
        src_data.push(1);
        // Root: both children are leaves, offset=0
        src_data.push(0xC0);
        // Left=3, Right=7
        src_data.push(0x03);
        src_data.push(0x07);
        // Bitstream: 0,1,0,1,0,1,0,1
        // bit31=0, bit30=1, bit29=0, bit28=1, bit27=0, bit26=1, bit25=0, bit24=1
        // = 0x55000000
        src_data.extend_from_slice(&0x55000000u32.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        // Nibbles packed LSB-first: (7<<4)|3 = 0x73
        assert_eq!(gba.bus_mut().read8(dst_addr), 0x73, "Huffman 4bit byte 0");
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 1),
            0x73,
            "Huffman 4bit byte 1"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 2),
            0x73,
            "Huffman 4bit byte 2"
        );
        assert_eq!(
            gba.bus_mut().read8(dst_addr + 3),
            0x73,
            "Huffman 4bit byte 3"
        );
    }

    // ---------------------------------------------------------------
    // SWI 0x14: RLUnCompWram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_rle_wram_mixed() {
        // RLE: compressed run (A×4) + uncompressed literals (B, C)
        // Output: [0x41, 0x41, 0x41, 0x41, 0x42, 0x43]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x14));
        code.push(ARM_IDLE);

        // Header: type=3 (RLE), size=6
        let header: u32 = (6 << 8) | (3 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Compressed run: flag=0x81 (bit7=1, N=1 → repeat N+3=4 times), data=0x41
        src_data.push(0x81);
        src_data.push(0x41);
        // Uncompressed: flag=0x01 (bit7=0, N=1 → copy N+1=2 bytes)
        src_data.push(0x01);
        src_data.extend_from_slice(&[0x42, 0x43]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [0x41, 0x41, 0x41, 0x41, 0x42, 0x43];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "RLE mixed byte {i}"
            );
        }
    }

    #[test]
    fn bios_rle_wram_all_compressed() {
        // RLE: single compressed run X×8
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x14));
        code.push(ARM_IDLE);

        // Header: type=3, size=8
        let header: u32 = (8 << 8) | (3 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Compressed: flag=0x85 (N=5 → repeat N+3=8), data=0x58
        src_data.push(0x85);
        src_data.push(0x58);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        for i in 0u32..8 {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i),
                0x58,
                "RLE all-compressed byte {i}"
            );
        }
    }

    #[test]
    fn bios_rle_wram_all_literal() {
        // RLE: all uncompressed literal bytes
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x14));
        code.push(ARM_IDLE);

        // Header: type=3, size=4
        let header: u32 = (4 << 8) | (3 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Uncompressed: flag=0x03 (N=3 → copy N+1=4 bytes)
        src_data.push(0x03);
        src_data.extend_from_slice(&[0x41, 0x42, 0x43, 0x44]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [0x41, 0x42, 0x43, 0x44];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "RLE literal byte {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // SWI 0x15: RLUnCompVram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_rle_vram_basic() {
        // RLE VRAM: same algorithm but 16-bit writes
        // Compressed run A×4 → halfwords 0x4141, 0x4141
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x15));
        code.push(ARM_IDLE);

        // Header: type=3, size=4
        let header: u32 = (4 << 8) | (3 << 4);
        let mut src_data = header.to_le_bytes().to_vec();
        // Compressed: A×4
        src_data.push(0x81);
        src_data.push(0x41);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read16(dst_addr), 0x4141, "RLE VRAM hw 0");
        assert_eq!(gba.bus_mut().read16(dst_addr + 2), 0x4141, "RLE VRAM hw 1");
    }

    // ---------------------------------------------------------------
    // SWI 0x16: Diff8bitUnFilterWram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_diff8_unfilter_wram_ascending() {
        // Cumulative 8-bit deltas: [10, +5, +3, +2] → [10, 15, 18, 20]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x16));
        code.push(ARM_IDLE);

        // Header: type=8 (DiffFilter), unit_size=1 (8-bit), size=4
        let header: u32 = (4 << 8) | (8 << 4) | 1;
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.extend_from_slice(&[10, 5, 3, 2]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [10u8, 15, 18, 20];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "Diff8 WRAM byte {i}"
            );
        }
    }

    #[test]
    fn bios_diff8_unfilter_wram_negative_deltas() {
        // Deltas with negative values: [100, -10, +20, -30] → [100, 90, 110, 80]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x16));
        code.push(ARM_IDLE);

        let header: u32 = (4 << 8) | (8 << 4) | 1;
        let mut src_data = header.to_le_bytes().to_vec();
        // -10 as u8 = 246, -30 as u8 = 226
        src_data.extend_from_slice(&[100, 246, 20, 226]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        let expected = [100u8, 90, 110, 80];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                gba.bus_mut().read8(dst_addr + i as u32),
                exp,
                "Diff8 neg delta byte {i}"
            );
        }
    }

    #[test]
    fn bios_diff8_unfilter_wram_wrapping() {
        // Test wrapping: [200, +100] → [200, 44] (200+100=300, wraps to 44)
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x16));
        code.push(ARM_IDLE);

        let header: u32 = (2 << 8) | (8 << 4) | 1;
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.extend_from_slice(&[200, 100]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read8(dst_addr), 200, "Diff8 wrap byte 0");
        assert_eq!(gba.bus_mut().read8(dst_addr + 1), 44, "Diff8 wrap byte 1");
    }

    // ---------------------------------------------------------------
    // SWI 0x17: Diff8bitUnFilterVram tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_diff8_unfilter_vram() {
        // Same 8-bit deltas but output written as 16-bit halfwords
        // Deltas: [10, +5, +3, +2] → values [10, 15, 18, 20]
        // Packed as hw: (15<<8)|10 = 0x0F0A, (20<<8)|18 = 0x1412
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x17));
        code.push(ARM_IDLE);

        let header: u32 = (4 << 8) | (8 << 4) | 1;
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.extend_from_slice(&[10, 5, 3, 2]);

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read16(dst_addr), 0x0F0A, "Diff8 VRAM hw 0");
        assert_eq!(
            gba.bus_mut().read16(dst_addr + 2),
            0x1412,
            "Diff8 VRAM hw 1"
        );
    }

    // ---------------------------------------------------------------
    // SWI 0x18: Diff16bitUnFilter tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_diff16_unfilter_ascending() {
        // Cumulative 16-bit deltas: [100, +50, +30] → [100, 150, 180]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x18));
        code.push(ARM_IDLE);

        // Header: type=8, unit_size=2 (16-bit), size=6 bytes (3 halfwords)
        let header: u32 = (6 << 8) | (8 << 4) | 2;
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.extend_from_slice(&100u16.to_le_bytes());
        src_data.extend_from_slice(&50u16.to_le_bytes());
        src_data.extend_from_slice(&30u16.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read16(dst_addr), 100, "Diff16 hw 0");
        assert_eq!(gba.bus_mut().read16(dst_addr + 2), 150, "Diff16 hw 1");
        assert_eq!(gba.bus_mut().read16(dst_addr + 4), 180, "Diff16 hw 2");
    }

    #[test]
    fn bios_diff16_unfilter_negative_deltas() {
        // Deltas: [1000, -200, +500] → [1000, 800, 1300]
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;

        let mut code = arm_load_const(0, src_addr);
        code.extend(arm_load_const(1, dst_addr));
        code.push(arm_swi(0x18));
        code.push(ARM_IDLE);

        let header: u32 = (6 << 8) | (8 << 4) | 2;
        let mut src_data = header.to_le_bytes().to_vec();
        src_data.extend_from_slice(&1000u16.to_le_bytes());
        src_data.extend_from_slice(&(-200i16 as u16).to_le_bytes());
        src_data.extend_from_slice(&500u16.to_le_bytes());

        let mut gba = boot_and_setup_memory(&code, &[(src_addr, &src_data)]);
        run_until_idle(&mut gba, 500_000);

        assert_eq!(gba.bus_mut().read16(dst_addr), 1000, "Diff16 neg hw 0");
        assert_eq!(gba.bus_mut().read16(dst_addr + 2), 800, "Diff16 neg hw 1");
        assert_eq!(gba.bus_mut().read16(dst_addr + 4), 1300, "Diff16 neg hw 2");
    }

    // ---------------------------------------------------------------
    // SWI 0x19: SoundBias tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_sound_bias_set_high() {
        // SoundBias with r0 != 0 should set SOUNDBIAS toward 0x200
        let src_addr: u32 = 0x0200_0100;
        let dst_addr: u32 = 0x0200_0200;
        let _ = (src_addr, dst_addr);

        let mut code = vec![arm_mov_imm(0, 1)]; // r0 = 1 (nonzero → target 0x200)
        code.push(arm_swi(0x19));
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 2_000_000);

        // SOUNDBIAS at 0x04000088 should be 0x200
        let bias = gba.bus_mut().read16(0x0400_0088);
        assert_eq!(bias & 0x3FF, 0x200, "SoundBias should set bias to 0x200");
    }

    #[test]
    fn bios_sound_bias_set_low() {
        // SoundBias with r0 = 0 should set SOUNDBIAS toward 0x000
        let mut code = vec![arm_mov_imm(0, 0)]; // r0 = 0 → target 0x000
        code.push(arm_swi(0x19));
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 2_000_000);

        // SOUNDBIAS at 0x04000088 should be 0x000
        let bias = gba.bus_mut().read16(0x0400_0088);
        assert_eq!(bias & 0x3FF, 0x000, "SoundBias should set bias to 0x000");
    }

    // ---------------------------------------------------------------
    // SWI 0x1F: MidiKey2Freq tests
    // ---------------------------------------------------------------

    #[test]
    fn bios_midi_key2freq_a4() {
        // MidiKey2Freq(wa, mk=69, fp=0) where wa->freq = 7040 Hz
        // Key 69 = A4 standard pitch
        // freq = 7040 / 2^((180-69-0/256)/12) = 7040 / 2^(111/12)
        //      = 7040 / 2^9.25 = 7040 / 608.87... ≈ 11.56...
        // But this is in fixed-point format: result = freq * 2^(-((180-mk)/12))
        // For mk=69: semitones_below = 180 - 69 = 111 = 9 octaves + 3 semitones
        // 2^(111/12) = 2^9 * 2^(3/12) = 512 * 1.1892... ≈ 608.87
        // result ≈ 7040 / 608.87 ≈ 11.56 Hz → as fixed point with result << 0
        //
        // The real formula returns a fixed-point value suitable for timer reload.
        // We just need to verify the SWI returns a plausible nonzero value in r0.
        let wa_addr: u32 = 0x0200_0100;

        // WaveData struct: first 4 bytes = type (unused by MidiKey2Freq)
        //                  next 4 bytes = status (unused)
        //                  next 4 bytes = freq (base frequency in Hz)
        //                  next 4 bytes = loop position
        //                  next 4 bytes = number of samples
        let mut wa_data: Vec<u8> = Vec::new();
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // type
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // status
        wa_data.extend_from_slice(&7040u32.to_le_bytes()); // freq
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // loop
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // nsamples

        let mut code = arm_load_const(0, wa_addr); // r0 = wa pointer
        code.extend(arm_load_const(1, 69)); // r1 = mk (A4)
        code.extend(arm_load_const(2, 0)); // r2 = fp (0)
        code.push(arm_swi(0x1F));
        // Store r0 result to memory so we can read it
        let result_addr: u32 = 0x0200_0300;
        code.extend(arm_load_const(4, result_addr));
        code.push(arm_str(0, 4, 0)); // str r0, [r4]
        code.push(ARM_IDLE);

        let mut gba = boot_and_setup_memory(&code, &[(wa_addr, &wa_data)]);
        run_until_idle(&mut gba, 500_000);

        let result = gba.bus_mut().read32(result_addr);
        // For mk=69, fp=0, freq=7040:
        // semitone_offset = 180 - 69 = 111 semitones = 9 octaves + 3 semitones
        // Result = 7040 / 2^(111/12) ≈ 7040 / 608.87 ≈ 11.56
        // As an integer: should be small but nonzero, and NOT equal to the input wa_addr
        assert!(
            result > 0 && result < 1000,
            "MidiKey2Freq should return a small positive value for high key offset, got {result}"
        );
    }

    #[test]
    fn bios_midi_key2freq_key_180() {
        // MidiKey2Freq(wa, mk=180, fp=0): exponent = 0, so result = wa->freq
        let wa_addr: u32 = 0x0200_0100;

        let mut wa_data: Vec<u8> = Vec::new();
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // type
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // status
        wa_data.extend_from_slice(&8000u32.to_le_bytes()); // freq
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // loop
        wa_data.extend_from_slice(&0u32.to_le_bytes()); // nsamples

        let mut code = arm_load_const(0, wa_addr);
        code.extend(arm_load_const(1, 180)); // mk = 180 → no shift
        code.extend(arm_load_const(2, 0)); // fp = 0
        code.push(arm_swi(0x1F));
        let result_addr: u32 = 0x0200_0300;
        code.extend(arm_load_const(4, result_addr));
        code.push(arm_str(0, 4, 0));
        code.push(ARM_IDLE);

        let mut gba = boot_and_setup_memory(&code, &[(wa_addr, &wa_data)]);
        run_until_idle(&mut gba, 500_000);

        let result = gba.bus_mut().read32(result_addr);
        // At mk=180, fp=0: no shifting needed, result should equal freq exactly
        assert_eq!(
            result, 8000,
            "MidiKey2Freq at key 180 should return base freq"
        );
    }

    // ---------------------------------------------------------------
    // SWI 0x25: MultiBoot test
    // ---------------------------------------------------------------

    #[test]
    fn bios_multiboot_returns_failure() {
        // MultiBoot stub should set r0 = 1 (failure)
        let result_addr: u32 = 0x0200_0300;

        let mut code = arm_load_const(0, 0x0200_0100); // r0 = param (unused)
        code.extend(arm_load_const(1, 0)); // r1 = mode
        code.push(arm_swi(0x25));
        code.extend(arm_load_const(4, result_addr));
        code.push(arm_str(0, 4, 0)); // store r0 result
        code.push(ARM_IDLE);

        let mut gba = boot_with_embedded_bios(&code);
        run_until_idle(&mut gba, 500_000);

        let result = gba.bus_mut().read32(result_addr);
        assert_eq!(result, 1, "MultiBoot stub should return r0=1 (failure)");
    }

    // ---------------------------------------------------------------
    // Sound driver stubs: verify they don't crash
    // ---------------------------------------------------------------

    #[test]
    fn bios_sound_driver_stubs_return() {
        // SWIs 0x1A-0x1E should just return without crashing
        for swi_num in [0x1A, 0x1B, 0x1C, 0x1D, 0x1E] {
            let mut code = vec![arm_mov_imm(0, 0)];
            code.push(arm_swi(swi_num));
            code.push(ARM_IDLE);

            let mut gba = boot_with_embedded_bios(&code);
            run_until_idle(&mut gba, 500_000);

            // If we reach here without hanging/crashing, the stub works
            let pc = gba.cpu_reg(15);
            assert!(
                pc >= 0x0800_0000,
                "SWI 0x{swi_num:02X} stub should return to ROM"
            );
        }
    }

    #[test]
    fn bios_sound_vsync_off_on_stubs_return() {
        // SWIs 0x28-0x2A should just return without crashing
        for swi_num in [0x28, 0x29, 0x2A] {
            let mut code = vec![arm_mov_imm(0, 0)];
            code.push(arm_swi(swi_num));
            code.push(ARM_IDLE);

            let mut gba = boot_with_embedded_bios(&code);
            run_until_idle(&mut gba, 500_000);

            let pc = gba.cpu_reg(15);
            assert!(
                pc >= 0x0800_0000,
                "SWI 0x{swi_num:02X} stub should return to ROM"
            );
        }
    }

    // ---------------------------------------------------------------
    // Boot sequence tests (Phase 1: Config + Skip Infrastructure)
    // ---------------------------------------------------------------

    #[test]
    fn bios_boot_writes_undocumented_0x04000410() {
        // The real GBA BIOS writes 0xFF to the undocumented register at
        // 0x04000410 during boot. Our BIOS should do the same.
        let code = &[ARM_IDLE];
        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        let val = gba.bus_mut().read8(0x04000410);
        assert_eq!(
            val, 0xFF,
            "BIOS should write 0xFF to undocumented 0x04000410"
        );
    }

    #[test]
    fn bios_boot_ramps_soundbias_to_0x200() {
        // The real GBA BIOS ramps SOUNDBIAS from 0x000 to 0x200 during boot
        // using SWI 0x19. After boot, SOUNDBIAS should be 0x200.
        let code = &[ARM_IDLE];
        let mut gba = boot_with_embedded_bios(code);
        run_until_idle(&mut gba, 100_000);

        let soundbias = gba.bus_mut().read16(0x04000088);
        assert_eq!(
            soundbias & 0x3FF,
            0x200,
            "SOUNDBIAS should be 0x200 after boot"
        );
    }

    #[test]
    fn bios_boot_with_invalid_header_locks_up() {
        // When the cartridge header has an invalid complement check,
        // the BIOS should lock up (never reach 0x08000000).
        let mut rom = make_test_rom(&[ARM_IDLE]);
        // Corrupt the complement check byte
        rom[COMPLEMENT_CHECK_OFFSET] = rom[COMPLEMENT_CHECK_OFFSET].wrapping_add(1);

        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));
        gba.load_rom(&rom, "bad-header.gba")
            .expect("ROM should still load");

        // Run for a while — BIOS should NOT reach cartridge entry point
        let mut cycles = 0u64;
        while cycles < 50_000 {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
        }
        assert!(
            gba.cpu_pc() < 0x0800_0000,
            "BIOS should lock up with invalid header, but PC reached {:#010X}",
            gba.cpu_pc()
        );
    }

    #[test]
    fn bios_boot_with_invalid_fixed_byte_locks_up() {
        // When the fixed byte at 0xB2 is not 0x96, BIOS should lock up.
        let mut rom = make_test_rom(&[ARM_IDLE]);
        rom[FIXED_BYTE_OFFSET] = 0x00; // Should be 0x96
        // Recompute complement check with the bad fixed byte
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);

        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));
        gba.load_rom(&rom, "bad-fixed-byte.gba")
            .expect("ROM should still load");

        let mut cycles = 0u64;
        while cycles < 50_000 {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
        }
        assert!(
            gba.cpu_pc() < 0x0800_0000,
            "BIOS should lock up with invalid fixed byte, but PC reached {:#010X}",
            gba.cpu_pc()
        );
    }

    #[test]
    fn bios_warm_boot_redirects_to_debug_vector() {
        // When POSTFLG is already 1 on reset, the BIOS should branch to
        // the debug handler vector at 0x0000001C instead of doing full boot.
        let code = &[ARM_IDLE];
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(code);
        gba.load_rom(&rom, "warm-boot.gba")
            .expect("ROM should load");

        // Set POSTFLG to 1 before boot starts (simulating warm boot)
        gba.bus_mut().write8(0x04000300, 1);

        // Run a few cycles — should end up at the FIQ/debug vector handler
        let mut cycles = 0u64;
        while cycles < 1_000 {
            let pc = gba.cpu_pc();
            // If we reach cartridge, something went wrong (should redirect)
            if pc >= 0x0800_0000 {
                break;
            }
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
        }

        // Should NOT have reached cartridge — should be stuck at debug vector
        // (which is currently a trap/infinite loop at 0x1C)
        let pc = gba.cpu_pc();
        assert!(
            pc < 0x0800_0000,
            "Warm boot should redirect to debug vector, not cartridge. PC={pc:#010X}"
        );
    }

    // ---------------------------------------------------------------
    // Boot sequence tests (Phase 3-5: Full intro with logo + jingle)
    // ---------------------------------------------------------------

    /// Boot with the full intro enabled (no skip flag).
    /// Runs until PC reaches 0x08000000 or cycle limit hit.
    /// The cycle limit is high enough for ~5 seconds of GBA time.
    fn boot_with_full_intro(arm_code: &[u32]) -> Option<Gba> {
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(arm_code);
        gba.load_rom(&rom, "bios-full-boot.gba")
            .expect("test ROM should load");

        // Do NOT set skip flag — let the full boot sequence run.
        // Run for up to ~5 sec of GBA time (16.78MHz × 5 = ~84M cycles).
        let max_cycles: u64 = 84_000_000;
        let mut cycles = 0u64;
        while cycles < max_cycles {
            let pc = gba.cpu_pc();
            if pc >= 0x0800_0000 {
                return Some(gba);
            }
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                return None;
            }
            cycles += tick;
        }
        None
    }

    #[test]
    fn bios_full_boot_reaches_cartridge() {
        // The full boot (with intro) should eventually reach the cartridge
        // entry point at 0x08000000 after displaying the logo and playing
        // the jingle.
        let gba = boot_with_full_intro(&[ARM_IDLE]);
        assert!(
            gba.is_some(),
            "Full boot with intro should eventually reach cartridge"
        );
    }

    #[test]
    fn bios_full_boot_enables_apu() {
        // After the full boot, the APU should be enabled (SOUNDCNT_X bit 7).
        let mut gba = boot_with_full_intro(&[ARM_IDLE]).expect("Full boot should reach cartridge");
        run_until_idle(&mut gba, 100_000);

        let soundcnt_x = gba.bus_mut().read16(0x04000084);
        assert_eq!(
            soundcnt_x & 0x80,
            0x80,
            "APU should be enabled after full boot (SOUNDCNT_X bit 7)"
        );
    }

    #[test]
    fn bios_full_boot_sets_sound_routing() {
        // After the full boot, SOUNDCNT_L should have channel routing set
        // (the jingle configures CH1 output to both speakers).
        let mut gba = boot_with_full_intro(&[ARM_IDLE]).expect("Full boot should reach cartridge");
        run_until_idle(&mut gba, 100_000);

        let soundcnt_l = gba.bus_mut().read16(0x04000080);
        // Volume should be max (0x77 in low byte) and CH1 routed to both
        // speakers (bits in high byte).
        assert_ne!(
            soundcnt_l, 0,
            "SOUNDCNT_L should have routing configured after boot jingle"
        );
    }

    #[test]
    fn bios_skip_flag_bypasses_intro() {
        // When the skip flag is set at 0x03007FFC, the BIOS should skip
        // the intro and boot quickly (within 10K cycles as before).
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(&[ARM_IDLE]);
        gba.load_rom(&rom, "skip-test.gba")
            .expect("ROM should load");

        // Set skip flag before boot
        gba.bus_mut().write8(0x03007FFC, 1);

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
            "Skip flag should bypass intro. PC={:#010X}",
            gba.cpu_pc()
        );
    }

    #[test]
    fn bios_logo_vram_no_byte_write_bleed() {
        // GBA VRAM ignores byte writes (STRB) and instead duplicates the byte
        // into both bytes of the addressed halfword. The logo drawing must use
        // halfword read-modify-write to avoid pixel bleed.
        //
        // The first RLE span is: offset=16630, count=3 (row 69, cols 70-72).
        // With correct writes, VRAM[16633] (col 73) should be 0 (background).
        // With buggy STRB, it would be 1 (bleed from the halfword duplication).
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(&[ARM_IDLE]);
        gba.load_rom(&rom, "vram-bleed-test.gba")
            .expect("ROM should load");

        // Run for ~1 second — logo is drawn early, well before fade starts.
        let target_cycles: u64 = 16_780_000;
        let mut cycles = 0u64;
        while cycles < target_cycles {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
        }

        // Check the first span: 3 pixels at VRAM offset 16630 (cols 70-72).
        // Pixels 70, 71, 72 should be 1 (palette entry 1).
        assert_eq!(
            gba.bus().debug_read_vram(16630),
            1,
            "Pixel at span start (col 70) should be palette entry 1"
        );
        assert_eq!(
            gba.bus().debug_read_vram(16632),
            1,
            "Pixel at span end (col 72) should be palette entry 1"
        );
        // The pixel AFTER the span must be 0 (background).
        assert_eq!(
            gba.bus().debug_read_vram(16633),
            0,
            "Pixel after span end (col 73) should be 0 — STRB bleed bug if not"
        );
    }

    #[test]
    fn bios_full_boot_produces_visible_framebuffer() {
        // During the full boot, the BIOS sets up Mode 4 display and draws
        // the NESER logo. Verify the framebuffer contains non-black pixels.
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(&[ARM_IDLE]);
        gba.load_rom(&rom, "display-test.gba")
            .expect("ROM should load");

        // Run for ~2 seconds of GBA time (enough for logo to be drawn and
        // at least one frame to be rendered, but before the fade finishes).
        let target_cycles: u64 = 33_000_000;
        let mut cycles = 0u64;
        let mut any_non_black_frame = false;
        while cycles < target_cycles {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
            if gba.is_ready_to_render() {
                let fb = gba.screen_snapshot();
                let has_non_black = fb
                    .chunks_exact(3)
                    .any(|px| px[0] != 0 || px[1] != 0 || px[2] != 0);
                if has_non_black {
                    any_non_black_frame = true;
                    break;
                }
                gba.clear_ready_to_render();
            }
        }
        assert!(
            any_non_black_frame,
            "Boot intro should produce visible (non-black) pixels in the framebuffer"
        );
    }

    #[test]
    fn bios_full_boot_produces_audio_samples() {
        // During the full boot, the BIOS enables the APU and triggers
        // CH1 notes. Verify that non-zero audio samples are produced.
        let mut config = Config::default();
        let tmp = std::env::temp_dir().join("neser_bios_test_nonexistent");
        config.gba.bios_path = Some(tmp.to_string_lossy().into_owned());
        let mut gba = Gba::new(AppContext::new_with_config(config));

        let rom = make_test_rom(&[ARM_IDLE]);
        gba.load_rom(&rom, "audio-test.gba")
            .expect("ROM should load");

        // Run for ~3 seconds of GBA time (jingle starts at frame 40 ≈ 670ms).
        let target_cycles: u64 = 50_000_000;
        let mut cycles = 0u64;
        let mut any_non_zero_sample = false;
        while cycles < target_cycles {
            let tick = gba.run_tick_for_tests() as u64;
            if tick == 0 {
                break;
            }
            cycles += tick;
            if gba.sample_ready()
                && let Some(sample) = gba.get_sample()
                && sample.abs() > 0.001
            {
                any_non_zero_sample = true;
                break;
            }
        }
        assert!(
            any_non_zero_sample,
            "Boot jingle should produce non-zero audio samples"
        );
    }
}
