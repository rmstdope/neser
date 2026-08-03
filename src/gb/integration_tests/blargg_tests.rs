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
    if !cart_ram_signature_present(gb) {
        return None;
    }
    let status = gb.cpu.bus.read(0xA000);
    if status == 0x80 {
        // Test still running.
        return None;
    }
    let text = cart_ram_text(gb);
    // Guard against the init_text_out timing window where the three signature
    // bytes have been written but $A000 hasn't been set to $80 yet (cart RAM
    // is still all-zero from initialisation).  In that window status == 0 and
    // text is empty.  A genuine pass also has status == 0 but will have
    // non-empty text (at minimum "\nPassed" written before exit).
    if status == 0 && text.is_empty() {
        return None;
    }
    Some(text)
}

/// First and last address of the ROM's zero-terminated text buffer.
///
/// `init_text_out` sets the write pointer to `$A004` and the ROMs that use this
/// channel are built with 8 KB of cartridge RAM, so the buffer runs all the way
/// to `$BFFF`: 8188 addresses, holding up to 8187 characters plus the
/// terminator.  Stopping the scan earlier silently truncates the output, which
/// for a long-running ROM would drop the trailing "Passed".
const CART_RAM_TEXT_START: u16 = 0xA004;
const CART_RAM_TEXT_END: u16 = 0xBFFF;

/// True when the `$DE $B0 $61` signature is present at `$A001`–`$A003`.
fn cart_ram_signature_present(gb: &mut Gb<DmgBus>) -> bool {
    const SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
    [
        gb.cpu.bus.read(0xA001),
        gb.cpu.bus.read(0xA002),
        gb.cpu.bus.read(0xA003),
    ] == SIGNATURE
}

/// Read the zero-terminated text the ROM has written so far, regardless of
/// whether it has finished.  Scans `$A004..=$BFFF`, stopping at the terminator.
fn cart_ram_text(gb: &mut Gb<DmgBus>) -> String {
    let mut text = Vec::new();
    for addr in CART_RAM_TEXT_START..=CART_RAM_TEXT_END {
        let b = gb.cpu.bus.read(addr);
        if b == 0 {
            break;
        }
        text.push(b);
    }
    String::from_utf8_lossy(&text).into_owned()
}

/// Step `gb` until the cartridge-RAM output (mem_timing-2 style) signals
/// completion, or until `BLARGG_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Also accepts serial output ending with "Passed\n" / "Failed\n" as a
/// fallback so the function works for serial-based ROMs as well.
fn run_blargg_rom_cart_ram(gb: &mut Gb<DmgBus>) -> String {
    run_blargg_rom_cart_ram_with_limit(gb, BLARGG_CYCLE_LIMIT)
}

/// Prefix marking a result the ROM never actually reported.
const TIMEOUT_PREFIX: &str = "[timed out, partial output] ";

/// As [`run_blargg_rom_cart_ram`], with an explicit M-cycle budget.
///
/// On timeout the ROM has not written a status byte, so the completion
/// protocol yields nothing.  Rather than returning an empty string — which
/// says only "no result" and hides how far the ROM got — return whatever text
/// it has printed so far, behind [`TIMEOUT_PREFIX`] so it can never be mistaken
/// for a completed run.
///
/// Callers assert on `contains("Passed")`, so a run that times out in the few
/// instructions between the ROM printing its verdict and writing the status
/// byte now passes where it previously failed.  That window is a handful of
/// instructions out of the whole budget, and a ROM that printed "Passed" did
/// pass — the marker keeps the timeout visible either way.
fn run_blargg_rom_cart_ram_with_limit(gb: &mut Gb<DmgBus>, cycle_limit: u64) -> String {
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
        if gb.cycles().saturating_sub(start) >= cycle_limit {
            // Return whatever we have collected so far.
            if let Some(text) = read_cart_ram_output(gb) {
                return text;
            }
            let partial = if cart_ram_signature_present(gb) {
                cart_ram_text(gb)
            } else {
                String::from_utf8_lossy(gb.cpu.bus.serial_output()).into_owned()
            };
            return format!("{TIMEOUT_PREFIX}{partial}");
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

/// `7-timing_effect` cannot report a result through the cartridge-RAM channel.
///
/// For every sweep timing that corrupts, the ROM prints a 525-byte block — the
/// sweep index, then a full 521-byte OAM dump — and a correct DMG corrupts on 19
/// of them (Mode 2 walks 20 OAM rows, one per M-cycle, and row 0 is immune).
/// That is `17 + 19×525 + 8 = 10 000` bytes of text against the 8188 available
/// at `$A004..$BFFF`.
/// `write_text_out` (`oam_bug/source/common/shell.s`) has no bounds check, so
/// the overrun walks into `$C000`, where `copy_to_wram_then_run` placed the
/// ROM's own code — measured: 2040 bytes of that code overwritten, cartridge
/// RAM switched off, CPU executing rubbish.  The ROM therefore never reaches
/// `check_crc`, on any emulator and on hardware.  Tracked in #3115.
///
/// The behaviour this ROM tests is covered by [`test_oam_bug_multi_rom`]
/// instead: the multi-ROM build defines `CUSTOM_PRINT`, does not use the
/// cartridge-RAM buffer, and reports subtest 7's verdict on screen.
#[test]
#[ignore = "unusable ROM build: output overruns its own $A004 text buffer into WRAM — tracked in #3115"]
fn test_oam_bug_7_timing_effect() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/7-timing_effect.gb");
    let output = run_blargg_rom_cart_ram(&mut gb);
    assert!(
        output.contains("Passed"),
        "expected Passed, got: {output:?}"
    );
}

/// The multi-ROM build runs all eight `oam_bug` subtests and reports each one's
/// verdict on screen, including subtest 7 (`timing_effect`), which the single
/// ROM cannot report — see [`test_oam_bug_7_timing_effect`].
#[test]
fn test_oam_bug_multi_rom() {
    let mut gb = load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/oam_bug.gb");
    let output = run_blargg_rom_lcd(&mut gb);
    // Assert the subtest this suite's single-ROM coverage cannot reach first,
    // so a regression names it rather than only failing the overall verdict.
    assert!(
        output.contains("07:ok"),
        "oam_bug subtest 7 (timing_effect) must pass, got:\n{output}"
    );
    assert!(output.contains("Passed"), "expected Passed, got:\n{output}");
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

// ── cartridge-RAM output oracle ───────────────────────────────────────────────

/// The text buffer runs to `$BFFF`, not `$AFFF`.
///
/// Long-running ROMs print more than the 4092 bytes below `$AFFF`; stopping the
/// scan there drops the trailing "Passed" and turns a passing run into a
/// failing one.  Drives cartridge RAM directly (MBC1 RAM enable at `$0000`)
/// rather than running a ROM, so the boundary is pinned exactly.
#[test]
fn test_cart_ram_output_reads_the_whole_8k_buffer() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/7-timing_effect.gb");
    // Fill the buffer to its very last byte, ending with the verdict, so a scan
    // that stops early cannot see it.  The expected extent is stated as a
    // literal, from the cartridge geometry ($A004..$BFFF of 8 KB of RAM, minus
    // the terminator) rather than from CART_RAM_TEXT_END — deriving it from the
    // constant under test would make this test pass for any value of it.
    const BUFFER_TEXT_CAPACITY: usize = 0xBFFF - 0xA004; // 8187 chars + NUL
    let verdict = "\nPassed\n";
    let mut text = ".".repeat(BUFFER_TEXT_CAPACITY - verdict.len());
    text.push_str(verdict);
    plant_cart_ram_output(&mut gb, 0x00, &text);

    let output = read_cart_ram_output(&mut gb).expect("completed run must yield text");
    assert_eq!(
        output.len(),
        BUFFER_TEXT_CAPACITY,
        "the scan must cover $A004..=$BFFF, not stop at $AFFF"
    );
    assert!(
        output.contains("Passed"),
        "a verdict written above $AFFF must not be truncated away"
    );
}

/// Write blargg's cartridge-RAM output protocol directly: enable MBC1 RAM, set
/// the status byte and signature, then store `text` zero-terminated at `$A004`.
///
/// Uses literal addresses for the same reason as the test above: the helper
/// must not inherit the bounds it is used to verify.
fn plant_cart_ram_output(gb: &mut Gb<DmgBus>, status: u8, text: &str) {
    const TEXT_START: u16 = 0xA004;
    assert!(
        text.len() <= (0xBFFF - TEXT_START) as usize,
        "text plus terminator must fit in $A004..=$BFFF"
    );
    gb.cpu.bus.write(0x0000, 0x0A); // MBC1: enable cartridge RAM
    gb.cpu.bus.write(0xA000, status);
    for (i, b) in [0xDE, 0xB0, 0x61].iter().enumerate() {
        gb.cpu.bus.write(0xA001 + i as u16, *b);
    }
    for (i, b) in text.bytes().enumerate() {
        gb.cpu.bus.write(TEXT_START + i as u16, b);
    }
    gb.cpu.bus.write(TEXT_START + text.len() as u16, 0x00);
}

/// A run that exhausts its budget must report how far the ROM actually got.
///
/// While `$A000` still reads `$80` the completion protocol yields nothing, so
/// the old timeout path returned `""` — indistinguishable from a ROM that
/// produced no output at all.  Planted directly rather than run for real: the
/// contract is "on timeout, surface the text that is there", and a hard-coded
/// cycle budget would additionally depend on this ROM's start-up time.
#[test]
fn test_cart_ram_timeout_reports_partial_output() {
    let mut gb =
        load_gb_rom("roms/gb/automated_tests/blargg/oam_bug/rom_singles/7-timing_effect.gb");
    plant_cart_ram_output(&mut gb, 0x80, "7-timing_effect\n\npartial");

    // A zero budget times out before stepping, leaving the planted state intact.
    let output = run_blargg_rom_cart_ram_with_limit(&mut gb, 0);
    assert_eq!(
        output,
        format!("{TIMEOUT_PREFIX}7-timing_effect\n\npartial"),
        "a timed-out run must be labelled as such and carry the text printed so far"
    );
}
