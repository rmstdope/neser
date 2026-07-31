//! Shared in-code LoROM fixture-ROM builder for the input verification
//! suites (#2886/#2889) and the custom DMA/HDMA fixture suites (#2884).
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
    FAIL_IDLE_PC, FAIL_STATUS, MARKER_ADDR, MARKER_MAGIC, PASS_IDLE_PC, PASS_STATUS,
    TIMEOUT_IDLE_PC,
};

/// CPU address of the first emitted program byte.
pub(crate) const PROGRAM_ORIGIN: u16 = 0x8200;

const ROM_SIZE: usize = 0x1_0000;
const HEADER: usize = 0x7FC0;
const STROBE_PORT: u16 = 0x4016;

pub(crate) struct FixtureRom {
    rom: Vec<u8>,
    cursor: usize,
    data_cursor: usize,
}

/// File offset (== CPU `$8000`) of the read-only data region that
/// [`FixtureRom::place_data`] fills upward. Kept clear of the program (which
/// starts at [`PROGRAM_ORIGIN`]) and the marker idle loops at `$8100`+.
const DATA_REGION_START: usize = 0x0000;
const DATA_REGION_END: usize = 0x0100;

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
            data_cursor: DATA_REGION_START,
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

    /// `LDA #value`.
    pub(crate) fn lda_imm(&mut self, value: u8) {
        self.emit(&[0xA9, value]);
    }

    /// `LDA abs addr` (bank 0).
    pub(crate) fn lda_abs(&mut self, addr: u16) {
        self.emit(&[0xAD, (addr & 0xFF) as u8, (addr >> 8) as u8]);
    }

    /// `STA abs addr` (bank 0) — stores the 8-bit accumulator.
    pub(crate) fn sta_abs(&mut self, addr: u16) {
        self.emit(&[0x8D, (addr & 0xFF) as u8, (addr >> 8) as u8]);
    }

    /// `LDA #value` followed by `STA abs addr` (bank 0). Reaches PPU
    /// (`$2100-$21FF`), CPU I/O (`$4200-$420C`) and DMA (`$4300-$437F`)
    /// registers, all of which live in bank 0.
    pub(crate) fn store_imm_abs(&mut self, addr: u16, value: u8) {
        self.lda_imm(value);
        self.sta_abs(addr);
    }

    /// Copies `bytes` into the read-only data region and returns the CPU
    /// address of the first byte (usable as a DMA A-bus source in bank 0).
    pub(crate) fn place_data(&mut self, bytes: &[u8]) -> u16 {
        assert!(
            self.data_cursor + bytes.len() <= DATA_REGION_END,
            "fixture data overflows the {}-byte data region",
            DATA_REGION_END - DATA_REGION_START
        );
        let addr = 0x8000 + self.data_cursor as u16;
        self.rom[self.data_cursor..self.data_cursor + bytes.len()].copy_from_slice(bytes);
        self.data_cursor += bytes.len();
        addr
    }

    /// `CMP #value`.
    pub(crate) fn cmp_imm(&mut self, value: u8) {
        self.emit(&[0xC9, value]);
    }

    /// `AND #value`.
    pub(crate) fn and_imm(&mut self, value: u8) {
        self.emit(&[0x29, value]);
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

    /// Branches to the file offset returned by an earlier [`FixtureRom::pos`]
    /// call when the Z flag is set (equal). Emits a short `BEQ` when the
    /// displacement fits, otherwise `BNE +3; JMP target`.
    pub(crate) fn beq_to(&mut self, target: usize) {
        let after = self.cursor as i64 + 2;
        let rel = target as i64 - after;
        if let Ok(rel) = i8::try_from(rel) {
            self.emit(&[0xF0, rel as u8]); // BEQ rel
        } else {
            let addr = 0x8000 + target as u16;
            self.emit(&[0xD0, 0x03]); // BNE over the JMP
            self.jmp_abs(addr);
        }
    }

    /// `JMP abs addr` (bank 0).
    pub(crate) fn jmp_abs(&mut self, addr: u16) {
        self.emit(&[0x4C, (addr & 0xFF) as u8, (addr >> 8) as u8]);
    }

    /// `LSR A` — shifts bit 0 of the accumulator into the carry flag.
    pub(crate) fn lsr_a(&mut self) {
        self.emit(&[0x4A]);
    }

    /// Branches to `target` while the carry flag is clear. Emits a short
    /// `BCC` when the displacement fits, otherwise `BCS +3; JMP target`.
    pub(crate) fn bcc_to(&mut self, target: usize) {
        self.carry_branch_to(target, 0x90, 0xB0);
    }

    /// Branches to `target` while the carry flag is set. Emits a short
    /// `BCS` when the displacement fits, otherwise `BCC +3; JMP target`.
    pub(crate) fn bcs_to(&mut self, target: usize) {
        self.carry_branch_to(target, 0xB0, 0x90);
    }

    fn carry_branch_to(&mut self, target: usize, opcode: u8, inverse_opcode: u8) {
        let after = self.cursor as i64 + 2;
        let rel = target as i64 - after;
        if let Ok(rel) = i8::try_from(rel) {
            self.emit(&[opcode, rel as u8]);
        } else {
            let addr = 0x8000 + target as u16;
            self.emit(&[inverse_opcode, 0x03]); // skip over the JMP
            self.jmp_abs(addr);
        }
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
    /// `bits` must be a multiple of 8, so every destination byte receives
    /// exactly eight `ROL`s and is fully overwritten by the bits read —
    /// prior WRAM contents never survive, and the scratch bytes can be
    /// reused freely across poll iterations.
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

    /// `INIDISP ($2100) <- $8F`: force blank on at full brightness, so
    /// VRAM/CGRAM/OAM are freely accessible by CPU and DMA.
    pub(crate) fn force_blank_on(&mut self) {
        self.store_imm_abs(0x2100, 0x8F);
    }

    /// `INIDISP ($2100) <- $0F`: release force blank at full brightness.
    pub(crate) fn force_blank_off(&mut self) {
        self.store_imm_abs(0x2100, 0x0F);
    }

    /// Programs general-purpose DMA channel `channel`'s registers:
    /// DMAP (`$43x0`), BBAD B-bus address (`$43x1`), the 24-bit A-bus source
    /// `src` (`$43x2..4`, bank taken from bits 16-23), and the 16-bit byte
    /// count `count` (`$43x5/6`). Does not trigger the transfer.
    pub(crate) fn setup_gpdma(&mut self, channel: u8, dmap: u8, bbad: u8, src: u32, count: u16) {
        let base = 0x4300 + u16::from(channel) * 0x10;
        self.store_imm_abs(base, dmap);
        self.store_imm_abs(base + 1, bbad);
        self.store_imm_abs(base + 2, (src & 0xFF) as u8);
        self.store_imm_abs(base + 3, ((src >> 8) & 0xFF) as u8);
        self.store_imm_abs(base + 4, ((src >> 16) & 0xFF) as u8);
        self.store_imm_abs(base + 5, (count & 0xFF) as u8);
        self.store_imm_abs(base + 6, (count >> 8) as u8);
    }

    /// `MDMAEN ($420B) <- mask`: starts general-purpose DMA on the selected
    /// channels (bit N = channel N). The CPU is paused until it completes.
    pub(crate) fn trigger_gpdma(&mut self, mask: u8) {
        self.store_imm_abs(0x420B, mask);
    }

    /// Programs HDMA channel `channel`: DMAP (`$43x0`), BBAD (`$43x1`), the
    /// 16-bit table address in bank 0 (`$43x2/3`, `$43x4`=0), and DASB
    /// (`$43x7`, the indirect-data bank used by indirect-mode entries). Does
    /// not enable the channel.
    pub(crate) fn setup_hdma(
        &mut self,
        channel: u8,
        dmap: u8,
        bbad: u8,
        table_addr: u16,
        indirect_bank: u8,
    ) {
        let base = 0x4300 + u16::from(channel) * 0x10;
        self.store_imm_abs(base, dmap);
        self.store_imm_abs(base + 1, bbad);
        self.store_imm_abs(base + 2, (table_addr & 0xFF) as u8);
        self.store_imm_abs(base + 3, (table_addr >> 8) as u8);
        self.store_imm_abs(base + 4, 0x00);
        self.store_imm_abs(base + 7, indirect_bank);
    }

    /// `HDMAEN ($420C) <- mask`: enables HDMA on the selected channels. Init
    /// runs at the top of the next frame; per-line transfers follow.
    pub(crate) fn enable_hdma(&mut self, mask: u8) {
        self.store_imm_abs(0x420C, mask);
    }

    /// `HDMAEN ($420C) <- 0`: disables all HDMA channels.
    pub(crate) fn disable_hdma(&mut self) {
        self.store_imm_abs(0x420C, 0x00);
    }

    /// Compares the accumulator against `expected` and, if they differ,
    /// writes the `NSER` FAIL marker and parks at the FAIL idle PC. The whole
    /// fail block is emitted inline and skipped by a `BEQ` when the values
    /// match, so a fixture can chain many readback assertions and only reach
    /// [`FixtureRom::pass_marker_and_idle`] if every one held.
    pub(crate) fn branch_fail_if_ne(&mut self, expected: u8) {
        self.cmp_imm(expected);
        let branch_operand = self.cursor + 1;
        self.emit(&[0xF0, 0x00]); // BEQ (operand patched below)
        let block_start = self.cursor;
        self.marker_and_idle(FAIL_STATUS, FAIL_IDLE_PC);
        let skip = self.cursor - block_start;
        self.rom[branch_operand] =
            i8::try_from(skip).expect("fail block too large for a short BEQ") as u8;
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
