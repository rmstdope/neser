#!/usr/bin/env python3
"""Tests for the SM83 assembler."""

import unittest

from scripts.gb_boot_rom.assembler import (
    assemble,
    assemble_instruction,
    parse_line,
    parse_number,
)


class TestParseNumber(unittest.TestCase):
    def test_hex_dollar(self):
        self.assertEqual(parse_number("$FF"), 0xFF)
        self.assertEqual(parse_number("$1234"), 0x1234)

    def test_hex_0x(self):
        self.assertEqual(parse_number("0xFF"), 0xFF)
        self.assertEqual(parse_number("0x1234"), 0x1234)

    def test_binary(self):
        self.assertEqual(parse_number("%11111111"), 0xFF)
        self.assertEqual(parse_number("0b11111111"), 0xFF)

    def test_decimal(self):
        self.assertEqual(parse_number("255"), 255)
        self.assertEqual(parse_number("0"), 0)


class TestParseLine(unittest.TestCase):
    def test_empty_line(self):
        pl = parse_line("", 1)
        self.assertIsNone(pl.instruction)

    def test_comment_only(self):
        pl = parse_line("; this is a comment", 1)
        self.assertIsNone(pl.instruction)
        self.assertEqual(pl.comment, "this is a comment")

    def test_simple_instruction(self):
        pl = parse_line("NOP", 1)
        self.assertEqual(pl.instruction, "NOP")
        self.assertEqual(pl.operands, [])

    def test_instruction_with_operand(self):
        pl = parse_line("LD A, $80", 1)
        self.assertEqual(pl.instruction, "LD")
        self.assertEqual(pl.operands, ["A", "$80"])

    def test_instruction_with_comment(self):
        pl = parse_line("LD A, $80  ; load 128", 1)
        self.assertEqual(pl.instruction, "LD")
        self.assertEqual(pl.operands, ["A", "$80"])
        self.assertEqual(pl.comment, "load 128")

    def test_label(self):
        pl = parse_line("Start:", 1)
        self.assertEqual(pl.label, "Start")
        self.assertIsNone(pl.instruction)

    def test_label_with_instruction(self):
        pl = parse_line("Loop: DEC C", 1)
        self.assertEqual(pl.label, "Loop")
        self.assertEqual(pl.instruction, "DEC")
        self.assertEqual(pl.operands, ["C"])

    def test_address_prefix(self):
        pl = parse_line("$00FE: LDH [$50], A", 1)
        self.assertEqual(pl.address, 0x00FE)
        self.assertEqual(pl.instruction, "LDH")


class TestAssembleInstruction(unittest.TestCase):
    def test_nop(self):
        bytes_out, _ = assemble_instruction("NOP", [], {}, 0)
        self.assertEqual(bytes_out, [0x00])

    def test_ld_sp_nn(self):
        bytes_out, _ = assemble_instruction("LD", ["SP", "$FFFE"], {}, 0)
        self.assertEqual(bytes_out, [0x31, 0xFE, 0xFF])

    def test_ld_a_n(self):
        bytes_out, _ = assemble_instruction("LD", ["A", "$80"], {}, 0)
        self.assertEqual(bytes_out, [0x3E, 0x80])

    def test_ldh_addr_a(self):
        bytes_out, _ = assemble_instruction("LDH", ["[$26]", "A"], {}, 0)
        self.assertEqual(bytes_out, [0xE0, 0x26])

    def test_xor_a(self):
        bytes_out, _ = assemble_instruction("XOR", ["A"], {}, 0)
        self.assertEqual(bytes_out, [0xAF])

    def test_jr_nz_relative(self):
        # JR NZ to address 3, from address 0
        # JR NZ, n where n = target - (current + 2) = 3 - 2 = 1
        bytes_out, _ = assemble_instruction("JR", ["NZ", "3"], {}, 0)
        self.assertEqual(bytes_out, [0x20, 0x01])

    def test_db(self):
        bytes_out, _ = assemble_instruction("DB", ["$00", "$FF"], {}, 0)
        self.assertEqual(bytes_out, [0x00, 0xFF])

    def test_dw(self):
        bytes_out, _ = assemble_instruction("DW", ["$1234"], {}, 0)
        self.assertEqual(bytes_out, [0x34, 0x12])  # Little-endian


class TestAssemble(unittest.TestCase):
    def test_simple_program(self):
        source = """
        LD SP, $FFFE
        LD A, $80
        LDH [$26], A
        """
        assembled = assemble(source)
        self.assertEqual(len(assembled), 3)
        self.assertEqual(assembled[0].bytes, [0x31, 0xFE, 0xFF])
        self.assertEqual(assembled[1].bytes, [0x3E, 0x80])
        self.assertEqual(assembled[2].bytes, [0xE0, 0x26])

    def test_with_label(self):
        source = """
        $0000: Start:
            LD C, $10
        Loop:
            DEC C
            JR NZ, Loop
        """
        assembled = assemble(source)
        self.assertEqual(len(assembled), 3)
        # Address layout:
        # $0000: LD C, $10 (2 bytes)
        # $0002: DEC C (1 byte) - Loop label here
        # $0003: JR NZ, Loop (2 bytes)
        # JR NZ from $0003 to $0002:
        # Offset = $0002 - ($0003 + 2) = $0002 - $0005 = -3 = 0xFD
        self.assertEqual(assembled[2].bytes, [0x20, 0xFD])


if __name__ == "__main__":
    unittest.main()
