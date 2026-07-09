//! Black-box coverage for issue #2960 (SA-1 cross-CPU IRQ and status registers): a hand-built
//! SA-1-chipset ROM that exercises the same SA-1-raises-IRQ / main-CPU-responds handshake
//! `SA1RamProtectionTest.sfc`'s `SendSA1MessageAcc` macro uses (see
//! `TestRoutines.asm`), simplified to a single round-trip:
//!
//! 1. The main CPU boots, enables `$2201` SIE (IRQ-from-SA-1), releases SA-1 from reset, and
//!    idles with interrupts unmasked (`CLI`).
//! 2. SA-1 boots and raises an IRQ via `$2209` SCNT, embedding a 4-bit message.
//! 3. The main CPU's real (fixed, emulation-mode) IRQ handler fires, reads the message back via
//!    `$2300` SFR, stashes it into WRAM, acknowledges via `$2202` SIC, and writes a response
//!    message into `$2200` CCNT.
//! 4. SA-1, busy-polling `$2301` CFR (exactly like the real ROM), observes the response and
//!    idles.

use crate::platform::app_context::AppContext;
use crate::platform::emulator::Emulator;
use crate::snes::console::Snes;

const HEADER: usize = 0x7FC0;

fn write_lorom_header(rom: &mut [u8], title: &[u8], chipset: u8) {
    rom[HEADER..HEADER + 21].fill(b' ');
    rom[HEADER..HEADER + title.len()].copy_from_slice(title);
    rom[HEADER + 0x15] = 0x20; // Map mode: LoROM, slow.
    rom[HEADER + 0x16] = chipset;
    rom[HEADER + 0x17] = 0x07; // ROM size.
    rom[HEADER + 0x18] = 0x00; // RAM size.
    rom[HEADER + 0x1C] = 0x34; // Complement check (not validated by this codebase).
    rom[HEADER + 0x1D] = 0x12;
    rom[HEADER + 0x1E] = 0xCB; // Checksum (not validated by this codebase).
    rom[HEADER + 0x1F] = 0xED;
    rom[HEADER + 0x3C] = 0x00; // Main CPU reset vector low byte.
    rom[HEADER + 0x3D] = 0x80; // Main CPU reset vector high byte -> $8000.
    rom[HEADER + 0x3E] = 0x00; // Main CPU emulation-mode IRQ/BRK vector low byte.
    rom[HEADER + 0x3F] = 0x81; // ... high byte -> $8100 (the CPU never switches to native mode).
}

/// Builds a 64 KiB LoROM SA-1 ROM whose main-CPU program at bank `$00:$8000` enables `$2201`
/// SIE, releases SA-1, unmasks interrupts, and idles; whose fixed IRQ handler at `$00:$8100`
/// reads `$2300` SFR, stashes the message at WRAM `$0000`, acknowledges via `$2202` SIC, and
/// replies via `$2200` CCNT; and whose SA-1-side program at `$00:$9000` raises an IRQ with
/// message `$7` via `$2209` SCNT, then busy-polls `$2301` CFR for the message-`$5` reply.
fn build_sa1_irq_roundtrip_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x1_0000];
    write_lorom_header(&mut rom, b"SA1 IRQ TEST", 0x35);

    // Main CPU boot, bank $00:$8000 (file offset $0000).
    #[rustfmt::skip]
    let main_boot: [u8; 24] = [
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x03, 0x22, // STA $2203 (CRVL)
        0xA9, 0x90,       // LDA #$90
        0x8D, 0x04, 0x22, // STA $2204 (CRVH) -> SA-1 reset vector = $9000
        0xA9, 0x80,       // LDA #$80
        0x8D, 0x01, 0x22, // STA $2201 (SIE) -> enable IRQ-from-SA-1
        0xA9, 0x00,       // LDA #$00
        0x8D, 0x00, 0x22, // STA $2200 (CCNT) -> release SA-1 from reset
        0x58,             // CLI
        0x4C, 0x15, 0x80, // JMP $8015 (idle loop)
    ];
    rom[0x0000..main_boot.len()].copy_from_slice(&main_boot);

    // Main CPU IRQ handler, bank $00:$8100 (file offset $0100).
    #[rustfmt::skip]
    let main_irq_handler: [u8; 19] = [
        0xAD, 0x00, 0x23, // LDA $2300 (SFR)
        0x29, 0x0F,       // AND #$0F
        0x8D, 0x00, 0x00, // STA $0000 (stash the message)
        0xA9, 0x80,       // LDA #$80
        0x8D, 0x02, 0x22, // STA $2202 (SIC) -> acknowledge
        0xA9, 0x05,       // LDA #$05
        0x8D, 0x00, 0x22, // STA $2200 (CCNT) -> reply with message $5
        0x40,             // RTI
    ];
    rom[0x0100..0x0100 + main_irq_handler.len()].copy_from_slice(&main_irq_handler);

    // SA-1 CPU program, bank $00:$9000 (file offset $1000).
    #[rustfmt::skip]
    let sa1_program: [u8; 17] = [
        0xA9, 0x87,       // LDA #$87
        0x8D, 0x09, 0x22, // STA $2209 (SCNT) -> raise IRQ, message = $7
        0xAD, 0x01, 0x23, // LDA $2301 (CFR)
        0x29, 0x0F,       // AND #$0F
        0xC9, 0x05,       // CMP #$05
        0xD0, 0xF7,       // BNE -9 (poll)
        0x4C, 0x0E, 0x90, // JMP $900E (idle loop)
    ];
    rom[0x1000..0x1000 + sa1_program.len()].copy_from_slice(&sa1_program);

    rom
}

#[test]
fn sa1_irq_round_trips_a_message_through_the_main_cpus_real_irq_handler() {
    let rom = build_sa1_irq_roundtrip_rom();
    let mut snes = Snes::new(AppContext::default());
    snes.load_rom(&rom, "sa1-irq-roundtrip-test.sfc")
        .expect("failed to load SA-1 IRQ roundtrip fixture ROM");

    for _ in 0..6000 {
        snes.run_tick();
    }

    assert_eq!(
        snes.read_bus_for_debugger_for_tests(0x00_0000),
        Some(0x07),
        "main CPU's real IRQ handler should have read SA-1's message via SFR"
    );
    assert_eq!(
        snes.sa1_cpu_pc_for_tests(),
        Some(0x900E),
        "SA-1 should have observed the $5 reply via CFR and reached its idle loop"
    );
}
