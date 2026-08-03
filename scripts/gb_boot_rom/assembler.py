#!/usr/bin/env python3
"""
Simple SM83 assembler for CGB boot ROM generation.

This tool converts an assembly-like source file to inline Rust byte array
assignments. The output can be copy-pasted into boot_rom.rs.

Usage:
    python assembler.py cgb_boot.asm > output.rs
"""

import re
import sys
from dataclasses import dataclass

# SM83 instruction encoding
INSTRUCTIONS = {
    # 8-bit loads
    "LD A, n": (0x3E, 2),
    "LD B, n": (0x06, 2),
    "LD C, n": (0x0E, 2),
    "LD D, n": (0x16, 2),
    "LD E, n": (0x1E, 2),
    "LD H, n": (0x26, 2),
    "LD L, n": (0x2E, 2),
    "LD [HL], A": (0x77, 1),
    "LD A, [HL]": (0x7E, 1),
    "LDI [HL], A": (0x22, 1),
    "LDD [HL], A": (0x32, 1),
    "LDI A, [HL]": (0x2A, 1),
    "LDD A, [HL]": (0x3A, 1),
    "LD [DE], A": (0x12, 1),
    "LD A, [DE]": (0x1A, 1),
    "LD A, [nn]": (0xFA, 3),
    "LD [nn], A": (0xEA, 3),
    "LDH [n], A": (0xE0, 2),
    "LDH A, [n]": (0xF0, 2),
    "LD [C], A": (0xE2, 1),
    "LD A, [C]": (0xF2, 1),
    # 16-bit loads
    "LD SP, nn": (0x31, 3),
    "LD HL, nn": (0x21, 3),
    "LD DE, nn": (0x11, 3),
    "LD BC, nn": (0x01, 3),
    # Stack
    "PUSH AF": (0xF5, 1),
    "PUSH BC": (0xC5, 1),
    "PUSH DE": (0xD5, 1),
    "PUSH HL": (0xE5, 1),
    "POP AF": (0xF1, 1),
    "POP BC": (0xC1, 1),
    "POP DE": (0xD1, 1),
    "POP HL": (0xE1, 1),
    # ALU
    "XOR A": (0xAF, 1),
    "XOR B": (0xA8, 1),
    "XOR C": (0xA9, 1),
    "XOR D": (0xAA, 1),
    "XOR E": (0xAB, 1),
    "XOR H": (0xAC, 1),
    "XOR L": (0xAD, 1),
    "XOR n": (0xEE, 2),
    "OR A": (0xB7, 1),
    "OR B": (0xB0, 1),
    "OR C": (0xB1, 1),
    "OR D": (0xB2, 1),
    "OR E": (0xB3, 1),
    "OR H": (0xB4, 1),
    "OR L": (0xB5, 1),
    "OR n": (0xF6, 2),
    "AND A": (0xA7, 1),
    "AND B": (0xA0, 1),
    "AND C": (0xA1, 1),
    "AND D": (0xA2, 1),
    "AND E": (0xA3, 1),
    "AND H": (0xA4, 1),
    "AND L": (0xA5, 1),
    "AND n": (0xE6, 2),
    "ADD A, A": (0x87, 1),
    "ADD A, B": (0x80, 1),
    "ADD A, C": (0x81, 1),
    "ADD A, D": (0x82, 1),
    "ADD A, E": (0x83, 1),
    "ADD A, H": (0x84, 1),
    "ADD A, L": (0x85, 1),
    "ADD A, [HL]": (0x86, 1),
    "ADD A, n": (0xC6, 2),
    "ADC A, n": (0xCE, 2),
    "SUB A, A": (0x97, 1),
    "SUB A, B": (0x90, 1),
    "SUB A, C": (0x91, 1),
    "SUB A, D": (0x92, 1),
    "SUB A, E": (0x93, 1),
    "SUB A, H": (0x94, 1),
    "SUB A, L": (0x95, 1),
    "SUB n": (0xD6, 2),
    "SBC A, n": (0xDE, 2),
    "CP A": (0xBF, 1),
    "CP B": (0xB8, 1),
    "CP C": (0xB9, 1),
    "CP D": (0xBA, 1),
    "CP E": (0xBB, 1),
    "CP H": (0xBC, 1),
    "CP L": (0xBD, 1),
    "CP [HL]": (0xBE, 1),
    "CP n": (0xFE, 2),
    "INC A": (0x3C, 1),
    "INC B": (0x04, 1),
    "INC C": (0x0C, 1),
    "INC D": (0x14, 1),
    "INC E": (0x1C, 1),
    "INC H": (0x24, 1),
    "INC L": (0x2C, 1),
    "INC [HL]": (0x34, 1),
    "DEC A": (0x3D, 1),
    "DEC B": (0x05, 1),
    "DEC C": (0x0D, 1),
    "DEC D": (0x15, 1),
    "DEC E": (0x1D, 1),
    "DEC H": (0x25, 1),
    "DEC L": (0x2D, 1),
    "DEC [HL]": (0x35, 1),
    "INC HL": (0x23, 1),
    "INC DE": (0x13, 1),
    "INC BC": (0x03, 1),
    "DEC HL": (0x2B, 1),
    "DEC DE": (0x1B, 1),
    "DEC BC": (0x0B, 1),
    "ADD HL, BC": (0x09, 1),
    "ADD HL, DE": (0x19, 1),
    "ADD HL, HL": (0x29, 1),
    "CPL": (0x2F, 1),
    "SCF": (0x37, 1),
    "CCF": (0x3F, 1),
    "DAA": (0x27, 1),
    # Rotates/shifts
    "RLCA": (0x07, 1),
    "RLA": (0x17, 1),
    "RRCA": (0x0F, 1),
    "RRA": (0x1F, 1),
    "RLC A": (0xCB07, 2),
    "RLC B": (0xCB00, 2),
    "RLC C": (0xCB01, 2),
    "RLC D": (0xCB02, 2),
    "RLC E": (0xCB03, 2),
    "RLC H": (0xCB04, 2),
    "RLC L": (0xCB05, 2),
    "RL A": (0xCB17, 2),
    "RL B": (0xCB10, 2),
    "RL C": (0xCB11, 2),
    "RL D": (0xCB12, 2),
    "RL E": (0xCB13, 2),
    "RL H": (0xCB14, 2),
    "RL L": (0xCB15, 2),
    "RRC A": (0xCB0F, 2),
    "RR A": (0xCB1F, 2),
    "RR B": (0xCB18, 2),
    "RR C": (0xCB19, 2),
    "SLA A": (0xCB27, 2),
    "SLA B": (0xCB20, 2),
    "SLA C": (0xCB21, 2),
    "SRA A": (0xCB2F, 2),
    "SRL A": (0xCB3F, 2),
    "SWAP A": (0xCB37, 2),
    "SWAP C": (0xCB31, 2),
    # Bit operations
    "BIT 0, A": (0xCB47, 2),
    "BIT 0, B": (0xCB40, 2),
    "BIT 0, C": (0xCB41, 2),
    "BIT 0, [HL]": (0xCB46, 2),
    "BIT 1, A": (0xCB4F, 2),
    "BIT 1, B": (0xCB48, 2),
    "BIT 2, A": (0xCB57, 2),
    "BIT 2, B": (0xCB50, 2),
    "BIT 3, A": (0xCB5F, 2),
    "BIT 4, A": (0xCB67, 2),
    "BIT 5, A": (0xCB6F, 2),
    "BIT 5, H": (0xCB6C, 2),
    "BIT 6, A": (0xCB77, 2),
    "BIT 7, A": (0xCB7F, 2),
    "BIT 7, [HL]": (0xCB7E, 2),
    "RES 0, A": (0xCB87, 2),
    "RES 0, [HL]": (0xCB86, 2),
    "RES 2, A": (0xCB97, 2),
    "RES 2, C": (0xCB91, 2),
    "RES 5, C": (0xCBA9, 2),
    "SET 0, [HL]": (0xCBC6, 2),
    "SET 5, [HL]": (0xCBEE, 2),
    # Jumps
    "JP nn": (0xC3, 3),
    "JP HL": (0xE9, 1),
    "JP NZ, nn": (0xC2, 3),
    "JP Z, nn": (0xCA, 3),
    "JP NC, nn": (0xD2, 3),
    "JP C, nn": (0xDA, 3),
    "JR n": (0x18, 2),
    "JR NZ, n": (0x20, 2),
    "JR Z, n": (0x28, 2),
    "JR NC, n": (0x30, 2),
    "JR C, n": (0x38, 2),
    "CALL nn": (0xCD, 3),
    "CALL NZ, nn": (0xC4, 3),
    "CALL Z, nn": (0xCC, 3),
    "CALL NC, nn": (0xD4, 3),
    "CALL C, nn": (0xDC, 3),
    "RET": (0xC9, 1),
    "RET NZ": (0xC0, 1),
    "RET Z": (0xC8, 1),
    "RET NC": (0xD0, 1),
    "RET C": (0xD8, 1),
    "RETI": (0xD9, 1),
    "RST $00": (0xC7, 1),
    "RST $08": (0xCF, 1),
    "RST $10": (0xD7, 1),
    "RST $18": (0xDF, 1),
    "RST $20": (0xE7, 1),
    "RST $28": (0xEF, 1),
    "RST $30": (0xF7, 1),
    "RST $38": (0xFF, 1),
    # Misc
    "NOP": (0x00, 1),
    "HALT": (0x76, 1),
    "DI": (0xF3, 1),
    "EI": (0xFB, 1),
    "STOP": (0x1000, 2),
    # Inter-register moves
    "LD A, A": (0x7F, 1),
    "LD A, B": (0x78, 1),
    "LD A, C": (0x79, 1),
    "LD A, D": (0x7A, 1),
    "LD A, E": (0x7B, 1),
    "LD A, H": (0x7C, 1),
    "LD A, L": (0x7D, 1),
    "LD B, A": (0x47, 1),
    "LD B, B": (0x40, 1),
    "LD B, C": (0x41, 1),
    "LD B, D": (0x42, 1),
    "LD B, E": (0x43, 1),
    "LD B, H": (0x44, 1),
    "LD B, L": (0x45, 1),
    "LD B, [HL]": (0x46, 1),
    "LD C, A": (0x4F, 1),
    "LD C, B": (0x48, 1),
    "LD C, C": (0x49, 1),
    "LD C, D": (0x4A, 1),
    "LD C, E": (0x4B, 1),
    "LD C, H": (0x4C, 1),
    "LD C, L": (0x4D, 1),
    "LD C, [HL]": (0x4E, 1),
    "LD D, A": (0x57, 1),
    "LD D, B": (0x50, 1),
    "LD D, C": (0x51, 1),
    "LD D, D": (0x52, 1),
    "LD D, E": (0x53, 1),
    "LD D, H": (0x54, 1),
    "LD D, L": (0x55, 1),
    "LD D, [HL]": (0x56, 1),
    "LD E, A": (0x5F, 1),
    "LD E, B": (0x58, 1),
    "LD E, C": (0x59, 1),
    "LD E, D": (0x5A, 1),
    "LD E, E": (0x5B, 1),
    "LD E, H": (0x5C, 1),
    "LD E, L": (0x5D, 1),
    "LD E, [HL]": (0x5E, 1),
    "LD H, A": (0x67, 1),
    "LD H, B": (0x60, 1),
    "LD H, C": (0x61, 1),
    "LD H, D": (0x62, 1),
    "LD H, E": (0x63, 1),
    "LD H, H": (0x64, 1),
    "LD H, L": (0x65, 1),
    "LD H, [HL]": (0x66, 1),
    "LD L, A": (0x6F, 1),
    "LD L, B": (0x68, 1),
    "LD L, C": (0x69, 1),
    "LD L, D": (0x6A, 1),
    "LD L, E": (0x6B, 1),
    "LD L, H": (0x6C, 1),
    "LD L, L": (0x6D, 1),
    "LD L, [HL]": (0x6E, 1),
    "LD [HL], B": (0x70, 1),
    "LD [HL], C": (0x71, 1),
    "LD [HL], D": (0x72, 1),
    "LD [HL], E": (0x73, 1),
    "LD [HL], H": (0x74, 1),
    "LD [HL], L": (0x75, 1),
    "LD [HL], n": (0x36, 2),
    "LD SP, HL": (0xF9, 1),
    "LD HL, SP+n": (0xF8, 2),
    "LD [nn], SP": (0x08, 3),
}


@dataclass
class AssemblyLine:
    """Represents a parsed assembly line."""

    address: int | None
    label: str | None
    instruction: str | None
    operands: list[str]
    comment: str | None
    raw: str
    line_num: int


@dataclass
class Assembled:
    """Represents assembled bytes at an address."""

    address: int
    bytes: list[int]
    source: str
    comment: str


def parse_number(s: str) -> int:
    """Parse a number from various formats."""
    s = s.strip()
    if s.startswith("$"):
        return int(s[1:], 16)
    elif s.startswith("0x"):
        return int(s[2:], 16)
    elif s.startswith("%"):
        return int(s[1:], 2)
    elif s.startswith("0b"):
        return int(s[2:], 2)
    else:
        return int(s)


def parse_line(line: str, line_num: int) -> AssemblyLine:
    """Parse a single assembly line."""
    raw = line
    comment = None

    # Extract comment
    if ";" in line:
        idx = line.index(";")
        comment = line[idx + 1 :].strip()
        line = line[:idx]

    line = line.strip()

    # Extract address (if prefixed with $xxxx:)
    address = None
    if re.match(r"^\$[0-9A-Fa-f]+:", line):
        m = re.match(r"^\$([0-9A-Fa-f]+):", line)
        address = int(m.group(1), 16)
        line = line[m.end() :].strip()

    # Extract label
    label = None
    if ":" in line and not line.startswith("["):
        idx = line.index(":")
        potential_label = line[:idx].strip()
        if re.match(r"^[a-zA-Z_\.][a-zA-Z0-9_\.]*$", potential_label):
            label = potential_label
            line = line[idx + 1 :].strip()

    # Empty line or just label
    if not line:
        return AssemblyLine(address, label, None, [], comment, raw, line_num)

    # Parse instruction and operands
    parts = re.split(r"[\s,]+", line, maxsplit=1)
    instruction = parts[0].upper()
    operands = []
    if len(parts) > 1:
        # Split operands by comma, but preserve brackets
        operand_str = parts[1]
        operands = [op.strip() for op in re.split(r",\s*", operand_str)]

    return AssemblyLine(address, label, instruction, operands, comment, raw, line_num)


def assemble_instruction(
    instr: str, operands: list[str], labels: dict[str, int], current_addr: int
) -> tuple[list[int], str]:
    """Assemble a single instruction to bytes."""
    # Handle DB (define byte)
    if instr == "DB":
        result = []
        for op in operands:
            if op.startswith('"') and op.endswith('"'):
                # String literal
                for char in op[1:-1]:
                    result.append(ord(char))
            else:
                result.append(parse_number(op) & 0xFF)
        return result, f"DB {', '.join(operands)}"

    # Handle DW (define word)
    if instr == "DW":
        result = []
        for op in operands:
            val = parse_number(op) if op not in labels else labels[op]
            result.append(val & 0xFF)
            result.append((val >> 8) & 0xFF)
        return result, f"DW {', '.join(operands)}"

    # Handle DS (define space)
    if instr == "DS":
        count = parse_number(operands[0])
        fill = parse_number(operands[1]) if len(operands) > 1 else 0
        return [fill] * count, f"DS {count}"

    # Normalize instruction format
    is_jr = instr.startswith("JR")
    is_ldh = instr == "LDH"

    if operands:
        # Replace operand values with placeholders
        norm_ops = []
        for op in operands:
            op_upper = op.upper()
            registers = ["A", "B", "C", "D", "E", "H", "L", "AF", "BC", "DE", "HL", "SP"]
            if op_upper in registers or op_upper in ["NZ", "Z", "NC"]:
                norm_ops.append(op_upper)
            elif op_upper.startswith("[") and op_upper.endswith("]"):
                inner = op_upper[1:-1]
                if inner in ["HL", "DE", "BC", "C"]:
                    norm_ops.append(f"[{inner}]")
                elif is_ldh:
                    # LDH uses 8-bit high-memory offset
                    norm_ops.append("[n]")
                elif inner.startswith("$") or inner.startswith("0X") or inner.isdigit():
                    norm_ops.append("[nn]")
                else:
                    norm_ops.append("[n]")
            elif op_upper.startswith("SP+"):
                norm_ops.append("SP+n")
            else:
                # For JR instructions, always use 'n' (relative offset)
                if is_jr:
                    norm_ops.append("n")
                # Check if it's a label
                elif op in labels or op.upper() in labels:
                    norm_ops.append("nn")
                elif "." in op or re.match(r"^[a-zA-Z_]", op):
                    norm_ops.append("nn")  # Assume forward reference
                else:
                    # It's an immediate
                    try:
                        val = parse_number(op)
                        if val > 0xFF or val < -128:
                            norm_ops.append("nn")
                        else:
                            norm_ops.append("n")
                    except ValueError:
                        norm_ops.append("nn")

        key = f"{instr} {', '.join(norm_ops)}"
    else:
        key = instr

    if key not in INSTRUCTIONS:
        # Try without spaces around commas
        key = f"{instr} {','.join(norm_ops)}" if operands else instr

    if key not in INSTRUCTIONS:
        raise ValueError(f"Unknown instruction: {key} (from {instr} {operands})")

    opcode, size = INSTRUCTIONS[key]
    result = []

    # Encode opcode
    if opcode > 0xFF:
        result.append((opcode >> 8) & 0xFF)
        result.append(opcode & 0xFF)
    else:
        result.append(opcode)

    # Encode operands
    for op in operands:
        op_upper = op.upper()
        if op_upper in ["A", "B", "C", "D", "E", "H", "L", "AF", "BC", "DE", "HL", "SP"]:
            continue
        if op_upper in ["NZ", "Z", "NC"]:
            continue
        if op_upper.startswith("[") and op_upper.endswith("]"):
            inner = op_upper[1:-1]
            if inner in ["HL", "DE", "BC", "C"]:
                continue
            # It's a memory address
            op = inner

        if op.upper().startswith("SP+"):
            op = op[3:]

        # Resolve value
        if op in labels:
            val = labels[op]
        elif op.upper() in labels:
            val = labels[op.upper()]
        else:
            try:
                val = parse_number(op)
            except ValueError as err:
                raise ValueError(f"Unknown label or invalid number: {op}") from err

        # For JR instructions, compute relative offset
        if instr in ["JR", "JR NZ", "JR Z", "JR NC", "JR C"]:
            offset = val - (current_addr + 2)
            if offset < -128 or offset > 127:
                raise ValueError(f"JR target out of range: {offset}")
            result.append(offset & 0xFF)
        elif size == 2:
            result.append(val & 0xFF)
        elif size == 3:
            result.append(val & 0xFF)
            result.append((val >> 8) & 0xFF)

    return result, f"{instr} {', '.join(operands)}" if operands else instr


def assemble(source: str) -> list[Assembled]:
    """Assemble source code to bytes."""
    lines = source.split("\n")
    parsed = [parse_line(line, i + 1) for i, line in enumerate(lines)]

    # First pass: collect labels
    labels: dict[str, int] = {}
    current_addr = 0

    for pl in parsed:
        if pl.address is not None:
            current_addr = pl.address
        if pl.label:
            labels[pl.label] = current_addr
        if pl.instruction:
            # Estimate size
            if pl.instruction == "DB":
                size = sum(len(op) - 2 if op.startswith('"') else 1 for op in pl.operands)
            elif pl.instruction == "DW":
                size = len(pl.operands) * 2
            elif pl.instruction == "DS":
                size = parse_number(pl.operands[0])
            else:
                # Look up instruction size
                norm_ops = []
                for op in pl.operands:
                    op_upper = op.upper()
                    if op_upper in [
                        "A",
                        "B",
                        "C",
                        "D",
                        "E",
                        "H",
                        "L",
                        "AF",
                        "BC",
                        "DE",
                        "HL",
                        "SP",
                    ] or op_upper in ["NZ", "Z", "NC"]:
                        norm_ops.append(op_upper)
                    elif op_upper.startswith("[") and op_upper.endswith("]"):
                        inner = op_upper[1:-1]
                        if inner in ["HL", "DE", "BC", "C"]:
                            norm_ops.append(f"[{inner}]")
                        else:
                            norm_ops.append("[nn]")
                    else:
                        norm_ops.append("n")

                key = f"{pl.instruction} {', '.join(norm_ops)}" if pl.operands else pl.instruction
                if key in INSTRUCTIONS:
                    _, size = INSTRUCTIONS[key]
                else:
                    size = 3  # Assume 3 bytes
            current_addr += size

    # Second pass: assemble
    result: list[Assembled] = []
    current_addr = 0

    for pl in parsed:
        if pl.address is not None:
            current_addr = pl.address
        if not pl.instruction:
            continue

        bytes_out, source_str = assemble_instruction(pl.instruction, pl.operands, labels, current_addr)
        result.append(Assembled(current_addr, bytes_out, source_str, pl.comment or ""))
        current_addr += len(bytes_out)

    return result


def to_rust_const(assembled: list[Assembled], name: str = "ROM") -> str:
    """Convert assembled bytes to Rust const array format."""
    lines = []
    lines.append(f"pub const {name}: [u8; 2048] = {{")
    lines.append("    let mut rom = [0u8; 2048];")
    lines.append("")

    for asm in assembled:
        for i, byte in enumerate(asm.bytes):
            addr = asm.address + i
            comment = f"  // {asm.source}" if i == 0 else ""
            if asm.comment and i == 0:
                comment += f" ; {asm.comment}"
            lines.append(f"    rom[0x{addr:04X}] = 0x{byte:02X};{comment}")

    lines.append("")
    lines.append("    rom")
    lines.append("};")
    return "\n".join(lines)


def main():
    if len(sys.argv) < 2:
        print("Usage: assembler.py <source.asm>", file=sys.stderr)
        sys.exit(1)

    with open(sys.argv[1]) as f:
        source = f.read()

    assembled = assemble(source)
    print(to_rust_const(assembled, "CGB_BOOT_ROM"))


if __name__ == "__main__":
    main()
