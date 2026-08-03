import re
from pathlib import Path

from py65.devices.mpu6502 import MPU
from py65.disassembler import Disassembler

ROM_PATH = Path(
    "/Users/henrikku/repos/neser/roms/nes/automated_tests/fadeout_and_triangle_tests/fadeout_and_triangle_test.nes"
)


def triangle_freq(timer: int) -> float:
    return 1_789_773.0 / (32.0 * (timer + 1))


def main() -> None:
    asm_path = ROM_PATH.with_suffix(".asm")

    data = ROM_PATH.read_bytes()
    if len(data) < 16 or data[0:4] != b"NES\x1a":
        raise SystemExit("Invalid iNES file")

    prg_banks = data[4]
    chr_banks = data[5]
    flags6 = data[6]
    flags7 = data[7]
    trainer = (flags6 & 0x04) != 0
    trainer_size = 512 if trainer else 0
    prg_size = prg_banks * 16 * 1024
    prg_start = 16 + trainer_size
    prg_end = prg_start + prg_size
    prg = data[prg_start:prg_end]

    mpu = MPU()
    mem = mpu.memory

    if prg_size == 0x4000:
        base = 0xC000
        mem[0x8000 : 0x8000 + 0x4000] = prg
        mem[0xC000 : 0xC000 + 0x4000] = prg
    else:
        base = 0x8000
        mem[base : base + prg_size] = prg

    dis = Disassembler(mpu)

    nmi_vec = mem[0xFFFA] | (mem[0xFFFB] << 8)
    reset_vec = mem[0xFFFC] | (mem[0xFFFD] << 8)
    irq_vec = mem[0xFFFE] | (mem[0xFFFF] << 8)

    labels = {"NMI": nmi_vec, "RESET": reset_vec, "IRQ": irq_vec}
    for label, addr in labels.items():
        dis._address_parser.labels[label] = addr

    last_imm = {"A": None, "X": None, "Y": None}
    tri_timer_low = None
    tri_timer_high = None
    last_tri_linear = None

    lines = []
    lines.append("; Disassembly of PRG-ROM for fadeout_and_triangle_test.nes")
    lines.append(f"; PRG ROM banks: {prg_banks} (size={prg_size} bytes)")
    lines.append(f"; CHR ROM banks: {chr_banks}")
    lines.append(f"; Mapper: {(flags6 >> 4) | (flags7 & 0xF0)}")
    lines.append(f"; Mirroring: {'vertical' if (flags6 & 0x01) else 'horizontal'}")
    lines.append(f"; Vectors: NMI=${nmi_vec:04X} RESET=${reset_vec:04X} IRQ=${irq_vec:04X}")
    lines.append(";")
    lines.append("; Triangle channel notes:")
    lines.append("; - $4015 bit 2 enables triangle output")
    lines.append("; - $4008: bit7=control (length counter halt/linear control), bits6-0=linear counter reload")
    lines.append("; - $400A/$400B: 11-bit timer; frequency = CPU/(32*(timer+1))")
    lines.append("; - $400B bits7-3 set length counter load")
    lines.append(";")

    pc = base
    end = base + prg_size

    while pc < end:
        length, text = dis.instruction_at(pc)
        raw = bytes(mem[pc : pc + length])
        bytes_str = " ".join(f"{b:02X}" for b in raw)

        imm_match = re.match(r"^(LDA|LDX|LDY) #\$([0-9A-F]{2})$", text)
        if imm_match:
            reg = imm_match.group(1)[-1]
            last_imm[reg] = int(imm_match.group(2), 16)

        comment_parts = []
        store_match = re.match(r"^(STA|STX|STY) \$([0-9A-F]{4})", text)
        if store_match:
            reg = store_match.group(1)[-1]
            target = int(store_match.group(2), 16)
            value = last_imm.get(reg)

            if target == 0x4015:
                if value is not None:
                    enabled = []
                    if value & 0x01:
                        enabled.append("square1")
                    if value & 0x02:
                        enabled.append("square2")
                    if value & 0x04:
                        enabled.append("triangle")
                    if value & 0x08:
                        enabled.append("noise")
                    if value & 0x10:
                        enabled.append("dmc")
                    comment_parts.append(f"APU enable: {', '.join(enabled) if enabled else 'none'}")
                else:
                    comment_parts.append("APU enable write")

            if target == 0x4008:
                if value is not None:
                    tri_linear = value & 0x7F
                    control = (value >> 7) & 1
                    comment_parts.append(f"triangle linear reload={tri_linear} control={control}")
                    if last_tri_linear is not None and tri_linear != last_tri_linear:
                        comment_parts.append("linear reload change (fade/gate shaping)")
                    last_tri_linear = tri_linear
                else:
                    comment_parts.append("triangle linear counter write")

            if target == 0x400A:
                if value is not None:
                    tri_timer_low = value
                    comment_parts.append(f"triangle timer low=${value:02X}")
                else:
                    comment_parts.append("triangle timer low write")

            if target == 0x400B:
                if value is not None:
                    tri_timer_high = value & 0x07
                    length_load = value >> 3
                    comment_parts.append(f"triangle timer high=${tri_timer_high:02X} length=${length_load}")
                    if tri_timer_low is not None:
                        timer = (tri_timer_high << 8) | tri_timer_low
                        freq = triangle_freq(timer)
                        comment_parts.append(f"triangle freq≈{freq:.2f} Hz")
                else:
                    comment_parts.append("triangle timer high/length write")

        for label, addr in labels.items():
            if addr == pc:
                lines.append(f"{label}:")
                break

        comment = "" if not comment_parts else " ; " + " | ".join(comment_parts)
        lines.append(f"${pc:04X}: {bytes_str:<11} {text}{comment}")

        pc += max(1, length)

    asm_path.write_text("\n".join(lines))
    print(f"Wrote {asm_path}")


if __name__ == "__main__":
    main()
