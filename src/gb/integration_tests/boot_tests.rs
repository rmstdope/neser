use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;

/// The expected Nintendo logo bitmap (48 bytes) at cartridge header $0104–$0133.
///
/// Per Pan Docs: <https://gbdev.io/pandocs/The_Cartridge_Header.html#0104-0133--nintendo-logo>
const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D,
    0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99,
    0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];

/// Maximum M-cycles to allow for the boot sequence.
///
/// The full DMG boot sequence takes ~1.5 M M-cycles (~80 frames × 17 556 cycles/frame).
/// 2.5 M gives ample headroom without running forever on a lock-up scenario.
const BOOT_CYCLE_LIMIT: u64 = 2_500_000;

/// Build a minimal 32 KiB ROM for boot tests.
///
/// Places `logo` at $0104–$0133 and a `JR $-2` infinite loop at $0100 so the
/// running test can detect exactly when the boot ROM hands off to the cartridge.
/// All other header bytes are zero (ROM-only, no RAM, no MBC).  The header
/// checksum at $014D is recomputed from the header bytes $0134–$014C.
fn build_test_rom(logo: &[u8; 48]) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    // Entry point: `JR $-2` loops at $0100 (2 bytes, 8 T-cycles/2 M-cycles)
    rom[0x0100] = 0x18; // JR opcode
    rom[0x0101] = 0xFE; // offset -2 → jumps back to $0100
    // Nintendo logo
    rom[0x0104..=0x0133].copy_from_slice(logo);
    // Cartridge type / ROM+RAM size
    rom[0x0147] = 0x00; // ROM only
    rom[0x0148] = 0x00; // 32 KiB
    rom[0x0149] = 0x00; // no RAM
    // Header checksum over $0134–$014C (all zero in this ROM)
    let chk = rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
    rom[0x014D] = chk;
    rom
}

/// Step `gb` until PC reaches $0100 or `BOOT_CYCLE_LIMIT` M-cycles have elapsed.
///
/// Returns `true` if $0100 was reached; `false` if the cycle budget was exhausted.
fn run_until_cartridge_entry(gb: &mut Gb<DmgBus>) -> bool {
    let start = gb.cycles();
    loop {
        if gb.cpu.regs.pc == 0x0100 {
            return true;
        }
        if gb.cycles().saturating_sub(start) >= BOOT_CYCLE_LIMIT {
            return false;
        }
        gb.step();
    }
}

/// After the DMG boot ROM completes, the CPU registers must match the documented
/// post-boot-ROM state.
///
/// Per Pan Docs: <https://gbdev.io/pandocs/Power_Up_Sequence.html#cpu-registers>
#[test]
fn test_dmg_boot_sets_correct_register_state() {
    let rom = build_test_rom(&NINTENDO_LOGO);
    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));

    let reached = run_until_cartridge_entry(&mut gb);

    assert!(
        reached,
        "Boot ROM never handed off to $0100 within the cycle limit"
    );
    assert_eq!(gb.cpu.regs.a, 0x01, "A register after boot");
    assert_eq!(gb.cpu.regs.f, 0xB0, "F register after boot");
    assert_eq!(gb.cpu.regs.b, 0x00, "B register after boot");
    assert_eq!(gb.cpu.regs.c, 0x13, "C register after boot");
    assert_eq!(gb.cpu.regs.d, 0x00, "D register after boot");
    assert_eq!(gb.cpu.regs.e, 0xD8, "E register after boot");
    assert_eq!(gb.cpu.regs.h, 0x01, "H register after boot");
    assert_eq!(gb.cpu.regs.l, 0x4D, "L register after boot");
    assert_eq!(gb.cpu.regs.sp, 0xFFFE, "SP after boot");
    assert_eq!(gb.cpu.regs.pc, 0x0100, "PC after boot");
}

/// A cartridge with a corrupted Nintendo logo must cause the boot ROM to lock up,
/// never handing off to $0100 — matching real DMG hardware behaviour.
///
/// Per Pan Docs: the boot ROM re-reads the logo after the scroll animation and
/// compares it byte-by-byte against the reference copy.  Mismatch → infinite loop.
#[test]
fn test_dmg_boot_halts_on_corrupted_logo() {
    let bad_logo = [0u8; 48]; // all-zero logo — definitely wrong
    let rom = build_test_rom(&bad_logo);
    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));

    let reached = run_until_cartridge_entry(&mut gb);

    assert!(
        !reached,
        "Boot ROM must lock up on a corrupted logo; it must not hand off to $0100"
    );
}

// ============================================================================
// IO register post-boot verification (Pan Docs reference values)
// ============================================================================

/// Helper: boot a fresh DMG with the given model and return it at PC=$0100.
fn boot_to_cartridge_entry(model: DmgModel) -> Gb<DmgBus> {
    let rom = build_test_rom(&NINTENDO_LOGO);
    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(DmgBus::new(cart, model));
    let reached = run_until_cartridge_entry(&mut gb);
    assert!(reached, "Boot ROM must reach $0100 for model {:?}", model);
    gb
}

/// Helper: read an IO register by address.
fn read_io(gb: &mut Gb<DmgBus>, addr: u16) -> u8 {
    gb.cpu.bus.read(addr)
}

/// Verify all documented post-boot IO register values for DMG production hardware
/// (DMG-A/B/C) against the Pan Docs reference table.
///
/// Reference: <https://gbdev.io/pandocs/Power_Up_Sequence.html#hardware-registers>
#[test]
fn test_dmg_production_boot_io_registers() {
    let mut gb = boot_to_cartridge_entry(DmgModel::DmgB);

    // Serial
    assert_eq!(read_io(&mut gb, 0xFF01), 0x00, "SB ($FF01)");
    assert_eq!(read_io(&mut gb, 0xFF02), 0x7E, "SC ($FF02)");

    // Timer
    assert_eq!(read_io(&mut gb, 0xFF04), 0xAB, "DIV ($FF04)");
    assert_eq!(read_io(&mut gb, 0xFF05), 0x00, "TIMA ($FF05)");
    assert_eq!(read_io(&mut gb, 0xFF06), 0x00, "TMA ($FF06)");
    assert_eq!(read_io(&mut gb, 0xFF07), 0xF8, "TAC ($FF07)");

    // Interrupt flag
    assert_eq!(read_io(&mut gb, 0xFF0F), 0xE1, "IF ($FF0F)");

    // APU registers
    assert_eq!(read_io(&mut gb, 0xFF10), 0x80, "NR10 ($FF10)");
    assert_eq!(read_io(&mut gb, 0xFF11), 0xBF, "NR11 ($FF11)");
    assert_eq!(read_io(&mut gb, 0xFF12), 0xF3, "NR12 ($FF12)");
    assert_eq!(read_io(&mut gb, 0xFF13), 0xFF, "NR13 ($FF13)");
    assert_eq!(read_io(&mut gb, 0xFF14), 0xBF, "NR14 ($FF14)");
    assert_eq!(read_io(&mut gb, 0xFF16), 0x3F, "NR21 ($FF16)");
    assert_eq!(read_io(&mut gb, 0xFF17), 0x00, "NR22 ($FF17)");
    assert_eq!(read_io(&mut gb, 0xFF18), 0xFF, "NR23 ($FF18)");
    assert_eq!(read_io(&mut gb, 0xFF19), 0xBF, "NR24 ($FF19)");
    assert_eq!(read_io(&mut gb, 0xFF1A), 0x7F, "NR30 ($FF1A)");
    assert_eq!(read_io(&mut gb, 0xFF1B), 0xFF, "NR31 ($FF1B)");
    assert_eq!(read_io(&mut gb, 0xFF1C), 0x9F, "NR32 ($FF1C)");
    assert_eq!(read_io(&mut gb, 0xFF1D), 0xFF, "NR33 ($FF1D)");
    assert_eq!(read_io(&mut gb, 0xFF1E), 0xBF, "NR34 ($FF1E)");
    assert_eq!(read_io(&mut gb, 0xFF20), 0xFF, "NR41 ($FF20)");
    assert_eq!(read_io(&mut gb, 0xFF21), 0x00, "NR42 ($FF21)");
    assert_eq!(read_io(&mut gb, 0xFF22), 0x00, "NR43 ($FF22)");
    assert_eq!(read_io(&mut gb, 0xFF23), 0xBF, "NR44 ($FF23)");
    assert_eq!(read_io(&mut gb, 0xFF24), 0x77, "NR50 ($FF24)");
    assert_eq!(read_io(&mut gb, 0xFF25), 0xF3, "NR51 ($FF25)");
    assert_eq!(read_io(&mut gb, 0xFF26), 0xF1, "NR52 ($FF26)");

    // PPU registers
    assert_eq!(read_io(&mut gb, 0xFF40), 0x91, "LCDC ($FF40)");
    // STAT ($FF41) and LY ($FF44) are timing-sensitive (change every few dots);
    // the Mooneye boot_hwio-dmgABCmgb test already verifies them with correct
    // cycle alignment so we skip them in this direct-read audit.
    assert_eq!(read_io(&mut gb, 0xFF42), 0x00, "SCY ($FF42)");
    assert_eq!(read_io(&mut gb, 0xFF43), 0x00, "SCX ($FF43)");
    assert_eq!(read_io(&mut gb, 0xFF45), 0x00, "LYC ($FF45)");
    assert_eq!(read_io(&mut gb, 0xFF46), 0xFF, "DMA ($FF46)");
    assert_eq!(read_io(&mut gb, 0xFF47), 0xFC, "BGP ($FF47)");
    // OBP0 ($FF48) and OBP1 ($FF49) are uninitialized per Pan Docs — not verified.
    assert_eq!(read_io(&mut gb, 0xFF4A), 0x00, "WY ($FF4A)");
    assert_eq!(read_io(&mut gb, 0xFF4B), 0x00, "WX ($FF4B)");

    // Interrupt enable
    assert_eq!(read_io(&mut gb, 0xFFFF), 0x00, "IE ($FFFF)");
}

/// Verify all documented post-boot IO register values for DMG-0 hardware
/// against the Pan Docs reference table.
///
/// Key differences from DMG production: DIV=$18, STAT=$81, LY=$91 (shorter boot ROM).
///
/// Reference: <https://gbdev.io/pandocs/Power_Up_Sequence.html#hardware-registers>
#[test]
fn test_dmg0_boot_io_registers() {
    let mut gb = boot_to_cartridge_entry(DmgModel::Dmg0);

    // Serial
    assert_eq!(read_io(&mut gb, 0xFF01), 0x00, "SB ($FF01)");
    assert_eq!(read_io(&mut gb, 0xFF02), 0x7E, "SC ($FF02)");

    // Timer
    assert_eq!(read_io(&mut gb, 0xFF04), 0x18, "DIV ($FF04)");
    assert_eq!(read_io(&mut gb, 0xFF05), 0x00, "TIMA ($FF05)");
    assert_eq!(read_io(&mut gb, 0xFF06), 0x00, "TMA ($FF06)");
    assert_eq!(read_io(&mut gb, 0xFF07), 0xF8, "TAC ($FF07)");

    // Interrupt flag
    assert_eq!(read_io(&mut gb, 0xFF0F), 0xE1, "IF ($FF0F)");

    // APU registers (identical to production)
    assert_eq!(read_io(&mut gb, 0xFF10), 0x80, "NR10 ($FF10)");
    assert_eq!(read_io(&mut gb, 0xFF11), 0xBF, "NR11 ($FF11)");
    assert_eq!(read_io(&mut gb, 0xFF12), 0xF3, "NR12 ($FF12)");
    assert_eq!(read_io(&mut gb, 0xFF13), 0xFF, "NR13 ($FF13)");
    assert_eq!(read_io(&mut gb, 0xFF14), 0xBF, "NR14 ($FF14)");
    assert_eq!(read_io(&mut gb, 0xFF16), 0x3F, "NR21 ($FF16)");
    assert_eq!(read_io(&mut gb, 0xFF17), 0x00, "NR22 ($FF17)");
    assert_eq!(read_io(&mut gb, 0xFF18), 0xFF, "NR23 ($FF18)");
    assert_eq!(read_io(&mut gb, 0xFF19), 0xBF, "NR24 ($FF19)");
    assert_eq!(read_io(&mut gb, 0xFF1A), 0x7F, "NR30 ($FF1A)");
    assert_eq!(read_io(&mut gb, 0xFF1B), 0xFF, "NR31 ($FF1B)");
    assert_eq!(read_io(&mut gb, 0xFF1C), 0x9F, "NR32 ($FF1C)");
    assert_eq!(read_io(&mut gb, 0xFF1D), 0xFF, "NR33 ($FF1D)");
    assert_eq!(read_io(&mut gb, 0xFF1E), 0xBF, "NR34 ($FF1E)");
    assert_eq!(read_io(&mut gb, 0xFF20), 0xFF, "NR41 ($FF20)");
    assert_eq!(read_io(&mut gb, 0xFF21), 0x00, "NR42 ($FF21)");
    assert_eq!(read_io(&mut gb, 0xFF22), 0x00, "NR43 ($FF22)");
    assert_eq!(read_io(&mut gb, 0xFF23), 0xBF, "NR44 ($FF23)");
    assert_eq!(read_io(&mut gb, 0xFF24), 0x77, "NR50 ($FF24)");
    assert_eq!(read_io(&mut gb, 0xFF25), 0xF3, "NR51 ($FF25)");
    assert_eq!(read_io(&mut gb, 0xFF26), 0xF1, "NR52 ($FF26)");

    // PPU registers — DMG-0 has different timing values (shorter boot ROM)
    assert_eq!(read_io(&mut gb, 0xFF40), 0x91, "LCDC ($FF40)");
    // STAT ($FF41) and LY ($FF44) are timing-sensitive; verified by Mooneye
    // boot_hwio-dmg0 test instead.
    assert_eq!(read_io(&mut gb, 0xFF42), 0x00, "SCY ($FF42)");
    assert_eq!(read_io(&mut gb, 0xFF43), 0x00, "SCX ($FF43)");
    assert_eq!(read_io(&mut gb, 0xFF45), 0x00, "LYC ($FF45)");
    assert_eq!(read_io(&mut gb, 0xFF46), 0xFF, "DMA ($FF46)");
    assert_eq!(read_io(&mut gb, 0xFF47), 0xFC, "BGP ($FF47)");
    // OBP0 ($FF48) and OBP1 ($FF49) are uninitialized per Pan Docs — not verified.
    assert_eq!(read_io(&mut gb, 0xFF4A), 0x00, "WY ($FF4A)");
    assert_eq!(read_io(&mut gb, 0xFF4B), 0x00, "WX ($FF4B)");

    // Interrupt enable
    assert_eq!(read_io(&mut gb, 0xFFFF), 0x00, "IE ($FFFF)");
}

// ============================================================================
// DMG-A/B/C identical post-boot state verification
// ============================================================================

/// DMG-A, DMG-B, and DMG-C must produce byte-identical post-boot state.
///
/// They share the same boot ROM (Production variant) and should produce
/// identical CPU register values, IO register values, and timing at $0100.
#[test]
fn test_dmg_a_b_c_produce_identical_post_boot_state() {
    let mut gb_a = boot_to_cartridge_entry(DmgModel::DmgA);
    let mut gb_b = boot_to_cartridge_entry(DmgModel::DmgB);
    let mut gb_c = boot_to_cartridge_entry(DmgModel::DmgC);

    // CPU registers
    assert_eq!(gb_a.cpu.regs.af(), gb_b.cpu.regs.af(), "AF: DMG-A vs DMG-B");
    assert_eq!(gb_b.cpu.regs.af(), gb_c.cpu.regs.af(), "AF: DMG-B vs DMG-C");
    assert_eq!(gb_a.cpu.regs.bc(), gb_b.cpu.regs.bc(), "BC: DMG-A vs DMG-B");
    assert_eq!(gb_b.cpu.regs.bc(), gb_c.cpu.regs.bc(), "BC: DMG-B vs DMG-C");
    assert_eq!(gb_a.cpu.regs.de(), gb_b.cpu.regs.de(), "DE: DMG-A vs DMG-B");
    assert_eq!(gb_b.cpu.regs.de(), gb_c.cpu.regs.de(), "DE: DMG-B vs DMG-C");
    assert_eq!(gb_a.cpu.regs.hl(), gb_b.cpu.regs.hl(), "HL: DMG-A vs DMG-B");
    assert_eq!(gb_b.cpu.regs.hl(), gb_c.cpu.regs.hl(), "HL: DMG-B vs DMG-C");
    assert_eq!(gb_a.cpu.regs.sp, gb_b.cpu.regs.sp, "SP: DMG-A vs DMG-B");
    assert_eq!(gb_b.cpu.regs.sp, gb_c.cpu.regs.sp, "SP: DMG-B vs DMG-C");

    // Elapsed cycles
    assert_eq!(gb_a.cycles(), gb_b.cycles(), "Cycles: DMG-A vs DMG-B");
    assert_eq!(gb_b.cycles(), gb_c.cycles(), "Cycles: DMG-B vs DMG-C");

    // IO registers — compare a comprehensive set
    let io_addrs: &[u16] = &[
        0xFF01, 0xFF02, 0xFF04, 0xFF05, 0xFF06, 0xFF07, 0xFF0F, 0xFF10, 0xFF11, 0xFF12, 0xFF13,
        0xFF14, 0xFF16, 0xFF17, 0xFF18, 0xFF19, 0xFF1A, 0xFF1B, 0xFF1C, 0xFF1D, 0xFF1E, 0xFF20,
        0xFF21, 0xFF22, 0xFF23, 0xFF24, 0xFF25, 0xFF26, 0xFF40, 0xFF41, 0xFF42, 0xFF43, 0xFF44,
        0xFF45, 0xFF46, 0xFF47, 0xFF4A, 0xFF4B, 0xFFFF,
    ];
    for &addr in io_addrs {
        let a = read_io(&mut gb_a, addr);
        let b = read_io(&mut gb_b, addr);
        let c = read_io(&mut gb_c, addr);
        assert_eq!(
            a, b,
            "IO ${:04X}: DMG-A (${:02X}) vs DMG-B (${:02X})",
            addr, a, b
        );
        assert_eq!(
            b, c,
            "IO ${:04X}: DMG-B (${:02X}) vs DMG-C (${:02X})",
            addr, b, c
        );
    }
}

/// DMG-0 boot ROM also sets correct CPU register state.
///
/// Per Pan Docs: A=$01, F=$00, B=$FF, C=$13, D=$00, E=$C1, H=$84, L=$03
#[test]
fn test_dmg0_boot_sets_correct_register_state() {
    let gb = boot_to_cartridge_entry(DmgModel::Dmg0);

    assert_eq!(gb.cpu.regs.a, 0x01, "A register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.f, 0x00, "F register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.b, 0xFF, "B register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.c, 0x13, "C register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.d, 0x00, "D register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.e, 0xC1, "E register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.h, 0x84, "H register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.l, 0x03, "L register after DMG-0 boot");
    assert_eq!(gb.cpu.regs.sp, 0xFFFE, "SP after DMG-0 boot");
    assert_eq!(gb.cpu.regs.pc, 0x0100, "PC after DMG-0 boot");
}
