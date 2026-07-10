//! Black-box coverage for issue #2962: the two vendored absindx SA-1 conformance ROMs
//! (`roms/snes/automated_tests/snes_test_roms/absindx/`), driven to completion using each ROM's
//! own pass/fail marker protocol rather than a Mesen2-cross-checked screen CRC -- both ROMs are
//! documented (and separately confirmed) to behave unreliably on Mesen2 itself.
//!
//! The result byte (`TestFinished`: `0`=Running, `1`=Passed, `255`=Failed) lives in **WRAM**
//! `$7E0000`, not SA-1 I-RAM: the ROMs' `org $0000` variable block is accessed by the SNES CPU
//! via direct-page addressing with `D=$0000`, which on the SNES bus resolves to WRAM. (The SA-1
//! CPU's identically-numbered address space puts I-RAM there instead, which is why I-RAM offset
//! `$0000` -- SNES-visible at `$003000` -- is a tempting but WRONG address to poll: it holds a
//! stale `$AA` left over from the I-RAM mirroring sub-tests.)
//!
//! Completion is detected via the SA-1 CPU's own PC reaching its documented
//! `SA1TestFinished_InfLoop` address (from each ROM's `.sym` file) combined with the byte
//! becoming nonzero -- see [`RunOracle::Sa1IdlePc`]'s doc comment for why neither signal alone
//! is sufficient.
//!
//! `SA1RamProtectionTest` additionally exposes `DisplayResult`/`DisplayTestID` (the first failed
//! sub-test's ID, per its `MessageID.asm`) at WRAM `$7E0030`/`$7E0031` for diagnostics;
//! `SA1VersionCodeTest` (whose source isn't vendored, only its prebuilt `.sfc`/`.sym`) exposes
//! `TestVersionVC`/`TestVersionVCTrue` at `$7E0001`/`$7E0002` instead.

use crate::snes::integration_tests::rom_runner::{RunConfig, RunOracle, run_rom_with_oracle};
use std::path::Path;

const ROOT: &str = "roms/snes/automated_tests/snes_test_roms/absindx";
const TEST_FINISHED_ADDR: u32 = 0x7E_0000;
const PASSED: u8 = 1;

fn load_rom(file: &str) -> Vec<u8> {
    let path = Path::new(ROOT).join(file);
    std::fs::read(&path).unwrap_or_else(|err| panic!("failed to read ROM {file}: {err}"))
}

#[test]
fn sa1_ram_protection_test_passes() {
    let rom = load_rom("SA1RamProtectionTest.sfc");
    let result = run_rom_with_oracle(
        &rom,
        "SA1RamProtectionTest.sfc",
        "sa1_absindx_tests",
        RunConfig::new(400_000_000, 0).with_debug_addrs(&[
            TEST_FINISHED_ADDR,
            0x7E_0030,
            0x7E_0031,
        ]),
        RunOracle::Sa1IdlePc {
            idle_pc: 0xA12D, // SA1TestFinished_InfLoop, per SA1RamProtectionTest.sym
            addr: TEST_FINISHED_ADDR,
            pass_value: PASSED,
        },
    );

    assert!(
        result.passed,
        "SA1RamProtectionTest failed: TestFinished=${:02X} DisplayResult=${:02X} \
         DisplayTestID=${:02X} (ticks={} frames={} pc=${:04X})",
        result.debug_bytes[0],
        result.debug_bytes[1],
        result.debug_bytes[2],
        result.ticks,
        result.frames,
        result.pc
    );
}

/// Unlike `SA1RamProtectionTest`, this ROM's `TestFinished` **never** becomes `1` -- not even on
/// real hardware. Disassembly of `CheckResult` (`$9E9F`, release build): it copies the SNES's
/// `$230E` read from the dump area into `TestVersionVC`/`TestVersionVCTrue`, then
/// *unconditionally* branches to `CheckResult_Failed` (`DEC TestFinished` -> `$FF`); the pass
/// path (`INC TestFinished` at `$9EAB`) is dead code with no reference anywhere in the ROM
/// (verified by scanning for absolute and relative references). That is deliberate: the SA-1's
/// true version-code value is unknown (fullsnes: "Existing value(s) are unknown"; bsnes: `$230E`
/// "does not actually exist on real hardware ... always returns open bus"), so the ROM only
/// *displays* the register dump for a human and has nothing to compare against.
///
/// What this test therefore asserts is real-hardware observable behavior:
/// - the ROM runs to completion through the full dual-CPU IRQ handshake (SA-1 parks in its
///   `InfLoop`, the SNES's `TestFinished` handler runs) with the terminal `$FF`;
/// - `TestVersionVC`/`TestVersionVCTrue` record `$23` -- the SNES-side *open-bus* read of the
///   nonexistent `$230E`, whose value is the `$23` operand byte of the ROM's own
///   `LDA $2300,X` (the last byte on the bus before the data fetch);
/// - the SNES-side dump entries prove open-bus detection worked: each register is read twice,
///   once via `LDA $2300,X` (residual bus byte `$23`) and once via `LDA $AA,X` after
///   `REP`-adjusting X so `$AA + X` lands on the same register (residual bus byte `$AA`) -- a
///   real register returns the same value twice ($2300 SFR = `00 00`), open bus returns
///   `23 AA` ($230E VC).
#[test]
fn sa1_version_code_test_completes_with_hardware_accurate_open_bus_observations() {
    let rom = load_rom("SA1VersionCodeTest.sfc");
    let result = run_rom_with_oracle(
        &rom,
        "SA1VersionCodeTest.sfc",
        "sa1_absindx_tests",
        RunConfig::new(400_000_000, 0).with_debug_addrs(&[
            TEST_FINISHED_ADDR,
            0x7E_0001, // TestVersionVC
            0x7E_0002, // TestVersionVCTrue
            0x00_3000, // I-RAM dump: SNES SFR read 1 (real register)
            0x00_3001, // I-RAM dump: SNES SFR read 2
            0x00_301C, // I-RAM dump: SNES VC read 1 (open bus)
            0x00_301D, // I-RAM dump: SNES VC read 2
        ]),
        RunOracle::Sa1IdlePc {
            idle_pc: 0x9DEB, // SA1TestFinished_InfLoop, per SA1VersionCodeTest.sym
            addr: TEST_FINISHED_ADDR,
            pass_value: 0xFF, // the ROM's only terminal value; see the doc comment
        },
    );

    let diagnostics = format!(
        "TestFinished=${:02X} TestVersionVC=${:02X} TestVersionVCTrue=${:02X} \
         SFR=[{:02X} {:02X}] VC=[{:02X} {:02X}] (ticks={} frames={} pc=${:04X})",
        result.debug_bytes[0],
        result.debug_bytes[1],
        result.debug_bytes[2],
        result.debug_bytes[3],
        result.debug_bytes[4],
        result.debug_bytes[5],
        result.debug_bytes[6],
        result.ticks,
        result.frames,
        result.pc
    );
    assert!(
        result.passed,
        "SA1VersionCodeTest did not run to completion: {diagnostics}"
    );
    assert_eq!(
        &result.debug_bytes[1..],
        &[0x23, 0x23, 0x00, 0x00, 0x23, 0xAA],
        "hardware-accurate open-bus observations: {diagnostics}"
    );
}
