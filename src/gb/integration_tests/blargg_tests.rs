use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;

fn load_gb_rom(path: &str) -> Gb<DmgBus> {
    let rom = std::fs::read(path).expect("ROM file should be present");
    let cart = load_cartridge(&rom).expect("valid GB ROM");
    Gb::new(DmgBus::new(cart))
}

/// Generous M-cycle budget per test.
///
/// Blargg individual cpu_instrs tests each take roughly 1–10 seconds of
/// real DMG time (~4 194 304 M-cycles/s).  150 M gives ample budget for
/// the slowest tests without running forever on a lockup.
const BLARGG_CYCLE_LIMIT: u64 = 150_000_000;

/// Return `true` if the serial output byte slice ends with `b"Passed\n"` or
/// `b"Failed\n"` — without allocating a `String`.
fn serial_is_done(output: &[u8]) -> bool {
    output.ends_with(b"Passed\n") || output.ends_with(b"Failed\n")
}

/// Step `gb` until the serial output ends with "Passed\n" or "Failed\n",
/// or until `BLARGG_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Returns the full serial output collected as a `String`.
fn run_blargg_rom(gb: &mut Gb<DmgBus>) -> String {
    let start = gb.cycles();
    loop {
        let output = gb.cpu.bus.serial_output();
        if serial_is_done(output) || gb.cycles().saturating_sub(start) >= BLARGG_CYCLE_LIMIT {
            return String::from_utf8_lossy(output).into_owned();
        }
        gb.step();
    }
}

/// Read a zero-terminated text string from `$A004` in cartridge RAM via the bus.
///
/// mem_timing-2 style ROMs write their output to cartridge RAM rather than the
/// serial port.  The readme documents the layout:
/// - `$A001`–`$A003`: signature `$DE $B0 $61` once the RAM is initialised
/// - `$A000`: `$80` while running, final result code (0 = pass) when done
/// - `$A004`+: zero-terminated text output string
///
/// **Timing note:** `init_text_out` writes the three signature bytes at
/// `$A001`–`$A003` *before* writing `$80` to `$A000`.  In that brief window
/// the signature already matches but `$A000` is still `0` (initial cart-RAM
/// value) and `$A004` is still empty.  A genuine "passed" exit also has
/// `$A000 == 0`, but by then the ROM has already printed at least `"\nPassed"`.
/// We therefore reject `status == 0 AND text.is_empty()` as the init window.
fn read_cart_ram_output(gb: &mut Gb<DmgBus>) -> Option<String> {
    const SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
    let sig = [
        gb.cpu.bus.read(0xA001),
        gb.cpu.bus.read(0xA002),
        gb.cpu.bus.read(0xA003),
    ];
    if sig != SIGNATURE {
        return None;
    }
    let status = gb.cpu.bus.read(0xA000);
    if status == 0x80 {
        // Test still running.
        return None;
    }
    let mut text = Vec::new();
    let mut addr: u16 = 0xA004;
    loop {
        let b = gb.cpu.bus.read(addr);
        if b == 0 {
            break;
        }
        text.push(b);
        addr = addr.wrapping_add(1);
        if addr > 0xAFFF {
            break;
        }
    }
    // Guard against the init_text_out timing window where the three signature
    // bytes have been written but $A000 hasn't been set to $80 yet (cart RAM
    // is still all-zero from initialisation).  In that window status == 0 and
    // text is empty.  A genuine pass also has status == 0 but will have
    // non-empty text (at minimum "\nPassed" written before exit).
    if status == 0 && text.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&text).into_owned())
}

/// Step `gb` until the cartridge-RAM output (mem_timing-2 style) signals
/// completion, or until `BLARGG_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Also accepts serial output ending with "Passed\n" / "Failed\n" as a
/// fallback so the function works for serial-based ROMs as well.
fn run_blargg_rom_cart_ram(gb: &mut Gb<DmgBus>) -> String {
    let start = gb.cycles();
    loop {
        // Check serial output on the raw byte slice — no allocation.
        if serial_is_done(gb.cpu.bus.serial_output()) {
            return String::from_utf8_lossy(gb.cpu.bus.serial_output()).into_owned();
        }
        // Check cartridge RAM output.
        if let Some(text) = read_cart_ram_output(gb) {
            return text;
        }
        if gb.cycles().saturating_sub(start) >= BLARGG_CYCLE_LIMIT {
            // Return whatever we have collected so far.
            if let Some(text) = read_cart_ram_output(gb) {
                return text;
            }
            return String::from_utf8_lossy(gb.cpu.bus.serial_output()).into_owned();
        }
        gb.step();
    }
}

// ── cpu_instrs individual ROMs ────────────────────────────────────────────────

#[test]
fn test_cpu_instrs_01_special() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/01-special.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_02_interrupts() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/02-interrupts.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_03_op_sp_hl() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/03-op sp,hl.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_04_op_r_imm() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/04-op r,imm.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_05_op_rp() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/05-op rp.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_06_ld_r_r() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/06-ld r,r.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_07_jr_jp_call_ret_rst() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/07-jr,jp,call,ret,rst.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_08_misc_instrs() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/08-misc instrs.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_09_op_r_r() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/09-op r,r.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_10_bit_ops() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/10-bit ops.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_11_op_a_hl() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/cpu_instrs/individual/11-op a,(hl).gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

// ── Other Blargg CPU ROMs ─────────────────────────────────────────────────────

#[test]
fn test_instr_timing() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/instr_timing/instr_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
#[ignore = "failing: HALT bug not implemented — tracked in #1982"]
fn test_halt_bug() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/halt_bug/halt_bug.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/mem_timing/mem_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_2() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/mem_timing-2/mem_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}
