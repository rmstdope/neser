//! Shared in-code LoROM fixture-ROM builder for the input verification
//! suites (#2886/#2889).
//!
//! Builds 64 KiB LoROM images whose 65816 programs are emitted as raw opcode
//! bytes and report results through the `rom_runner` WRAM marker protocol.
//! The program is emitted from CPU address `$8200` upward (the reset vector
//! points there) so the canonical marker idle loops at `$8100`/`$8110`/`$8120`
//! stay clear of program bytes; [`FixtureRom::build`] writes those idle loops.
//!
//! Emitted programs run in the CPU's post-reset emulation mode (8-bit A/X/Y,
//! bank 0), so absolute addresses below `$2000` reach low WRAM and `$4016`/
//! `$4017`/`$42xx` reach the CPU I/O registers.

use super::rom_runner::{
    FAIL_IDLE_PC, MARKER_ADDR, MARKER_MAGIC, PASS_IDLE_PC, PASS_STATUS, TIMEOUT_IDLE_PC,
};

/// CPU address of the first emitted program byte.
pub(crate) const PROGRAM_ORIGIN: u16 = 0x8200;

const ROM_SIZE: usize = 0x1_0000;
const HEADER: usize = 0x7FC0;
const STROBE_PORT: u16 = 0x4016;

pub(crate) struct FixtureRom {
    rom: Vec<u8>,
    cursor: usize,
}

impl FixtureRom {
    /// Creates a 64 KiB LoROM image with `title` in the internal header and
    /// the emulation reset vector pointing at [`PROGRAM_ORIGIN`].
    pub(crate) fn new(title: &[u8]) -> Self {
        assert!(title.len() <= 21, "LoROM header title is at most 21 bytes");
        let mut rom = vec![0u8; ROM_SIZE];
        rom[HEADER..HEADER + 21].fill(b' ');
        rom[HEADER..HEADER + title.len()].copy_from_slice(title);
        rom[HEADER + 0x15] = 0x20; // Map mode: LoROM, slow.
        rom[HEADER + 0x16] = 0x00; // Chipset: ROM only.
        rom[HEADER + 0x17] = 0x07; // ROM size code.
        rom[HEADER + 0x18] = 0x00; // RAM size code.
        rom[HEADER + 0x1C] = 0x34; // Complement check (not validated here).
        rom[HEADER + 0x1D] = 0x12;
        rom[HEADER + 0x1E] = 0xCB; // Checksum (not validated here).
        rom[HEADER + 0x1F] = 0xED;
        rom[HEADER + 0x3C] = (PROGRAM_ORIGIN & 0xFF) as u8; // Reset vector.
        rom[HEADER + 0x3D] = (PROGRAM_ORIGIN >> 8) as u8;

        Self {
            rom,
            cursor: usize::from(PROGRAM_ORIGIN - 0x8000),
        }
    }

    /// Current emit position (file offset), used as a branch target for
    /// [`FixtureRom::bne_to`] poll loops.
    pub(crate) fn pos(&self) -> usize {
        self.cursor
    }

    fn emit(&mut self, bytes: &[u8]) {
        assert!(
            self.cursor + bytes.len() <= HEADER,
            "fixture program overflows into the LoROM header"
        );
        self.rom[self.cursor..self.cursor + bytes.len()].copy_from_slice(bytes);
        self.cursor += bytes.len();
    }

    /// `LDA #value` followed by `STA long addr`.
    pub(crate) fn write_long(&mut self, addr: u32, value: u8) {
        self.emit(&[
            0xA9, // LDA #imm
            value,
            0x8F, // STA long
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            ((addr >> 16) & 0xFF) as u8,
        ]);
    }

    /// `LDA abs addr` (bank 0).
    pub(crate) fn lda_abs(&mut self, addr: u16) {
        self.emit(&[0xAD, (addr & 0xFF) as u8, (addr >> 8) as u8]);
    }

    /// `CMP #value`.
    pub(crate) fn cmp_imm(&mut self, value: u8) {
        self.emit(&[0xC9, value]);
    }

    /// Branches to the file offset returned by an earlier
    /// [`FixtureRom::pos`] call when the Z flag is clear (not equal). Emits
    /// a short `BNE` when the displacement fits, otherwise the long-branch
    /// idiom `BEQ +3; JMP target`.
    pub(crate) fn bne_to(&mut self, target: usize) {
        let after = self.cursor as i64 + 2;
        let rel = target as i64 - after;
        if let Ok(rel) = i8::try_from(rel) {
            self.emit(&[0xD0, rel as u8]); // BNE rel
        } else {
            let addr = 0x8000 + target as u16;
            self.emit(&[0xF0, 0x03]); // BEQ over the JMP
            self.jmp_abs(addr);
        }
    }

    /// `JMP abs addr` (bank 0).
    pub(crate) fn jmp_abs(&mut self, addr: u16) {
        self.emit(&[0x4C, (addr & 0xFF) as u8, (addr >> 8) as u8]);
    }

    /// Pulses the controller strobe: `$4016 <- 1` then `$4016 <- 0`.
    pub(crate) fn strobe_pulse(&mut self) {
        self.emit(&[
            0xA9,
            0x01, // LDA #$01
            0x8D,
            (STROBE_PORT & 0xFF) as u8,
            (STROBE_PORT >> 8) as u8, // STA $4016
            0xA9,
            0x00, // LDA #$00
            0x8D,
            (STROBE_PORT & 0xFF) as u8,
            (STROBE_PORT >> 8) as u8, // STA $4016
        ]);
    }

    /// Serially reads `bits` bits from `joy_addr` (`$4016` or `$4017`) data1
    /// (bit 0), packing them MSB-first into consecutive WRAM bytes starting
    /// at `wram_addr`: the first bit read lands in bit 7 of the first byte.
    /// `bits` must be a multiple of 8.
    pub(crate) fn serial_read_bits(&mut self, joy_addr: u16, bits: usize, wram_addr: u16) {
        assert!(
            bits > 0 && bits.is_multiple_of(8),
            "serial read must be whole bytes"
        );
        for bit in 0..bits {
            let dest = wram_addr + (bit / 8) as u16;
            self.lda_abs(joy_addr);
            self.emit(&[0x4A]); // LSR A: data1 bit -> carry
            self.emit(&[0x2E, (dest & 0xFF) as u8, (dest >> 8) as u8]); // ROL abs
        }
    }

    fn marker_and_idle(&mut self, status: u8, idle_pc: u16) {
        for (offset, byte) in MARKER_MAGIC.iter().copied().enumerate() {
            self.write_long(MARKER_ADDR + offset as u32, byte);
        }
        self.write_long(MARKER_ADDR + 4, status);
        self.jmp_abs(idle_pc);
    }

    /// Writes the `NSER` PASS marker and jumps to the PASS idle loop.
    pub(crate) fn pass_marker_and_idle(&mut self) {
        self.marker_and_idle(PASS_STATUS, PASS_IDLE_PC);
    }

    /// Finalizes the image: writes the marker idle loops at the canonical
    /// PASS/FAIL/TIMEOUT PCs and returns the ROM bytes.
    pub(crate) fn build(mut self) -> Vec<u8> {
        for idle_pc in [PASS_IDLE_PC, FAIL_IDLE_PC, TIMEOUT_IDLE_PC] {
            let offset = usize::from(idle_pc - 0x8000);
            self.rom[offset] = 0x4C; // JMP abs (to itself)
            self.rom[offset + 1] = (idle_pc & 0xFF) as u8;
            self.rom[offset + 2] = (idle_pc >> 8) as u8;
        }
        self.rom
    }
}
