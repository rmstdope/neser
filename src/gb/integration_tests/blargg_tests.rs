use crate::gb::bus::DmgBus;
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

/// Step `gb` until the serial output ends with "Passed\n" or "Failed\n",
/// or until `BLARGG_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Returns the full serial output collected as a `String`.
fn run_blargg_rom(gb: &mut Gb<DmgBus>) -> String {
    let start = gb.cycles();
    loop {
        let output = String::from_utf8_lossy(gb.cpu.bus.serial_output()).into_owned();
        if output.ends_with("Passed\n") || output.ends_with("Failed\n") {
            return output;
        }
        if gb.cycles().saturating_sub(start) >= BLARGG_CYCLE_LIMIT {
            return output;
        }
        gb.step();
    }
}

/// Decode the BG tile map 0 ($9800–$9BFF in VRAM) as printable ASCII.
///
/// Blargg tests that output to the LCD (e.g. halt_bug) write ASCII character
/// codes directly as tile indices into the 32×32 tile map.  The visible area
/// is 20×18 tiles so we read the first 18 rows of 20 tiles each.
///
/// Returns a `String` containing the decoded visible tile map content with
/// newlines between rows, with trailing whitespace stripped from each row.
fn vram_tilemap_text(gb: &Gb<DmgBus>) -> String {
    let tile_map = &gb.cpu.bus.ppu.vram[0x1800..0x1C00];
    let mut result = String::new();
    for row in 0..18usize {
        let start = row * 32;
        let row_text: String = tile_map[start..start + 20]
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    ' '
                }
            })
            .collect::<String>();
        let trimmed = row_text.trim_end();
        if !trimmed.is_empty() {
            result.push_str(trimmed);
            result.push('\n');
        }
    }
    result
}

/// Run the halt_bug ROM until the decoded tilemap contains "Passed" or
/// "Failed", or until `BLARGG_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Polls the tilemap every 50 000 M-cycles rather than always running to the
/// full budget, keeping CI runtime predictable.
fn run_blargg_rom_lcd(gb: &mut Gb<DmgBus>) -> String {
    const POLL_INTERVAL: u64 = 50_000;
    let start = gb.cycles();
    loop {
        let elapsed = gb.cycles().saturating_sub(start);
        if elapsed >= BLARGG_CYCLE_LIMIT {
            break;
        }
        // Step one poll interval.
        let poll_end = start + elapsed + POLL_INTERVAL;
        while gb.cycles() < poll_end {
            gb.step();
        }
        let text = vram_tilemap_text(gb);
        if text.contains("Passed") || text.contains("Failed") {
            return text;
        }
    }
    vram_tilemap_text(gb)
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
fn test_halt_bug() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/halt_bug/halt_bug.gb");
    let output = run_blargg_rom_lcd(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed in LCD output, got: {output:?}"
    );
}

#[test]
#[ignore = "failing: memory access timing inaccuracies — tracked in #1983"]
fn test_mem_timing() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/mem_timing/mem_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
#[ignore = "failing: memory access timing inaccuracies — tracked in #1983"]
fn test_mem_timing_2() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/mem_timing-2/mem_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}
