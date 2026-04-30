use crate::gb::bus::{DmgBus, GbBus};
use crate::gb::cartridge::load_cartridge;
use crate::gb::console::Gb;
use crate::gb::model::DmgModel;

/// Maximum M-cycles to allow for the boot sequence.
///
/// The DMG boot sequence uses LY-polling VBlank sync: ~132 iterations × 2
/// VBlanks × 17 556 M-cycles/frame ≈ 4.6 M M-cycles.  8 M gives ample
/// headroom without running forever on a lock-up scenario.
const BOOT_CYCLE_LIMIT: u64 = 8_000_000;

/// Compute the header checksum for $0134–$014C.
fn compute_header_checksum(rom: &[u8]) -> u8 {
    rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1))
}

/// Build a minimal 32 KiB ROM for boot tests.
///
/// Places a `JR $-2` infinite loop at $0100 so the running test can detect
/// when the boot ROM hands off to the cartridge. The caller supplies the logo
/// bytes to place at $0104–$0133; the boot ROM loads whatever is there without
/// verification.
/// The header checksum at $014D is recomputed from $0134–$014C.
fn build_test_rom(logo: [u8; 48]) -> Vec<u8> {
    let mut rom = build_base_test_rom();
    rom[0x0104..0x0134].copy_from_slice(&logo);
    rom[0x014D] = compute_header_checksum(&rom);
    rom
}

/// Build a base 32 KiB test ROM with common setup.
///
/// Places a `JR $-2` infinite loop at $0100 and sets cartridge type/size fields.
/// The header checksum is NOT set; callers must set it after customizing the ROM.
fn build_base_test_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0100] = 0x18; // JR opcode
    rom[0x0101] = 0xFE; // offset -2 → jumps back to $0100
    // Cartridge type / ROM+RAM size
    rom[0x0147] = 0x00; // ROM only
    rom[0x0148] = 0x00; // 32 KiB
    rom[0x0149] = 0x00; // no RAM
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
    let rom = build_test_rom([0u8; 48]);
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

/// The boot ROM must accept any cartridge logo — no logo verification is performed.
/// This test uses a generated non-trivial logo pattern to confirm the boot ROM
/// completes normally without relying on a specific payload.
#[test]
fn test_dmg_boot_accepts_any_cartridge_logo() {
    let custom_logo = std::array::from_fn(|index| ((index as u8) << 1) ^ 0x5A);
    let rom = build_test_rom(custom_logo);
    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(DmgBus::new(cart, DmgModel::DmgB));

    let reached = run_until_cartridge_entry(&mut gb);

    assert!(
        reached,
        "Boot ROM must accept any cartridge logo and reach $0100"
    );
}

// ============================================================================
// IO register post-boot verification (Pan Docs reference values)
// ============================================================================

/// Helper: boot a fresh DMG with the given model and return it at PC=$0100.
fn boot_to_cartridge_entry(model: DmgModel) -> Gb<DmgBus> {
    let rom = build_test_rom([0u8; 48]);
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

// ============================================================================
// CGB boot ROM tests
// ============================================================================

use crate::gb::bus::CgbBus;
use crate::gb::model::CgbModel;

/// Build a minimal 32 KiB ROM for CGB boot tests.
///
/// Places a `JR $-2` infinite loop at $0100 and sets the CGB compatibility
/// flag at $0143 to indicate CGB-native mode.
fn build_cgb_test_rom() -> Vec<u8> {
    let mut rom = build_base_test_rom();
    // CGB compatibility flag: $80 = CGB-compatible, $C0 = CGB-only
    rom[0x0143] = 0x80;
    rom[0x014D] = compute_header_checksum(&rom);
    rom
}

/// Step CGB until PC reaches $0100 or cycle limit is exceeded.
fn run_cgb_until_cartridge_entry(gb: &mut Gb<CgbBus>) -> bool {
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

/// Helper: create a fresh CGB with the given model, ready to boot (PC at boot ROM start).
fn make_cgb_for_boot_test(model: CgbModel) -> Gb<CgbBus> {
    let rom = build_cgb_test_rom();
    let cart = load_cartridge(&rom).expect("valid ROM");
    // skip_boot_rom=false to actually run the boot ROM
    Gb::new(CgbBus::new(cart, model, false))
}

/// Helper: boot a fresh CGB with the given model and return it at PC=$0100.
fn boot_cgb_to_cartridge_entry(model: CgbModel) -> Gb<CgbBus> {
    let mut gb = make_cgb_for_boot_test(model);
    let reached = run_cgb_until_cartridge_entry(&mut gb);
    assert!(reached, "Boot ROM must reach $0100 for model {:?}", model);
    gb
}

/// Helper: read a value from the CGB bus.
fn read_cgb_bus(gb: &mut Gb<CgbBus>, addr: u16) -> u8 {
    gb.cpu.bus.read(addr)
}

/// After the CGB boot ROM completes, the CPU registers must match the
/// Mooneye-verified post-boot-ROM state.
///
/// Reference: Mooneye test `misc/boot_regs-cgb.s` (verified on real CGB hardware)
#[test]
fn test_cgb_boot_sets_correct_register_state() {
    let gb = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    assert_eq!(gb.cpu.regs.a, 0x11, "A register after CGB boot");
    assert_eq!(gb.cpu.regs.f, 0x80, "F register after CGB boot (Z=1)");
    assert_eq!(gb.cpu.regs.b, 0x00, "B register after CGB boot");
    assert_eq!(gb.cpu.regs.c, 0x00, "C register after CGB boot");
    assert_eq!(gb.cpu.regs.d, 0x00, "D register after CGB boot");
    assert_eq!(gb.cpu.regs.e, 0x08, "E register after CGB boot");
    assert_eq!(gb.cpu.regs.h, 0x00, "H register after CGB boot");
    assert_eq!(gb.cpu.regs.l, 0x7C, "L register after CGB boot");
    assert_eq!(gb.cpu.regs.sp, 0xFFFE, "SP after CGB boot");
    assert_eq!(gb.cpu.regs.pc, 0x0100, "PC after CGB boot");
}

/// Verify key IO registers after CGB boot ROM completes.
///
/// Reference: Pan Docs Power Up Sequence hardware registers table
#[test]
fn test_cgb_boot_sets_correct_io_registers() {
    let mut gb = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    // APU registers
    assert_eq!(read_cgb_bus(&mut gb, 0xFF24), 0x77, "NR50 ($FF24)");
    assert_eq!(read_cgb_bus(&mut gb, 0xFF25), 0xF3, "NR51 ($FF25)");
    assert_eq!(
        read_cgb_bus(&mut gb, 0xFF26),
        0xF1,
        "NR52 ($FF26) - APU on, CH1 active"
    );

    // PPU registers
    assert_eq!(read_cgb_bus(&mut gb, 0xFF40), 0x91, "LCDC ($FF40)");
    assert_eq!(read_cgb_bus(&mut gb, 0xFF47), 0xFC, "BGP ($FF47)");
}

/// The boot ROM must unmap itself after writing to $FF50.
/// Subsequent reads from $0000-$00FF should return cartridge data.
#[test]
fn test_cgb_boot_rom_unmaps_after_completion() {
    let mut gb = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    // Boot ROM should be inactive after reaching $0100
    assert!(
        !gb.cpu.bus.is_boot_rom_active(),
        "Boot ROM should be unmapped after boot completion"
    );

    // Read from $0000 should return cartridge data (all zeros in our test ROM)
    assert_eq!(
        read_cgb_bus(&mut gb, 0x0000),
        0x00,
        "Read from $0000 should return cartridge data after boot"
    );

    // Read from $0100-$0101 should return our JR -2 loop
    assert_eq!(
        read_cgb_bus(&mut gb, 0x0100),
        0x18,
        "Read from $0100 should return JR opcode"
    );
    assert_eq!(
        read_cgb_bus(&mut gb, 0x0101),
        0xFE,
        "Read from $0101 should return -2 offset"
    );
}

/// CGB-A through CGB-E should produce identical post-boot state.
///
/// All production CGB models share the same boot ROM and should produce
/// identical CPU register values and IO register values at $0100.
#[test]
fn test_cgb_a_through_e_produce_identical_post_boot_state() {
    let mut gb_a = boot_cgb_to_cartridge_entry(CgbModel::CgbA);
    let mut gb_e = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    // CPU registers
    assert_eq!(gb_a.cpu.regs.af(), gb_e.cpu.regs.af(), "AF: CGB-A vs CGB-E");
    assert_eq!(gb_a.cpu.regs.bc(), gb_e.cpu.regs.bc(), "BC: CGB-A vs CGB-E");
    assert_eq!(gb_a.cpu.regs.de(), gb_e.cpu.regs.de(), "DE: CGB-A vs CGB-E");
    assert_eq!(gb_a.cpu.regs.hl(), gb_e.cpu.regs.hl(), "HL: CGB-A vs CGB-E");
    assert_eq!(gb_a.cpu.regs.sp, gb_e.cpu.regs.sp, "SP: CGB-A vs CGB-E");

    // Key IO registers
    let io_addrs: &[u16] = &[0xFF24, 0xFF25, 0xFF26, 0xFF40, 0xFF47];
    for &addr in io_addrs {
        let a = read_cgb_bus(&mut gb_a, addr);
        let e = read_cgb_bus(&mut gb_e, addr);
        assert_eq!(
            a, e,
            "IO ${:04X}: CGB-A (${:02X}) vs CGB-E (${:02X})",
            addr, a, e
        );
    }
}

/// The CGB boot ROM accepts any cartridge without logo verification.
///
/// Unlike the real Nintendo boot ROM, our IPR-free implementation does not
/// verify the Nintendo logo, so any cartridge data is accepted.
#[test]
fn test_cgb_boot_accepts_any_cartridge() {
    let mut rom = build_cgb_test_rom();
    // Put garbage in the logo area
    for (i, byte) in rom[0x0104..0x0134].iter_mut().enumerate() {
        *byte = (((0x0104 + i) * 17) & 0xFF) as u8;
    }
    // Recompute header checksum
    let chk = rom[0x0134..=0x014C]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
    rom[0x014D] = chk;

    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(CgbBus::new(cart, CgbModel::CgbE, false));
    let reached = run_cgb_until_cartridge_entry(&mut gb);

    assert!(
        reached,
        "Boot ROM must accept any cartridge and reach $0100"
    );
}

// ============================================================================
// CGB-0 specific tests
// ============================================================================

/// CGB-0 boot ROM produces the same CPU register state as Production CGB.
///
/// Reference: SameBoy's CGB boot ROM uses identical register values for both
/// CGB-0 and Production CGB. Mooneye does not have a boot_regs-cgb0 test.
#[test]
fn test_cgb0_boot_sets_same_register_state_as_production() {
    let gb = boot_cgb_to_cartridge_entry(CgbModel::Cgb0);

    // Same values as Production CGB (Mooneye-verified)
    assert_eq!(gb.cpu.regs.a, 0x11, "A register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.f, 0x80, "F register after CGB-0 boot (Z=1)");
    assert_eq!(gb.cpu.regs.b, 0x00, "B register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.c, 0x00, "C register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.d, 0x00, "D register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.e, 0x08, "E register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.h, 0x00, "H register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.l, 0x7C, "L register after CGB-0 boot");
    assert_eq!(gb.cpu.regs.sp, 0xFFFE, "SP after CGB-0 boot");
    assert_eq!(gb.cpu.regs.pc, 0x0100, "PC after CGB-0 boot");
}

/// CGB-0 does NOT initialize wave RAM.
///
/// The behavioral requirement is that the boot ROM leaves wave RAM unchanged,
/// not that it forces any particular power-on pattern.
/// Reference: Pan Docs and SameBoy's implementation both confirm this.
#[test]
fn test_cgb0_boot_does_not_init_wave_ram() {
    let mut gb = make_cgb_for_boot_test(CgbModel::Cgb0);

    // Snapshot wave RAM before boot
    let wave_ram_before: Vec<u8> = (0xFF30..=0xFF3F)
        .map(|addr| read_cgb_bus(&mut gb, addr))
        .collect();

    let reached = run_cgb_until_cartridge_entry(&mut gb);
    assert!(reached, "Boot ROM must reach $0100");

    // Wave RAM ($FF30-$FF3F) should be unchanged for CGB-0
    for (addr, expected) in (0xFF30..=0xFF3F).zip(wave_ram_before.iter().copied()) {
        let val = read_cgb_bus(&mut gb, addr);
        assert_eq!(
            val, expected,
            "CGB-0 boot should preserve wave RAM at ${:04X} (before ${:02X}, after ${:02X})",
            addr, expected, val
        );
    }
}

/// Production CGB initializes wave RAM to an alternating 0x00/0xFF pattern.
///
/// Reference: SameBoy's CGB boot ROM implementation.
#[test]
fn test_cgb_production_boot_inits_wave_ram() {
    let mut gb = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    // Wave RAM ($FF30-$FF3F) should alternate 0x00/0xFF for Production CGB
    for (i, addr) in (0xFF30..=0xFF3F).enumerate() {
        let expected = if i % 2 == 0 { 0x00 } else { 0xFF };
        let val = read_cgb_bus(&mut gb, addr);
        assert_eq!(
            val, expected,
            "Production CGB wave RAM at ${:04X} should be ${:02X} (got ${:02X})",
            addr, expected, val
        );
    }
}

/// CGB-0 and Production CGB produce identical IO register state.
///
/// The only difference is wave RAM initialization.
#[test]
fn test_cgb0_and_production_have_same_io_state() {
    let mut gb_0 = boot_cgb_to_cartridge_entry(CgbModel::Cgb0);
    let mut gb_e = boot_cgb_to_cartridge_entry(CgbModel::CgbE);

    // Check APU and PPU registers (should be identical)
    let io_addrs: &[u16] = &[0xFF24, 0xFF25, 0xFF26, 0xFF40, 0xFF47];
    for &addr in io_addrs {
        let val_0 = read_cgb_bus(&mut gb_0, addr);
        let val_e = read_cgb_bus(&mut gb_e, addr);
        assert_eq!(
            val_0, val_e,
            "IO ${:04X}: CGB-0 (${:02X}) vs CGB-E (${:02X})",
            addr, val_0, val_e
        );
    }
}

// ============================================================================
// DMG Compatibility Mode Detection Tests
// ============================================================================

/// Build a CGB test ROM with a specified CGB flag value at $0143.
///
/// The CGB flag determines compatibility mode:
/// - $00: DMG-only
/// - $80: CGB-compatible (also runs on DMG)
/// - $C0: CGB-only
fn build_cgb_test_rom_with_flag(cgb_flag: u8) -> Vec<u8> {
    let mut rom = build_base_test_rom();
    rom[0x0143] = cgb_flag;
    rom[0x014D] = compute_header_checksum(&rom);
    rom
}

/// Boot a CGB with a ROM using the specified CGB flag and return it at PC=$0100.
fn boot_cgb_with_cgb_flag(model: CgbModel, cgb_flag: u8) -> Gb<CgbBus> {
    let rom = build_cgb_test_rom_with_flag(cgb_flag);
    let cart = load_cartridge(&rom).expect("valid ROM");
    let mut gb = Gb::new(CgbBus::new(cart, model, false));
    let reached = run_cgb_until_cartridge_entry(&mut gb);
    assert!(
        reached,
        "Boot ROM must reach $0100 for model {:?} with CGB flag ${:02X}",
        model, cgb_flag
    );
    gb
}

/// DMG-only cartridges ($0143 = $00) must set KEY0 to $04 for DMG compatibility mode.
///
/// Reference: Pan Docs CGB Registers, KEY0 ($FF4C) description:
/// Bit 2 set indicates DMG compatibility mode.
#[test]
fn test_cgb_boot_dmg_only_cartridge_sets_key0_dmg_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0x00);

    // KEY0 ($FF4C): $04 = DMG compatibility mode (bit 2 set)
    // Upper nibble reads as 1s for unused bits, so expect $F4
    let key0 = read_cgb_bus(&mut gb, 0xFF4C);
    assert_eq!(
        key0, 0xF4,
        "KEY0 should be $F4 ($04 with unused bits as 1s) for DMG-only cartridge, got ${:02X}",
        key0
    );
}

/// DMG-only cartridges ($0143 = $00) must set OPRI to $01 for DMG OBJ priority.
///
/// Reference: Pan Docs PPU, OPRI ($FF6C) description:
/// Bit 0 set indicates DMG OBJ priority mode.
#[test]
fn test_cgb_boot_dmg_only_cartridge_sets_opri_dmg_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0x00);

    // OPRI ($FF6C): $01 = DMG OBJ priority mode
    // Upper bits read as 1s, so expect $FF if all unused bits are 1
    let opri = read_cgb_bus(&mut gb, 0xFF6C);
    assert_eq!(
        opri & 0x01,
        0x01,
        "OPRI bit 0 should be set for DMG-only cartridge, got ${:02X}",
        opri
    );
}

/// CGB-compatible cartridges ($0143 = $80) should set KEY0 to the CGB flag value.
///
/// Reference: Pan Docs CGB Registers, KEY0 description.
#[test]
fn test_cgb_boot_cgb_compatible_cartridge_sets_key0_cgb_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0x80);

    // KEY0 ($FF4C): should contain CGB flag value $80
    // Upper nibble reads as 1s, so expect $F0 | (0x80 & 0x0F) = $F0
    let key0 = read_cgb_bus(&mut gb, 0xFF4C);
    assert_eq!(
        key0, 0xF0,
        "KEY0 should be $F0 ($80 with unused bits as 1s) for CGB-compatible, got ${:02X}",
        key0
    );
}

/// CGB-compatible cartridges ($0143 = $80) should keep OPRI at $00 for CGB OBJ priority.
///
/// Reference: Pan Docs PPU, OPRI description.
#[test]
fn test_cgb_boot_cgb_compatible_cartridge_keeps_opri_cgb_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0x80);

    // OPRI ($FF6C): $00 = CGB OBJ priority mode
    let opri = read_cgb_bus(&mut gb, 0xFF6C);
    assert_eq!(
        opri & 0x01,
        0x00,
        "OPRI bit 0 should be clear for CGB-compatible cartridge, got ${:02X}",
        opri
    );
}

/// CGB-only cartridges ($0143 = $C0) should set KEY0 to the CGB flag value.
#[test]
fn test_cgb_boot_cgb_only_cartridge_sets_key0() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0xC0);

    // KEY0 ($FF4C): should contain CGB flag value $C0
    // Upper nibble reads as 1s, so expect $F0 | (0xC0 & 0x0F) = $F0
    let key0 = read_cgb_bus(&mut gb, 0xFF4C);
    assert_eq!(
        key0, 0xF0,
        "KEY0 should be $F0 ($C0 with unused bits as 1s) for CGB-only, got ${:02X}",
        key0
    );
}

/// CGB-only cartridges should keep OPRI at $00 for CGB OBJ priority.
#[test]
fn test_cgb_boot_cgb_only_cartridge_keeps_opri_cgb_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0xC0);

    let opri = read_cgb_bus(&mut gb, 0xFF6C);
    assert_eq!(
        opri & 0x01,
        0x00,
        "OPRI bit 0 should be clear for CGB-only cartridge, got ${:02X}",
        opri
    );
}

/// KEY0 must be locked after boot ROM unmaps.
///
/// After writing to $FF50, the KEY0 register should ignore further writes.
#[test]
fn test_cgb_boot_locks_key0_after_boot() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::CgbE, 0x00);

    // Verify KEY0 is locked
    assert!(
        gb.cpu.bus.is_key0_locked(),
        "KEY0 should be locked after boot ROM unmaps"
    );

    // Try to write a different value to KEY0
    let key0_before = read_cgb_bus(&mut gb, 0xFF4C);
    gb.cpu.bus.write(0xFF4C, 0x00);
    let key0_after = read_cgb_bus(&mut gb, 0xFF4C);

    assert_eq!(
        key0_before, key0_after,
        "KEY0 should not change after boot ROM unmaps (before=${:02X}, after=${:02X})",
        key0_before, key0_after
    );
}

/// CGB-0 boot ROM should detect DMG-only cartridges the same as Production.
#[test]
fn test_cgb0_boot_dmg_only_cartridge_sets_key0_dmg_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::Cgb0, 0x00);

    let key0 = read_cgb_bus(&mut gb, 0xFF4C);
    assert_eq!(
        key0, 0xF4,
        "CGB-0: KEY0 should be $F4 for DMG-only cartridge, got ${:02X}",
        key0
    );

    let opri = read_cgb_bus(&mut gb, 0xFF6C);
    assert_eq!(
        opri & 0x01,
        0x01,
        "CGB-0: OPRI bit 0 should be set for DMG-only cartridge, got ${:02X}",
        opri
    );
}

/// CGB-0 boot ROM should handle CGB-compatible cartridges the same as Production.
#[test]
fn test_cgb0_boot_cgb_compatible_cartridge_sets_key0_cgb_mode() {
    let mut gb = boot_cgb_with_cgb_flag(CgbModel::Cgb0, 0x80);

    let key0 = read_cgb_bus(&mut gb, 0xFF4C);
    assert_eq!(
        key0, 0xF0,
        "CGB-0: KEY0 should be $F0 for CGB-compatible cartridge, got ${:02X}",
        key0
    );

    let opri = read_cgb_bus(&mut gb, 0xFF6C);
    assert_eq!(
        opri & 0x01,
        0x00,
        "CGB-0: OPRI bit 0 should be clear for CGB-compatible cartridge, got ${:02X}",
        opri
    );
}
