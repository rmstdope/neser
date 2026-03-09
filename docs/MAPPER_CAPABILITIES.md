# Mapper Capabilities Matrix

This document lists the hardware capabilities reported by each supported mapper via the `MapperCapabilities` struct. The data is sourced from each mapper's `capabilities()` implementation.

## Capability Fields

| Field | Type | Description |
| ------- | ------ | ------------- |
| `has_irq` | `bool` | Mapper has a scanline/cycle counter that can trigger IRQs |
| `has_chr_banking` | `bool` | Mapper supports CHR bank switching |
| `has_dynamic_mirroring` | `bool` | Mapper can change nametable mirroring at runtime |
| `has_expansion_audio` | `bool` | Mapper provides additional audio channels |
| `max_prg_ram_kb` | `usize` | Maximum PRG-RAM (WRAM/SRAM) size in KB |
| `prg_bank_size_kb` | `usize` | Smallest switchable PRG-ROM bank size in KB |
| `chr_bank_size_kb` | `usize` | Smallest switchable CHR bank size in KB |

## Composable Mapper Traits

Mapper behavior is now also exposed through smaller trait concerns in `src/cartridge/mapper.rs`:

- `MapperCore` — required PRG/CHR I/O and mirroring contract.
- `MapperIrq` — optional IRQ signaling/clocking behavior.
- `MapperPpuExtension` — optional PPU-driven hooks (A12/scanline style integration).
- `MapperAudio` — optional expansion-audio sample hook.
- `MapperStateSnapshot` — optional WRAM/save-state snapshot and restore contract.
- `MapperComposable` — convenience bound for `MapperCore + MapperStateSnapshot`.

Current explicit trait-split adoption for issue #535 proof-of-concept:

- `NROM` (mapper 0): `MapperCore + MapperStateSnapshot`
- `MMC3` (mapper 4): `MapperCore + MapperIrq + MapperPpuExtension + MapperStateSnapshot`
- `VRC6` (mappers 24/26): `MapperCore + MapperIrq + MapperAudio + MapperStateSnapshot`

Existing runtime behavior through `Mapper` remains unchanged.

## Capabilities by Mapper

| # | Name | IRQ | CHR Bank | Dyn Mirror | Exp Audio | PRG-RAM (KB) | PRG Bank (KB) | CHR Bank (KB) |
| --: | ------ | :---: | :--------: | :----------: | :---------: | :------------: | :-------------: | :-------------: |
| 0 | NROM | | | | | 8 | 32 | 8 |
| 1 | MMC1 | | x | x | | 8 | 16 | 4 |
| 2 | UxROM | | | | | 8 | 16 | 8 |
| 3 | CNROM | | x | | | 8 | 32 | 8 |
| 4 | MMC3 | x | x | x | | 8 | 8 | 1 |
| 5 | MMC5 | x | x | x | x | 64 | 8 | 1 |
| 6 | SuperMagicCardMapper  | x | x | x |  | 0 | 8 | 8 |
| 7 | AxROM | | | x | | 8 | 32 | 8 |
| 8 | SuperMagicCardMapper  | x | x | x |  | 0 | 8 | 8 |
| 9 | MMC2 | | x | x | | 8 | 8 | 4 |
| 10 | MMC4 | | x | x | | 8 | 16 | 4 |
| 11 | ColorDreams | | x | | | 8 | 32 | 8 |
| 12 | SL-5020B (MMC3 + CHR A18 extension) | x | x | x |  | 8 | 8 | 1 |
| 13 | CPROM | | x | | | 8 | 32 | 4 |
| 14 | SL1632 (MMC3/VRC2 hybrid) | x | x | x |  | 8 | 8 | 1 |
| 15 | Multicart 15 | | | x | | 8 | 8 | 8 |
| 16 | Bandai FCG | x | x | x | | 0 | 16 | 1 |
| 17 | SuperMagicCardMapper  | x | x | x |  | 0 | 8 | 8 |
| 18 | Jaleco SS 88006 | x | x | x | | 0 | 8 | 1 |
| 19 | Namco 163 | x | x | x | x | 8 | 8 | 1 |
| 20 | Famicom Disk System (FDS) | x |  |  | x | 8 | 32 | 8 |
| 21 | VRC4a/VRC4c | x | x | x | | 8 | 8 | 1 |
| 22 | VRC2a | | x | x | | 8 | 8 | 1 |
| 23 | VRC2b/VRC4e | x | x | x | | 8 | 8 | 1 |
| 24 | VRC6a | x | x | x | x | 8 | 8 | 1 |
| 25 | VRC4b/VRC4d | x | x | x | | 8 | 8 | 1 |
| 26 | VRC6b | x | x | x | x | 8 | 8 | 1 |
| 27 | Vrc2Vrc4Mapper  |  | x | x |  | 8 | 8 | 1 |
| 28 | Action 53 (homebrew multicart) |  | x | x |  | 0 | 16 | 8 |
| 29 | Sealie Computing | | x | | | 8 | 16 | 8 |
| 30 | UNROM-512 (homebrew) |  | x | x |  | 0 | 16 | 8 |
| 31 | NSF playing cart / 8-in-1 (Homebrew) |  | x |  |  | 0 | 4 | 8 |
| 32 | Irem G-101 |  | x |  |  | 0 | 8 | 1 |
| 33 | TaitoTc0190Mapper  |  | x | x |  | 8 | 8 | 1 |
| 34 | BNROM/NINA-001 | | * | | | 8 | 32 | 8 |
| 35 | J.Y. Company (simple variant) | x | x | x |  | 8 | 8 | 1 |
| 36 | TXC 01-22000-400 (PCM063 family) |  | x |  |  | 0 | 32 | 8 |
| 37 | ZZ board MMC3 multicart (Super Mario Bros + Tetris + Nintendo World Cup) | x | x | x |  | 0 | 8 | 1 |
| 38 | Mapper 38 |  | x |  |  | 0 | 32 | 8 |
| 39 | BMC-STUDYNGAME (Study and Game 32-in-1) |  |  |  |  | 0 | 32 | 8 |
| 40 | NTDEC 2722 (Super Mario Bros. 2 Japanese) | x |  |  |  | 8 | 8 | 8 |
| 41 | Caltron 6-in-1 |  | x | x |  | 0 | 32 | 8 |
| 42 | FDS Conversions (Ai Senshi Nicol, Bio Miracle Bokutte Upa) | x | x | x |  | 8 | 8 | 8 |
| 43 | TONY-I / YS-612 (Super Mario Bros. 2 FDS conversion) | x |  |  |  | 8 | 8 | 8 |
| 44 | MMC3 multicart (Super HIK 7-in-1 and others) | x | x | x |  | 8 | 8 | 1 |
| 45 | MMC3 multicart with sequential outer bank registers | x | x | x |  | 8 | 8 | 1 |
| 46 | Rumble Station (Color Dreams multicart) |  | x |  |  | 0 | 32 | 8 |
| 47 | MMC3 multicart (Super Spike V'Ball + Nintendo World Cup) | x | x | x |  | 0 | 8 | 1 |
| 48 | Mapper 48 | x | x | x |  | 8 | 8 | 1 |
| 49 | MMC3 multicart (Super HIK 4-in-1 and others) | x | x | x |  | 0 | 8 | 1 |
| 50 | N-32 (Romeo / Super Mario Bros. 2 Japanese conversion) | x |  |  |  | 8 | 8 | 8 |
| 51 | 11-in-1 Ball Games |  |  | x |  | 8 | 8 | 8 |
| 52 | BMC Realtec 8213 MMC3 multicart | x | x | x |  | 8 | 8 | 1 |
| 53 | Supervision 16-in-1 |  |  | x |  | 0 | 16 | 8 |
| 54 | Novel Diamond |  | x |  |  | 8 | 32 | 8 |
| 55 | Mapper 55 | x |  | x |  | 8 | 8 | 8 |
| 56 | Kaiser KS202 (Pirate SMB3) | x | x | x |  | 8 | 8 | 1 |
| 57 | BMC GK-192 multicart |  | x | x |  | 0 | 16 | 8 |
| 58 | BMC multicart (address latch) |  | x | x |  | 0 | 16 | 8 |
| 59 | BMC-T3H53 / BMC-D1038 |  | x | x |  | 0 | 16 | 8 |
| 60 | Reset-based NROM-128 4-in-1 |  | x |  |  | 8 | 16 | 8 |
| 61 | NTDEC address latch multicart |  |  | x |  | 8 | 16 | 8 |
| 62 | Super 700-in-1 (address latch + data latch) |  | x | x |  | 0 | 16 | 8 |
| 63 | BMC multi-game bank switching |  |  | x |  | 0 | 16 | 8 |
| 64 | Tengen RAMBO-1 | x | x | x |  | 8 | 8 | 1 |
| 65 | Irem H3001 | x | x | x |  | 8 | 8 | 1 |
| 66 | GxROM | | x | | | 8 | 32 | 8 |
| 67 | Mapper 67 | x | x | x |  | 8 | 16 | 2 |
| 68 | Sunsoft-4 | | x | x | | ** | 16 | 2 |
| 69 | Sunsoft FME-7 | x | x | x | | 8 | 8 | 1 |
| 70 | Mapper 70 |  | x |  |  | 0 | 16 | 8 |
| 71 | Camerica | | | x | | 8 | 16 | 8 |
| 72 | Jaleco JF-17 |  | x |  |  | 0 | 16 | 8 |
| 73 | Konami VRC3 | x |  |  |  | 8 | 16 | 8 |
| 74 | MMC3 variant with CHR-RAM at banks 8–9 (Chinese pirate boards) | x | x | x |  | 8 | 8 | 1 |
| 75 | Konami VRC1 |  | x | x |  | 0 | 8 | 4 |
| 76 | Namco 109 (Megami Tensei) |  | x |  |  | 8 | 8 | 2 |
| 77 | IREM NINA-03 (Napoleon Senki) |  | x |  |  | 8 | 32 | 4 |
| 78 | Holy Diver/Cosmo Carrier | | x | x | | 8 | 16 | 8 |
| 79 | NINA-03/NINA-06 |  | x |  |  | 8 | 32 | 8 |
| 80 | Taito X1-005 |  | x | x |  | 8 | 8 | 1 |
| 81 | Mapper 81 |  | x |  |  | 0 | 16 | 8 |
| 82 | Mapper 82 |  | x | x |  | 8 | 8 | 1 |
| 83 | Mapper 83 | x | x | x |  | 8 | 8 | 1 |
| 84 | NTDEC 2722 (Super Mario Bros. 2 Japanese) | x |  |  |  | 8 | 8 | 8 |
| 85 | Konami VRC7 | x | x | x | x | 0 | 8 | 1 |
| 86 | Mapper 86 |  | x |  | x | 0 | 32 | 8 |
| 87 | Jaleco/Konami CHR-only | | x | | | 0 | 32 | 8 |
| 88 | Namco 118 (Namco 108 chip, CHR A12 wired to CHR A16) |  | x |  |  | 8 | 8 | 1 |
| 90 | Mapper 90 | x | x | x |  | 8 | 8 | 1 |
| 91 | Mapper 91 | x | x |  |  | 8 | 8 | 2 |
| 93 | Mapper 93 |  |  |  |  | 0 | 16 | 8 |
| 129 | BMC multicart (address latch) |  | x | x |  | 0 | 16 | 8 |
| 132 | TXC 22111 / UNL-22211 |  | x |  |  | 0 | 32 | 8 |
| 133 | Sachen 72008 / UNL-SA-72008 |  | x |  |  | 0 | 32 | 8 |
| 140 | Jaleco JF-11/JF-14 |  | x |  |  | 0 | 32 | 8 |
| 155 | MMC1 (SxROM) |  | x | x |  | 0 | 16 | 4 |
| 185 | CNROM with CHR-ROM enable gating (chip select) |  |  |  |  | 8 | 32 | 8 |
| 205 | MMC3-based multicart (BMC-JC-016-2) | x | x | x |  | 8 | 8 | 1 |
| 206 | Namco 118/108 | | x | | | 8 | 8 | 1 |
| 241 | BxROM variant (150-in-1) |  |  |  |  | 8 | 32 | 8 |
| 242 | 43272 (address-latch PRG switch with mirroring control) |  |  | x |  | 8 | 32 | 8 |
| 243 | Sachen 74LS374N (SA-020A) |  | x | x |  | 0 | 32 | 8 |
| 244 | Decathlon (C-N22M) |  | x |  |  | 8 | 32 | 8 |
| 245 | Waixing MMC3 variant |  |  |  |  | 8 | 8 | 1 |
| 246 | Fong Shen Bang |  | x |  |  | 8 | 8 | 2 |
| 251 | Alias of Mapper 45 (Nestopia assignment) |  |  |  |  | 8 | 32 | 8 |
| 254 | Pikachu Y2K (MMC3 variant with copy protection) |  | x |  |  | 8 | 8 | 1 |
| 255 | 110-in-1 Multicart |  | x | x |  | 0 | 16 | 8 |
| 302 | Mapper 302 |  |  | x |  | 8 | 2 | 8 |
| 324 | Farid UNROM-8 |  |  |  |  | 0 | 16 | 8 |
| 326 | Mapper 326 |  |  |  |  | 0 | 8 | 1 |
| 327 | Mapper 327 |  |  |  |  | 0 | 32 | 8 |
| 328 | Mapper 328 |  |  |  |  | 0 | 16 | 2 |
| 330 | Mapper 330 |  |  |  |  | 8 | 32 | 8 |
| 332 | Super 40-in-1 (BMC-WS) |  | x | x |  | 0 | 16 | 8 |
| 335 | BMC-CTC-09 (10-in-1 multicart) |  | x | x |  | 0 | 16 | 8 |
| 339 | BMC-K-3006 (MMC3+NROM multicart) |  |  |  |  | 8 | 32 | 8 |
| 340 | BMC-K-3036 (35-in-1 multicart) |  |  | x |  | 0 | 16 | 8 |
| 341 | BMC-TJ-03 |  | x | x |  | 8 | 16 | 8 |
| 342 | COOLGIRL multicart (minimal baseline) |  |  |  |  | * | 32 | 8 |
| 343 | Reset-based 4-in-1 multicart | | x | | | 8 | 16 | 8 |
| 344 | BMC-GN-26 | x | x | x | | 8 | 8 | 1 |
| 345 | BMC-L6IN1 | x | x | x |  | 8 | 8 | 1 |
| 346 | Kaiser KS7012 | | | | | 0 | 32 | 8 |
| 347 | Kaiser UNL-KS7030 | | | x | | 8 | 4 | 8 |
| 348 | BMC-830118-C (MMC3 variant) | x | x | x |  | 0 | 8 | 1 |
| 349 | BMC G-146 multicart |  |  | x |  | 0 | 16 | 8 |
| 350 | BMC-891227 multicart |  |  | x |  | 0 | 16 | 8 |
### Notes

- **\*** Mapper 34 `has_chr_banking` is dynamic — true for NINA-001 sub-variant, false for BNROM.
- **\*\*** Mapper 68 `max_prg_ram_kb` is dynamic — depends on the cartridge header's PRG-RAM bank count.
- VRC2/VRC4 (mappers 21–25) share one implementation. VRC2a (mapper 22) lacks an IRQ counter; VRC4 variants have one.
- VRC6 mappers 24 and 26 differ only in address line swapping; capabilities are identical.



## Feature Summary

| Feature | Mappers |
| --------- | --------- |
| IRQ | 4, 5, 6, 8, 12, 14, 16, 17, 18, 19, 20, 21, 23, 24, 25, 26, 35, 37, 40, 42, 43, 44, 45, 47, 48, 49, 50, 52, 55, 56, 64, 65, 67, 69, 73, 74, 83, 84, 85, 90, 91, 205, 344, 345, 348 |
| CHR Banking | 1, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 35, 36, 37, 38, 41, 42, 44, 45, 46, 47, 48, 49, 52, 54, 56, 57, 58, 59, 60, 62, 64, 65, 66, 67, 68, 69, 70, 72, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 85, 86, 87, 88, 90, 91, 129, 132, 133, 140, 155, 205, 206, 243, 244, 246, 254, 255, 332, 335, 341, 343, 344, 345, 348 |
| Dynamic Mirroring | 1, 4, 5, 6, 7, 8, 9, 10, 12, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 30, 33, 35, 37, 41, 42, 44, 45, 47, 48, 49, 51, 52, 53, 55, 56, 57, 58, 59, 61, 62, 63, 64, 65, 67, 68, 69, 71, 74, 75, 78, 80, 82, 83, 85, 90, 129, 155, 205, 242, 243, 255, 302, 332, 335, 340, 341, 344, 345, 347, 348, 349, 350 |
| Expansion Audio | 5, 19, 20, 24, 26, 85, 86 |
