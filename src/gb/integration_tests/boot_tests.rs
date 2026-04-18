use crate::gb::bus::DmgBus;
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
