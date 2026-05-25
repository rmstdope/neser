use super::helpers::{load_cgb_rom, load_gb_rom};
use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::console::Gb;

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

/// Decode the BG tile map 0 ($9800–$9BFF in VRAM) as printable ASCII.
///
/// Blargg tests that output to the LCD (e.g. halt_bug) write ASCII character
/// codes directly as tile indices into the 32×32 tile map.  The visible area
/// is 20×18 tiles so we read the first 18 rows of 20 tiles each.
///
/// Returns a `String` containing the decoded visible tile map content with
/// newlines between rows, with trailing whitespace stripped from each row.
fn vram_tilemap_text<B: GbBus>(gb: &Gb<B>) -> String {
    let tile_map = &gb.cpu.bus.ppu().vram[0x1800..0x1C00];
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
fn run_blargg_rom_lcd<B: GbBus>(gb: &mut Gb<B>) -> String {
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
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/01-special.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_02_interrupts() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/02-interrupts.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_03_op_sp_hl() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/03-op sp,hl.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_04_op_r_imm() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/04-op r,imm.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_05_op_rp() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/05-op rp.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_06_ld_r_r() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/06-ld r,r.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_07_jr_jp_call_ret_rst() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cpu_instrs/individual/07-jr,jp,call,ret,rst.gb",
    );
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_08_misc_instrs() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/08-misc instrs.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_09_op_r_r() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/09-op r,r.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_10_bit_ops() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/10-bit ops.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cpu_instrs_11_op_a_hl() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cpu_instrs/individual/11-op a,(hl).gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

// ── Other Blargg CPU ROMs ─────────────────────────────────────────────────────

#[test]
fn test_instr_timing() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/instr_timing/instr_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_halt_bug() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/halt_bug/halt_bug.gb");
    let output = run_blargg_rom_lcd(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed in LCD output, got: {output:?}"
    );
}

#[test]
fn test_interrupt_time() {
    let mut gb = load_cgb_rom("roms/gb/automated_tests/blargg/interrupt_time/interrupt_time.gb");
    let output = run_blargg_rom_lcd(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed in LCD output, got: {output:?}"
    );
}

#[test]
fn test_mem_timing() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/mem_timing/mem_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_01_read_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing/individual/01-read_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_02_write_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing/individual/02-write_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_03_modify_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing/individual/03-modify_timing.gb");
    let output = run_blargg_rom(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_2() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/mem_timing-2/mem_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_2_01_read_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing-2/rom_singles/01-read_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_2_02_write_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing-2/rom_singles/02-write_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_mem_timing_2_03_modify_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/mem_timing-2/rom_singles/03-modify_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

// ── dmg_sound single ROMs ────────────────────────────────────────────────────

#[test]
fn test_dmg_sound_01_registers() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/01-registers.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_02_len_ctr() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/02-len ctr.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_03_trigger() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/03-trigger.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_04_sweep() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/04-sweep.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_05_sweep_details() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/05-sweep details.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_06_overflow_on_trigger() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/06-overflow on trigger.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_07_len_sweep_period_sync() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/07-len sweep period sync.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_08_len_ctr_during_power() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/08-len ctr during power.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_09_wave_read_while_on() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/09-wave read while on.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_10_wave_trigger_while_on() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/10-wave trigger while on.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_11_regs_after_power() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/dmg_sound/rom_singles/11-regs after power.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_dmg_sound_12_wave_write_while_on() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/dmg_sound/rom_singles/12-wave write while on.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

// ── cgb_sound single ROMs ─────────────────────────────────────────────────────

#[test]
fn test_cgb_sound_01_registers() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/01-registers.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_02_len_ctr() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/02-len ctr.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_03_trigger() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/03-trigger.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_04_sweep() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/04-sweep.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_05_sweep_details() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/05-sweep details.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_06_overflow_on_trigger() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cgb_sound/rom_singles/06-overflow on trigger.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_07_len_sweep_period_sync() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cgb_sound/rom_singles/07-len sweep period sync.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_08_len_ctr_during_power() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cgb_sound/rom_singles/08-len ctr during power.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_09_wave_read_while_on() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cgb_sound/rom_singles/09-wave read while on.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_10_wave_trigger_while_on() {
    let mut gb = load_gb_rom(
        "roms/gb/automated_tests/blargg/cgb_sound/rom_singles/10-wave trigger while on.gb",
    );
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_11_regs_after_power() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/11-regs after power.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_cgb_sound_12_wave() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/cgb_sound/rom_singles/12-wave.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

// ── OAM bug single ROMs ───────────────────────────────────────────────────────

#[test]
fn test_oam_bug_1_lcd_sync() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/1-lcd_sync.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_2_causes() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/2-causes.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_3_non_causes() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/3-non_causes.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_4_scanline_timing() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/4-scanline_timing.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_5_timing_bug() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/5-timing_bug.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_6_timing_no_bug() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/6-timing_no_bug.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
#[ignore = "failing: OAM corruption not emulated — tracked in #1993"]
fn test_oam_bug_7_timing_effect() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/7-timing_effect.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

#[test]
fn test_oam_bug_8_instr_effect() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/8-instr_effect.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}
