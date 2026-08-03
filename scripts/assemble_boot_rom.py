#!/usr/bin/env python3
"""Hand-assemble the new DMG boot ROM with real-hardware-like behavior."""

rom = [0x00] * 256

# ── $0000: LD SP, $FFFE ──
rom[0x00:0x03] = [0x31, 0xFE, 0xFF]

# ── $0003: Clear VRAM ($8000–$9FFF) ──
rom[0x03:0x0C] = [0x21, 0x00, 0x80, 0xAF, 0x22, 0xCB, 0x6C, 0x28, 0xFB]

# ── $000C: APU init WITHOUT trigger ──
rom[0x0C:0x10] = [0x3E, 0x80, 0xE0, 0x26]  # LD A,$80; LDH [$FF26] → NR52
rom[0x10:0x12] = [0xE0, 0x11]  # LDH [$FF11] → NR11=$80 (50% duty)
rom[0x12:0x16] = [0x3E, 0xF3, 0xE0, 0x12]  # LD A,$F3; LDH [$FF12] → NR12
rom[0x16:0x18] = [0xE0, 0x25]  # LDH [$FF25] → NR51=$F3
rom[0x18:0x1C] = [0x3E, 0x77, 0xE0, 0x24]  # LD A,$77; LDH [$FF24] → NR50

# ── $001C: BGP = $FC ──
rom[0x1C:0x20] = [0x3E, 0xFC, 0xE0, 0x47]

# ── $0020: Logo tile load ──
rom[0x20:0x23] = [0x11, 0x04, 0x01]  # LD DE, $0104
rom[0x23:0x26] = [0x21, 0x10, 0x80]  # LD HL, $8010
# .logoLoop ($0026):
rom[0x26:0x28] = [0x1A, 0x47]  # LD A,[DE]; LD B,A
rom[0x28:0x2B] = [0xCD, 0xA7, 0x00]  # CALL $00A7
rom[0x2B:0x2E] = [0xCD, 0xA7, 0x00]  # CALL $00A7
rom[0x2E] = 0x13  # INC DE
rom[0x2F:0x32] = [0x7B, 0xEE, 0x34]  # LD A,E; XOR $34
rom[0x32:0x34] = [0x20, 0xF2]  # JR NZ, .logoLoop (→$0026)

# ── $0034: Build BG tile map ──
rom[0x34:0x36] = [0x3E, 0x19]  # LD A, $19
rom[0x36:0x39] = [0x21, 0x2F, 0x99]  # LD HL, $992F
rom[0x39:0x3B] = [0x0E, 0x0C]  # LD C, 12
# .tmapLoop ($003B):
rom[0x3B] = 0x3D  # DEC A
rom[0x3C:0x3E] = [0x28, 0x08]  # JR Z, .tmapDone (+8 → $0046)
rom[0x3E] = 0x32  # LD [HL-], A
rom[0x3F] = 0x0D  # DEC C
rom[0x40:0x42] = [0x20, 0xF9]  # JR NZ, .tmapLoop (→$003B)
rom[0x42:0x44] = [0x2E, 0x0F]  # LD L, $0F
rom[0x44:0x46] = [0x18, 0xF5]  # JR .tmapLoop (→$003B)

# ── $0046: Pre-LCD delay ──
rom[0x46:0x49] = [0x21, 0xFE, 0x07]  # LD HL, $07FE (2046)
rom[0x49:0x4B] = [0x00, 0x00]  # 2 × NOP
# .preLoop ($004B):
rom[0x4B:0x50] = [0x2B, 0x7C, 0xB5, 0x20, 0xFB]  # DEC HL; LD A,H; OR L; JR NZ

# ── $0050: Enable LCD ──
rom[0x50:0x54] = [0x3E, 0x91, 0xE0, 0x40]  # LD A,$91; LDH [$FF40]

# ── $0054: SCY = $64 (100) ──
rom[0x54:0x58] = [0x3E, 0x64, 0xE0, 0x42]  # LD A,$64; LDH [$FF42]

# ── $0058: Set up scroll loop registers ──
rom[0x58] = 0x57  # LD D, A  (D = 100)
rom[0x59] = 0x04  # INC B    (B = 1, scroll flag)
rom[0x5A:0x5C] = [0x26, 0x00]  # LD H, 0 (frame counter)

# ═══════════════════════════════════════════════════════════════
# ── $005C: Main scroll loop ──
# ═══════════════════════════════════════════════════════════════

LOOP = 0x5C

# .loop ($005C): LD E, 2
rom[0x5C:0x5E] = [0x1E, 0x02]  # LD E, 2 (VBlank counter)

# .waitNotVblank ($005E): wait until LY < 144
WAIT_NOT_VB = 0x5E
rom[0x5E:0x60] = [0xF0, 0x44]  # LDH A, [$FF44]
rom[0x60:0x62] = [0xFE, 0x90]  # CP 144
# JR NC → still in VBlank (LY >= 144), keep waiting
offset = WAIT_NOT_VB - (0x64)  # -6
rom[0x62:0x64] = [0x30, offset & 0xFF]  # JR NC, .waitNotVblank

# .waitVblank ($0064): wait until LY >= 144
WAIT_VB = 0x64
rom[0x64:0x66] = [0xF0, 0x44]  # LDH A, [$FF44]
rom[0x66:0x68] = [0xFE, 0x90]  # CP 144
# JR C → LY < 144, keep waiting
offset = WAIT_VB - (0x6A)  # -6
rom[0x68:0x6A] = [0x38, offset & 0xFF]  # JR C, .waitVblank

# VBlank entered; decrement E
rom[0x6A] = 0x1D  # DEC E
# JR NZ → wait for another VBlank
offset = WAIT_NOT_VB - (0x6D)  # -15
rom[0x6B:0x6D] = [0x20, offset & 0xFF]  # JR NZ, .waitNotVblank

# ── $006D: Sound trigger check ──
rom[0x6D] = 0x24  # INC H
rom[0x6E] = 0x7C  # LD A, H
rom[0x6F:0x71] = [0x0E, 0x13]  # LD C, $13 (LOW(rNR13))
rom[0x71:0x73] = [0x1E, 0x83]  # LD E, $83 (first note)
rom[0x73:0x75] = [0xFE, 0x62]  # CP $62 (iter 98?)

PLAY_SOUND = 0x7D
offset = PLAY_SOUND - (0x77)  # 6
rom[0x75:0x77] = [0x28, offset & 0xFF]  # JR Z, .playSound

rom[0x77:0x79] = [0x1E, 0xC1]  # LD E, $C1 (second note)
rom[0x79:0x7B] = [0xFE, 0x64]  # CP $64 (iter 100?)

NO_SOUND = 0x85
offset = NO_SOUND - (0x7D)  # 8
rom[0x7B:0x7D] = [0x20, offset & 0xFF]  # JR NZ, .noSound

# .playSound ($007D):
rom[0x7D] = 0x7B  # LD A, E
rom[0x7E] = 0xE2  # LD ($FF00+C), A → NR13
rom[0x7F] = 0x0C  # INC C → C = $14
rom[0x80:0x82] = [0x3E, 0x87]  # LD A, $87
rom[0x82] = 0xE2  # LD ($FF00+C), A → NR14 (trigger)

# JR to .noSound (skip 0 bytes since .noSound follows)
offset = NO_SOUND - (0x85)  # 0
rom[0x83:0x85] = [0x18, offset & 0xFF]  # JR .noSound

# .noSound ($0085):
rom[0x85:0x87] = [0xF0, 0x42]  # LDH A, [$FF42] (read SCY)
rom[0x87] = 0x90  # SUB B (SCY -= B)
rom[0x88:0x8A] = [0xE0, 0x42]  # LDH [$FF42], A

# ── $008A: Loop control ──
rom[0x8A] = 0x15  # DEC D
offset = LOOP - (0x8D)  # $5C - $8D = -49
rom[0x8B:0x8D] = [0x20, offset & 0xFF]  # JR NZ, .loop

rom[0x8D] = 0x05  # DEC B (1→0 hold, 0→$FF exit)

DONE = 0xBC  # Post-loop continuation in padding area
offset = DONE - (0x90)  # $BC - $90 = 44
rom[0x8E:0x90] = [0x20, offset & 0xFF]  # JR NZ, .done

rom[0x90:0x92] = [0x16, 0x20]  # LD D, 32 (hold iterations)
offset = LOOP - (0x94)  # $5C - $94 = -56
rom[0x92:0x94] = [0x18, offset & 0xFF]  # JR .loop

# ── $0094–$00A6: padding ──
# (already zeros)

# ═══════════════════════════════════════════════════════════════
# ── $00A7: DoubleBitsAndWriteRow (UNCHANGED) ──
# ═══════════════════════════════════════════════════════════════
rom[0xA7:0xA9] = [0x3E, 0x04]  # LD A, 4
rom[0xA9:0xAB] = [0x0E, 0x00]  # LD C, 0
rom[0xAB:0xAD] = [0xCB, 0x20]  # SLA B
rom[0xAD] = 0xF5  # PUSH AF
rom[0xAE:0xB0] = [0xCB, 0x11]  # RL C
rom[0xB0] = 0xF1  # POP AF
rom[0xB1:0xB3] = [0xCB, 0x11]  # RL C
rom[0xB3] = 0x3D  # DEC A
rom[0xB4:0xB6] = [0x20, 0xF5]  # JR NZ, .dblLoop (→$00AB)
rom[0xB6] = 0x79  # LD A, C
rom[0xB7:0xB9] = [0x22, 0x23]  # LD [HL+],A; INC HL
rom[0xB9:0xBB] = [0x22, 0x23]  # LD [HL+],A; INC HL
rom[0xBB] = 0xC9  # RET

# ═══════════════════════════════════════════════════════════════
# ── $00BC: Post-loop (.done) ──
# ═══════════════════════════════════════════════════════════════

# IF = $E1
rom[0xBC:0xC0] = [0x3E, 0xE1, 0xE0, 0x0F]  # LD A,$E1; LDH [$FF0F]

# Fine-tune delay: placeholder LD HL,$0000 + loop
# Will calibrate after measuring total M-cycles
rom[0xC0:0xC3] = [0x21, 0x00, 0x00]  # LD HL, $0000 (placeholder)
rom[0xC3:0xC5] = [0x00, 0x00]  # 2 × NOP
rom[0xC5:0xCA] = [0x2B, 0x7C, 0xB5, 0x20, 0xFB]  # DEC HL; LD A,H; OR L; JR NZ

# Set post-boot register state
rom[0xCA:0xCD] = [0x21, 0xB0, 0x01]  # LD HL, $01B0
rom[0xCD:0xCF] = [0xE5, 0xF1]  # PUSH HL; POP AF → AF=$01B0
rom[0xCF:0xD2] = [0x21, 0x4D, 0x01]  # LD HL, $014D
rom[0xD2:0xD5] = [0x01, 0x13, 0x00]  # LD BC, $0013
rom[0xD5:0xD8] = [0x11, 0xD8, 0x00]  # LD DE, $00D8
rom[0xD8:0xDB] = [0xC3, 0xFE, 0x00]  # JP $00FE

# ── $00FE: BootGame ──
rom[0xFE:0x100] = [0xE0, 0x50]  # LDH [$FF50], A

# ═══════════════════════════════════════════════════════════════
# VERIFICATION
# ═══════════════════════════════════════════════════════════════

print("ROM assembled successfully!")
print(f"Total non-zero bytes: {sum(1 for b in rom if b != 0)}")
print(f"Scroll section: $005C-$0093 ({0x94 - 0x5C} bytes)")
print(f"Post-loop: $00BC-$00DA ({0xDB - 0xBC} bytes)")
print()


# Verify jump offsets
def check_jr(addr, target, name):
    offset_byte = rom[addr + 1]
    signed_offset = offset_byte if offset_byte < 128 else offset_byte - 256
    actual_target = addr + 2 + signed_offset
    ok = "OK" if actual_target == target else f"FAIL (goes to ${actual_target:04X})"
    print(f"  JR at ${addr:04X} → ${target:04X}: offset={signed_offset:+d} (${offset_byte:02X}) {ok}")
    assert actual_target == target, f"{name}: expected ${target:04X}, got ${actual_target:04X}"


print("Jump offset verification:")
check_jr(0x32, 0x26, "logoLoop")
check_jr(0x3C, 0x46, "tmapDone")
check_jr(0x40, 0x3B, "tmapLoop NZ")
check_jr(0x44, 0x3B, "tmapLoop JR")
check_jr(0x4E, 0x4B, "preLoop")
check_jr(0x62, 0x5E, "waitNotVblank NC")
check_jr(0x68, 0x64, "waitVblank C")
check_jr(0x6B, 0x5E, "waitNotVblank NZ")
check_jr(0x75, 0x7D, "playSound Z")
check_jr(0x7B, 0x85, "noSound NZ")
check_jr(0x83, 0x85, "noSound JR")
check_jr(0x8B, 0x5C, "loop NZ")
check_jr(0x8E, 0xBC, "done NZ")
check_jr(0x92, 0x5C, "loop JR")
check_jr(0xC8, 0xC5, "fineLoop")
print()

# Verify test assertions
print("Test assertion verification:")
assert rom[0x10] == 0xE0 and rom[0x11] == 0x11, "NR11 write"
print("  ✓ NR11 duty cycle write present")

apu_init = rom[0x0C:0x1C]
assert not any(apu_init[i] == 0xE0 and apu_init[i + 1] == 0x14 for i in range(len(apu_init) - 1))
assert not any(apu_init[i] == 0xE0 and apu_init[i + 1] == 0x13 for i in range(len(apu_init) - 1))
print("  ✓ No NR13/NR14 trigger in APU init")

assert rom[0x54:0x58] == [0x3E, 0x64, 0xE0, 0x42]
print("  ✓ SCY starts at $64 (100)")

scroll = rom[0x5A:0xFE]
assert any(scroll[i] == 0xF0 and scroll[i + 1] == 0x44 for i in range(len(scroll) - 1))
print("  ✓ LY polling (F0 44) present")

assert 0x83 in scroll and 0xC1 in scroll
print("  ✓ Two note frequencies ($83, $C1) present")

assert 0x62 in scroll and 0x64 in scroll
print("  ✓ Trigger thresholds ($62, $64) present")

assert any(scroll[i] == 0x16 and scroll[i + 1] == 0x20 for i in range(len(scroll) - 1))
print("  ✓ Hold phase LD D,$20 (32 iterations) present")

print()
print("All checks passed!")
print()

# Output as Rust array format
print("Rust array bytes:")
for i in range(0, 256, 16):
    chunk = rom[i : i + 16]
    hex_vals = ", ".join(f"0x{b:02X}" for b in chunk)
    addr_end = min(i + 15, 255)
    print(f"    {hex_vals}, // ${i:04X}–${addr_end:04X}")
