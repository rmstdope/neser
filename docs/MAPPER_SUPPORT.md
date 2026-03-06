# Mapper Support

This document catalogues every mapper implemented in **neser** and cross-references each one
against the [NesDev wiki](https://www.nesdev.org/wiki/) specification. Where the implementation
deviates from the specification the delta is noted. A separate section lists commonly-needed
mappers that are not yet implemented.

> **Legend**
>
> | Symbol | Meaning |
> |--------|---------|
> | ✅ | Fully implemented, no known delta |
> | ⚠️ | Implemented with known limitations / partial deltas |
> | ❌ | Not implemented (stub or missing) |

---

## Implemented Mappers

| # | Hardware Name | PRG Banking | CHR Banking | Mirroring | IRQ | Audio | PRG-RAM | Status | Delta / Notes |
|---|---------------|-------------|-------------|-----------|-----|-------|---------|--------|---------------|
| 0 | NROM | Fixed 16 or 32 KB | Fixed 8 KB ROM | Header | None | None | Optional 2–8 KB | ✅ | Family BASIC PRG-RAM present in neser. No deltas. |
| 1 | MMC1 (SxROM) | 16 KB or 32 KB switchable | 4 KB or 8 KB switchable | H / V / 1scA / 1scB | None | None | Present, disable-able | ⚠️ | **Submappers 5 (SEROM/SHROM – fixed 32 KB PRG), 6 (2ME EEPROM Famicom Network System) and 7 (Kaiser KS-7058 hard-wired NT) are not handled.** MMC1A vs MMC1B PRG-RAM disable bit difference unverified. SUROM/SXROM 512 KB PRG-ROM (CHR bank bits extend PRG address) not tested. |
| 2 | UxROM | 16 KB switchable + last fixed | 8 KB CHR-RAM | Header | None | None | None | ✅ | NES 2.0 submappers (bus-conflict disambiguation) informational only; bus conflicts not emulated – acceptable for gameplay. |
| 3 | CNROM | Fixed 32 KB | 8 KB switchable ROM | Header | None | None | None | ⚠️ | Submapper bus-conflict modes (sub 1 = no conflicts, sub 2 = AND conflicts) not distinguished. Mapper 185 CHR-disable anti-piracy extension unimplemented. Hayauchi Super Igo PRG-RAM not handled (edge case). |
| 4 | MMC3 / TxROM | 8 KB × 2 switchable + 2 fixed | 2 KB × 2 + 1 KB × 4 | H / V | Scanline (A12) | None | Optional, write-protect | ⚠️ | **IRQ Sharp vs NEC mode selected via CRC heuristic, not NES 2.0 submapper.** MMC6 (submapper 1) uses a different PRG-RAM protect scheme – not properly distinguished. Waixing T9552 (submapper 5, scrambled CHR) unimplemented. |
| 5 | MMC5 (ExROM) | 8 / 16 / 32 KB flexible | 1 / 2 / 4 / 8 KB flexible | H / V / 1sc / 4-screen | CPU-cycle | 8-ch PCM | Up to 64 KB | ⚠️ | **Sunsoft 5B expansion audio (YM2149) present on 5B variant – irrelevant to MMC5.** Vertical split ($5200–$5202) registers present but split-screen rendering not fully functional. Extended attribute mode ($5104 = 01) partially implemented. $5130 upper CHR bank bits need verification. MMC5A extra regs ($5207–$520A) unimplemented (no commercial games use them). |
| 6 | Super Magic Card | Complex bank modes via $4500-style regs | 1 / 2 / 4 / 8 KB | H / V switchable | None | None | Banked WRAM | ⚠️ | **Trainer JSR $7003 execution not implemented.** Submapper 0 = loader at $7000. Unusual hardware; no other emulator fully emulates all modes. |
| 7 | AxROM | 32 KB switchable | 8 KB CHR-RAM | 1-screen switchable | None | None | None | ✅ | NES 2.0 submappers (AMROM/AOROM with bus conflicts vs ANROM without) not distinguished – bus conflicts not emulated; acceptable. |
| 8 | Super Magic Card (variant) | As mapper 6 | As mapper 6 | As mapper 6 | None | None | WRAM | ⚠️ | Shares implementation with mapper 6; trainer not executed. |
| 9 | MMC2 (PxROM) | 8 KB switchable + 3 fixed | 4 KB latch-switched × 2 | H / V | None | None | None | ✅ | Latch 0 fires only at $0FD8 (single address); latch 1 fires at $1FD8–$1FDF (range). Confirmed correct. |
| 10 | MMC4 (FxROM) | 16 KB switchable + last fixed | 4 KB latch-switched × 2 (symmetric) | H / V | None | None | None | ✅ | Symmetric latch behavior for both pattern tables. PRG-RAM at $6000 fixed. No deltas. |
| 11 | Color Dreams | 32 KB switchable | 8 KB switchable | Header | None | None | None | ✅ | Bus conflicts present on real hardware but not emulated – acceptable. No deltas. |
| 13 | CPROM (Videomation) | Fixed 32 KB | Lower 4 KB fixed to bank 0, upper 4 KB via 2-bit selector | Fixed vertical | None | None | None | ✅ | Bus conflicts not emulated – acceptable. No deltas. |
| 15 | 100-in-1 Multicart | 4 modes: NROM-256 / UNROM / NROM-64 / NROM-128 | 8 KB CHR-RAM | H / V switchable | None | None | None | ⚠️ | **CHR-RAM write-protect (applied in PRG modes 0 and 3) may not be enforced.** PRG-RAM not present on real hardware – emulator addition for hacks. |
| 16 | Bandai FCG (LZ93D50 / FCG-1/2) | 16 KB switchable + last fixed | 8 × 1 KB switchable | H / V / 1scA / 1scB | CPU-cycle (IRQ latch/counter) | None | None (sub 3 = 8 KB WRAM; use mapper 153) | ⚠️ | **24C02 EEPROM save (Dragon Ball Z II/III/Gaiden, SD Gundam stories, etc.) not implemented – saves broken for those games.** Submappers 1 (24C01 → use mapper 159), 2 (Datach → use mapper 157), 3 (WRAM → use mapper 153) deprecated per NES 2.0 and not routed. FCG-1/2 (sub 4) vs LZ93D50 (sub 5) IRQ latch behavior differs: sub 4 writes directly to counter, sub 5 uses a latch copied on $800A write – verify neser handles both paths. |
| 17 | Super Magic Card (variant) | As mapper 6 | As mapper 6 | As mapper 6 | None | None | WRAM | ⚠️ | Shares implementation with mapper 6; trainer not executed. |
| 18 | Jaleco SS 88006 | 8 KB × 3 switchable + last 8 KB fixed | 8 × 1 KB switchable | H / V / 1scA / 1scB switchable | CPU-cycle down-counter (4/8/12/16-bit) | None | None | ✅ | Nibble-paired PRG/CHR bank registers (`addr & 0xF003` decode), IRQ reload/ack via `$F000/$F001`, and 4-way mirroring at `$F002` are implemented. |
| 19 | Namco 163 (N163) | 8 KB × 3 switchable + last fixed | 8 × 1 KB switchable; banks $E0–$FF map CIRAM | H / V / CIRAM-switched | 15-bit CPU-cycle up-counter | 8-channel wavetable | 128 B chip RAM (battery-backed) | ⚠️ | **$F800 PRG-RAM write-protect register (key $40–$4E) may not be implemented.** NT-via-CHR-bank (register values $E0–$FF enabled by $E800 bits 6/7) needs verification. Audio submapper volume levels (0–5) are informational only. Pin 22 open-collector behaviour unverified. |
| 20 | Famicom Disk System (FDS) | 32 KB work RAM ($6000–$DFFF) | 8 KB CHR-RAM | Controlled by $4025 bit 3 (default Horizontal) | 16-bit CPU-cycle down-counter ($4020–$4022) | Wave-table ($4040–$408F, backed but not mixed) | 32 KB work RAM | ⚠️ | **Disk I/O not emulated: .fds disk image loading, disk reads/writes and CRC are not implemented.** BIOS ROM must be supplied as PRG-ROM; no user-supplied BIOS path. FDS audio (wave table + modulation channel) registers are stored but not mixed into the audio output. |
| 21 | Konami VRC4a/VRC4c | 8 KB × 2 switchable + 2 fixed | 8 × 1 KB switchable | H / V / 1sc switchable | CPU-cycle / scanline latch-based | None | 8 KB optional | ⚠️ | VRC2 microwire EEPROM latch read-back at $6000 (required for Contra/Goemon 2 on some boards) unverified. WRAM enable bit at $9002 (VRC4 only) – verify neser honours it. |
| 22 | Konami VRC2a | 8 KB × 2 switchable + 2 fixed | 8 × 1 KB switchable (CHR shifted right by 1) | H / V switchable | None | None | None | ⚠️ | CHR right-shift-by-1 (A0 ignored on CHR registers) must be applied. Bit 0 of register address ignored on all VRC2a writes. |
| 23 | Konami VRC2b/VRC4e/VRC4f | 8 KB × 2 switchable + 2 fixed | 8 × 1 KB switchable | H / V / 1sc switchable | CPU-cycle / scanline | None | 8 KB optional | ⚠️ | Address bit mapping disambiguated by NES 2.0 submappers. WRAM control at $9002 (VRC4 variants). Same delta as mapper 21. |
| 24 | Konami VRC6a | 8 KB × 3 switchable + last fixed | 8 × 1 KB or 16 × 0.5 KB switchable | H / V / 1sc / ROM-NT switchable | CPU-cycle or scanline (M bit) | 3-ch pulse/sawtooth | None | ✅ | $B003 PPU banking style register (H/V/1scA/1scB) confirmed. Commercial games use only mirroring modes 0–3 which are implemented. VRC6a vs VRC6b (A0/A1 swapped for mapper 26) handled by separate mapper entry. Audio implemented. |
| 25 | Konami VRC4b/VRC4d | 8 KB × 2 switchable + 2 fixed | 8 × 1 KB switchable | H / V / 1sc switchable | CPU-cycle / scanline | None | 8 KB optional | ⚠️ | Same A0/A1 swapping considerations as VRC2/VRC4 family. WRAM enable bit at $9002. Same delta as mapper 21/23. |
| 26 | Konami VRC6b | 8 KB × 3 switchable + last fixed | 8 × 1 KB or 16 × 0.5 KB | H / V / 1sc / ROM-NT | CPU-cycle or scanline | 3-ch pulse/sawtooth | None | ✅ | A0/A1 swapped relative to VRC6a (mapper 24). Implemented as separate mapper entry. Audio implemented. |
| 29 | Sealie Computing | 16 KB switchable at `$8000` + last 16 KB fixed | 8 KB CHR-RAM switchable (4 banks) | Fixed from header | None | None | 8 KB WRAM | ✅ | Register writes at `$8000-$FFFF` use bits `[..PPP PCC]` for PRG/CHR selection. Uses 32 KB CHR-RAM and fixed mirroring from header. |
| 32 | Irem G-101 | 8 KB × 2 switchable + 2 fixed; mode 1 swaps $8000/$C000 | 8 × 1 KB switchable | H / V via bit; hardwired 1-screen on Major League | None | None | None | ⚠️ | **NES 2.0 submapper for Major League (hardwired 1-screen + PRG mode 1 disabled) must be handled via hash or submapper header.** PRG mode 1 ("愛先生" game) rare but present. |
| 33 | Taito TC0190 | 8 KB × 2 switchable + 2 fixed | 2 × 2 KB + 4 × 1 KB switchable | H / V via bit in $8000 | None | None | None | ✅ | Unlike mapper 48 (TC0350), no IRQ. Note: many mapper 48 dumps were mislabelled as 033 – ensure correct routing. No deltas. |
| 34 | BNROM / NINA-001 | BNROM: 32 KB switchable; NINA: 32 KB switchable | BNROM: 8 KB CHR-RAM; NINA: 4 KB × 2 switchable | Fixed (solder pads) | None | None | BNROM: none; NINA: 8 KB PRG-RAM | ⚠️ | NES 2.0 submapper 1 = strict NINA-001 (bit0 PRG at $7FFD, CHR at $7FFE/$7FFF), submapper 2 = BNROM. **Submapper 0 / iNES fallback uses CHR-ROM presence (CHR present → NINA path, no CHR → BNROM).** For submapper 0 NINA hacks, FCEUX-compatible hybrid behavior is enabled ($8000-$FFFF PRG writes accepted, $7FFD uses full register value). BNROM AND-type bus conflicts are emulated. NINA registers at $7FFD/$7FFE/$7FFF overlap PRG-RAM. |
| 36 | TXC 01-22000-400 (PCM063 family) | 32 KB switchable via TXC output (`RR[1:0]`) | 8 KB switchable via `$4200` register | Fixed from header | None | None | None | ✅ | Implements masked TXC register window at `$4100-$5FFF` plus PRG apply strobe at `$8000`. Includes `$4100` status read bits `[5:4]` and save-state register roundtrip. |
| 40 | NTDEC 2722 (SMB2J FDS conversion) | Fixed banks at $6000/$8000/$A000/$E000 + switchable $C000 | 8 KB CHR-RAM | Fixed via header | CPU-cycle (4096 cycles fixed) | None | None | ⚠️ | **Submapper 1 (NTDEC 2752 multicart) outer bank register at $C000-$DFFF not implemented.** IRQ fires 4096 cycles after enable (CD4020 13-bit counter, self-acknowledges after another 4096 cycles). |
| 42 | FDS cartridge conversion | 8 KB switchable at $6000, last 32 KB fixed | 8 KB switchable ROM or CHR-RAM | H / V switchable | CPU-cycle (fires after 24576 cycles) | None | None | ✅ | IRQ is a 15-bit counter; asserted while two MSBs are set (24576 on, 8192 off cycle). CHR-RAM used by Bio Miracle Bokutte Upa. No deltas. |
| 44 | Super Big 7-in-1 (MMC3 multicart) | MMC3 8 KB PRG banks within 128/256 KB blocks | MMC3 CHR within 128/256 KB blocks | MMC3 H / V | MMC3 scanline | None | None | ✅ | Block select in $A001 bits [2:0]. Block 6/7 = 256 KB. 7 selects same as 6. No PRG-RAM. No deltas. |
| 45 | GA23C multicart | MMC3 8 KB PRG + outer PRG-AND/PRG-OR | MMC3 CHR + outer CHR-AND/CHR-OR | MMC3 H / V | MMC3 scanline | None | Via MMC3 WRAM bits | ✅ | Four sequential outer bank registers at $6000. Register lock bit prevents further writes. DIP-switch menu selection ($5000–$5FFF read) for 超强年度新卡 multicart present. No deltas. |
| 46 | Rumble Station (Color Dreams multicart) | 32 KB page via outer [$6000 bits 3:0] + inner [$8000 bit 0] | 8 KB bank via outer [$6000 bits 7:4] + inner [$8000 bits 6:4] | Fixed by header | None | None | None | ✅ | $6000–$7FFF is outer register (no PRG-RAM). At power-up outer = 0. No PRG-RAM. No deltas. |
| 47 | Super Spike V'Ball + World Cup (MMC3 2-game) | MMC3 within 128 KB blocks (1-bit block select) | MMC3 CHR within 128 KB blocks | MMC3 H / V | MMC3 scanline | None | None | ✅ | Block register at $6000–$7FFF writable only when MMC3 PRG-RAM is enabled+writable. Each block = 128 KB PRG + 128 KB CHR. No deltas. |
| 49 | Super HIK 4-in-1 (MMC3 multicart) | MMC3 within 128 KB blocks; 32 KB mode when O=0 | MMC3 CHR within 128 KB blocks | MMC3 H / V | MMC3 scanline | None | None | ✅ | Multicart register at $6000–$7FFF. Mode bit O: 0 = entire 32 KB fixed (ignores MMC3 PRG regs), 1 = MMC3 PRG normal. Block = BB bits. No deltas. |
| 50 | SMB2J N-32 (FDS conversion) | PRG scrambled [HLLM] at $4020; fixed $6000/$8000/$A000/$E000; switchable $C000 | 8 KB CHR-ROM/RAM | Fixed by header | CPU-cycle (fires at 4096 cycles, $0FFF→$1000) | None | None | ✅ | Register mask $4120. IRQ enable at $4120[0]; disable also acknowledges and resets counter. PRG register unscrambling implemented. No deltas. |
| 66 | GxROM (GNROM / MHROM) | 32 KB switchable | 8 KB switchable | Fixed H or V (solder pads / header) | None | None | None | ✅ | Bus conflicts not emulated – acceptable. MHROM uses only 2 PRG banks (bit 5 = 0). No deltas. |
| 68 | Sunsoft-4 | 16 KB switchable + last fixed | 4 × 2 KB switchable ROM; CHR ROM can replace nametables | H / V / 1scA / 1scB switchable | None | None | 8 KB PRG-RAM | ⚠️ | **ROM nametable mode ($E000 bit 4 = 1, nametable banks from $C000/$D000) must be implemented for After Burner and Maharaja.** Licensing IC timer at $6000 for Nantettatte!! Baseball not emulated (internal/external ROM switching) – that game is unlikely to be played. PRG-RAM enable bit in $F000 bit 4. |
| 69 | Sunsoft FME-7 / 5A / 5B | 8 KB × 4 switchable + last fixed; $6000 can be PRG-ROM or PRG-RAM | 8 × 1 KB switchable | H / V / 1scA / 1scB switchable | CPU-cycle 16-bit (separate counter-enable and IRQ-enable) | **5B: YM2149 (Sunsoft 5B audio)** | Up to 512 KB (FME-7) / 256 KB (5A/5B) switchable | ⚠️ | **Sunsoft 5B YM2149 expansion audio not implemented → Gimmick! (and Hebereke, Gremlins 2 JP) missing music tracks.** PRG bank 0 at $6000: RAM/ROM select and enable bits in register $8 – verify PRG-RAM enable handled. |
| 71 | Camerica (BF9093/BF9096/BF9097) | 16 KB switchable + last fixed | 8 KB CHR-RAM | Hardwired H or V (most); 1-screen switchable (Fire Hawk only) | None | None | None | ⚠️ | **NES 2.0 submapper 1 (Fire Hawk, BF9097) uses mirroring register at $8000–$9FFF (bit 4). Most games do not have this register. Verify that neser applies the 1-screen mirroring only when writing to $8000–$9FFF (not $C000–$FFFF).** MiG 29 uses DMC IRQs and is picky about timing (APU concern, not mapper). |
| 77 | IREM NINA-03 (Napoleon Senki) | Fixed 32 KB | 4 KB switchable CHR-ROM ($0000–$0FFF); 2 KB fixed CHR-RAM ($1000–$17FF) | One-screen lower (fixed) | None | None | None | ✅ | Register: any write to $8000–$FFFF; bits [3:0] select the 4 KB CHR-ROM page. Uses internal CIRAM nametables with fixed one-screen-lower mirroring; the 2 KB CHR-RAM is used only for pattern data. No known deltas from current implementation. |
| 78 | Holy Diver / Uchuusen – Cosmo Carrier | 16 KB switchable + last fixed | 8 KB switchable CHR-ROM | Mapper-controlled (bit 3) | None | None | None | ⚠️ | **Bit 3 mirroring interpretation differs by game: Holy Diver (NES 2.0 submapper 3) → 0 = H, 1 = V; Uchuusen/Cosmo Carrier (NES 2.0 submapper 1) → 0 = 1scA, 1 = 1scB. Without submapper info, the iNES1 "alternative nametables" flag is used as a heuristic, but this is unreliable. Submapper support required for correct behaviour.** Bus conflicts not emulated. |
| 81 | NTDEC N715021 (Super Gun) | Super Gun (NTDEC) | 16 KB switchable + last fixed; 8 KB CHR switchable; mirroring fixed | None | None | None | ✅ | Address-latch mapper: bits [3:2] of write address select PRG bank; bits [1:0] select CHR bank. Data byte ignored. No deltas. |
| 82 | Taito X1-017 | Kyuukyoku Harikiri Stadium II/III | 3 × 8 KB switchable PRG + last fixed; CHR 2×2 KB + 4×1 KB (mode-selectable); H/V switchable; 5 KB password-protected PRG-RAM | None | None | 5 KB (password-gated) | ✅ | Registers at $7EF0–$7EFF. RAM split into three independently password-protected regions. CHR mode bit swaps 2 KB and 1 KB halves. PRG bank = write-value >> 2. No deltas. |
| 129 | Duplicate assignment (Mapper 58 alias) | Same as mapper 58 | Same as mapper 58 | Same as mapper 58 | None | None | None | ✅ | NesDev marks mapper 129 as duplicate assignment of mapper 58; routed to mapper 58 implementation in factory. |
| 132 | TXC 22111 / UNL-22211 | 32 KB switchable (selected by output bit 2) | 8 KB switchable (selected by output bits 1:0) | Fixed from header | None | None | None | ✅ | Register window decoded via `$E103` mask in `$4100-$4FFF`, status readback at `$4100` (`S xor V` + `RRR`), and bank output applied on `$8000` writes per NesDev. |
| 205 | BMC-JC-016-2 (MMC3 multicart) | MMC3 8 KB PRG banks constrained by outer block register | MMC3 1 KB CHR banks constrained by outer block register | MMC3 H / V | MMC3 scanline | None | None | ✅ | Outer block register at `$6000-$7FFF` (`MM`) selects PRG/CHR `AND/OR` masks per NesDev table. No PRG-RAM window at `$6000-$7FFF`. |
| 206 | DxROM / Namco 108/118 / Tengen MIMIC-1 | 8 KB × 2 switchable + 2 banks always fixed to last | 2 KB × 2 + 1 KB × 4 switchable; CHR layout fixed | Fixed H/V or 4-screen (Gauntlet / DRROM) | **None** | None | **None** | ⚠️ | **Namco 108 errata: write to $0000–$1FFF while CPU is executing from $8000–$9FFF can cause a spurious bankswitch.** Submapper 1 (3407/3417/3451 PCBs with unbanked 32 KB PRG) should bypass PRG banking. No IRQ, no WRAM registers at $A000–$FFFF. Ensure those registers are absent (not inherited from MMC3 code). |

---

## Unimplemented Mappers of Note

The following mappers are **not** present in neser's `mapper_registry!` and represent the most
commonly-played or interesting gaps. The full NesDev mapper list is at
<https://www.nesdev.org/wiki/Mapper>.

| # | Hardware Name | Notable Games | Description / Why it Matters |
|---|---------------|---------------|-------------------------------|
| 48 | Taito TC0350/TC8521 | Flintstones 2, Jajamaru, Kyonshis | MMC3-like with scanline IRQ; often mislabelled as mapper 33 |
| 57 | Multicart | Game 168-in-1, etc. | Simple multicart; two-register design |
| 58 | Multicart | Game 68-in-1 K-3046 | Single-register multicart (PRG+CHR combined) |
| 60 | Multicart | Reset-based 4-in-1 | Reset-based bank switching |
| 61 | Multicart | 20-in-1 | 5-bit PRG register, 1-bit CHR+mirroring |
| 62 | Multicart | Super 700-in-1 | 6-bit outer+inner VRC4-style banks |
| 64 | Tengen RAMBO-1 | Gauntlet, Klax, Skull & Crossbones | MMC3-like with extra CHR bank mode and scanline IRQ variant |
| 65 | Irem H-3001 | Daiku no Gen San 2 | 16 KB PRG switchable, 8 × 1 KB CHR, CPU-cycle IRQ |
| 67 | Sunsoft-3 (FME-3?) | Fantasy Zone II | 16 KB PRG × 2 switchable, 4 × 2 KB CHR, scanline IRQ |
| 75 | Konami VRC1 | Exciting Boxing, Tetris (J), King Kong 2 | 8 KB PRG × 2 + fixed, 4 KB CHR × 2, switchable H/V |
| 76 | Namco 109 | Megami Tensei | MMC3/206 variant with 2 KB CHR inflated to 2 KB |
| 79 | NINA-03/06 | AVE Nina games, Tiles of Fate | Simple: 3-bit PRG + 4-bit CHR register at $4100 |
| 85 | Konami VRC7 | Lagrange Point | 8 KB PRG × 3 + fixed, 8 × 1 KB CHR, FM audio (YM2413 OPLL) |
| 86 | Jaleco JF-13 | Moero!! Pro Yakyuu | Similar to mapper 101; 3-bit CHR + 2-bit PRG |
| 87 | Konami/Jaleco (simple CHR) | Goonies, City Connection | 2-bit CHR bank, fixed 32 KB PRG |
| 88 | Namco 118 | Youkai Douchuuki, Devil Man | Mapper 206 variant with 128 KB CHR (A12 wired to CHR A16) |
| 93 | Sunsoft-2 (Fantasy Zone) | Fantasy Zone (J) | UxROM-like with CHR-ROM instead of CHR-RAM |
| 94 | Senjou no Ookami / Capcom | Senjou no Ookami | UNROM clone with register at $8000, bus conflicts |
| 95 | Namco 118 1-screen variant | Dragon Buster | Mapper 206 + CHR A15 selects CIRAM A10 |
| 97 | Irem TAM-S1 | Kaiketsu Yanchamaru | 32 KB PRG switchable, hardwired mirroring |
| 105 | NES-EVENT (MMC1 variant) | Nintendo World Championships 1990 | MMC1 variant with additional timer/counter |
| 112 | NTDEC multicart | Asder 20-in-1 | 4-bit PRG and CHR multi-register set |
| 113 | Multicart (NINA-03 style) | HES 6-in-1, Sachen Glorys | PRG+CHR register at $4100/$6000 |
| 118 | TxSROM (MMC3 variant) | Ys, NES Play Action Football | MMC3 + CHR A17 selects CIRAM A10 |
| 119 | TQROM | Pin Bot, High Speed | MMC3 + CHR mixed ROM+RAM banks |
| 152 | Bandai (FCG-like, 1-screen) | Gegege no Kitarou 2 | Mapper 70 variant with 1-screen mirroring |
| 153 | Bandai (LZ93D50 + WRAM) | Famicom Jump II | Mapper 16 variant with 8 KB WRAM instead of EEPROM |
| 154 | Namco 118 + 1-screen | Devil Man | Mapper 88 + CIRAM A10 control |
| 157 | Bandai Datach | Dragon Ball Z series (Datach) | Mapper 16 + barcode reader interface |
| 159 | Bandai (LZ93D50 + 24C01) | SD Gundam Gaiden | Mapper 16 variant with 128-byte EEPROM |
| 163 | Nanjing (FC protection) | Diablo knockoff, etc. | Complex protection registers |
| 180 | Nihon Bussan (UNROM variant) | Crazy Climber | UNROM-like with fixed low bank, switchable high bank |
| 184 | Sunsoft-1 | Atlantis no Nazo (some), Wing of Madoola | Fixed 16 KB PRG, 2 × 4 KB CHR switchable |
| 185 | CNROM + CHR protection | Spy vs Spy, others | CNROM with CHR-disable bits (anti-piracy) |
| 189 | TXC (PCB 01-22000-400) | Armadillo | MMC3-like with external PRG register |
| 232 | Camerica BF9096 (Quattro games) | Quattro Adventure, etc. | Mapper 71 with additional outer bank register |

---

## Key Compatibility Notes

### Expansion Audio Summary

| Mapper | Chip | Status |
|--------|------|--------|
| 5 | MMC5 8-ch PCM | Implemented |
| 19 | Namco 163 wavetable | Implemented |
| 24 / 26 | VRC6 pulse + sawtooth | Implemented |
| 69 | Sunsoft 5B (YM2149) | **Not implemented** → Gimmick! music broken |
| 85 | VRC7 FM (YM2413) | Not implemented (mapper itself not supported) |

### Save Game (EEPROM/SRAM) Status

| Mapper | Save Type | Status |
|--------|-----------|--------|
| 1 | PRG-RAM battery-backed | Implemented |
| 4 | PRG-RAM battery-backed | Implemented |
| 16 (sub 5) | I²C 24C02 EEPROM | **Not implemented** – DBZ II/III saves broken |
| 69 | Switchable PRG-RAM | Implemented (ROM/RAM select bit) |

---

*Last updated: cross-referenced against NesDev wiki July 2025.*
