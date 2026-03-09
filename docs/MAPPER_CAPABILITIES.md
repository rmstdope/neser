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
| 7 | AxROM | | | x | | 8 | 32 | 8 |
| 9 | MMC2 | | x | x | | 8 | 8 | 4 |
| 10 | MMC4 | | x | x | | 8 | 16 | 4 |
| 11 | ColorDreams | | x | | | 8 | 32 | 8 |
| 13 | CPROM | | x | | | 8 | 32 | 4 |
| 15 | Multicart 15 | | | x | | 8 | 8 | 8 |
| 16 | Bandai FCG | x | x | x | | 0 | 16 | 1 |
| 18 | Jaleco SS 88006 | x | x | x | | 0 | 8 | 1 |
| 19 | Namco 163 | x | x | x | x | 8 | 8 | 1 |
| 21 | VRC4a/VRC4c | x | x | x | | 8 | 8 | 1 |
| 22 | VRC2a | | x | x | | 8 | 8 | 1 |
| 23 | VRC2b/VRC4e | x | x | x | | 8 | 8 | 1 |
| 24 | VRC6a | x | x | x | x | 8 | 8 | 1 |
| 25 | VRC4b/VRC4d | x | x | x | | 8 | 8 | 1 |
| 26 | VRC6b | x | x | x | x | 8 | 8 | 1 |
| 29 | Sealie Computing | | x | | | 8 | 16 | 8 |
| 34 | BNROM/NINA-001 | | * | | | 8 | 32 | 8 |
| 66 | GxROM | | x | | | 8 | 32 | 8 |
| 68 | Sunsoft-4 | | x | x | | ** | 16 | 2 |
| 69 | Sunsoft FME-7 | x | x | x | | 8 | 8 | 1 |
| 71 | Camerica | | | x | | 8 | 16 | 8 |
| 78 | Holy Diver/Cosmo Carrier | | x | x | | 8 | 16 | 8 |
| 87 | Jaleco/Konami CHR-only | | x | | | 0 | 32 | 8 |
| 206 | Namco 118/108 | | x | | | 8 | 8 | 1 |
| 343 | Reset-based 4-in-1 multicart | | x | | | 8 | 16 | 8 |
| 344 | BMC-GN-26 | x | x | x | | 8 | 8 | 1 |
| 346 | Kaiser KS7012 | | | | | 0 | 32 | 8 |
| 347 | Kaiser UNL-KS7030 | | | x | | 8 | 4 | 8 |

### Notes

- **\*** Mapper 34 `has_chr_banking` is dynamic — true for NINA-001 sub-variant, false for BNROM.
- **\*\*** Mapper 68 `max_prg_ram_kb` is dynamic — depends on the cartridge header's PRG-RAM bank count.
- VRC2/VRC4 (mappers 21–25) share one implementation. VRC2a (mapper 22) lacks an IRQ counter; VRC4 variants have one.
- VRC6 mappers 24 and 26 differ only in address line swapping; capabilities are identical.


## Additional Implemented Mappers (Coverage List)

The following mapper IDs are implemented in `mapper_registry!` but are not yet split out
with dedicated per-mapper capability rows above. They are listed here so every implemented
mapper is represented in this document.

| # | Registry Constructor |
| --: | -------------------- |
| 6 | `SuperMagicCardMapper::new` |
| 8 | `SuperMagicCardMapper::new` |
| 12 | `Mapper12::new` |
| 14 | `Mapper14::new` |
| 17 | `SuperMagicCardMapper::new` |
| 20 | `Mapper20::new` |
| 27 | `Vrc2Vrc4Mapper::new` |
| 28 | `Mapper28::new` |
| 30 | `Mapper30::new` |
| 31 | `Mapper31::new` |
| 32 | `IremG101Mapper::new` |
| 33 | `TaitoTc0190Mapper::new` |
| 35 | `Mapper35::new` |
| 36 | `Mapper36::new` |
| 37 | `Mapper37::new` |
| 38 | `Mapper38::new` |
| 39 | `Mapper39::new` |
| 40 | `Ntdec2722Mapper::new` |
| 41 | `Mapper41::new` |
| 42 | `Mapper42::new` |
| 43 | `Mapper43::new` |
| 44 | `Mapper44::new` |
| 45 | `Mapper45::new` |
| 46 | `Mapper46::new` |
| 47 | `Mapper47::new` |
| 48 | `Mapper48::new` |
| 49 | `Mapper49::new` |
| 50 | `Mapper50::new` |
| 51 | `Mapper51::new` |
| 52 | `Mapper52::new` |
| 53 | `Mapper53::new` |
| 54 | `Mapper54::new` |
| 55 | `Mapper55::new` |
| 56 | `Mapper56::new` |
| 57 | `Mapper57::new` |
| 58 | `Mapper58::new` |
| 59 | `Mapper59::new` |
| 60 | `Mapper60::new` |
| 61 | `Mapper61::new` |
| 62 | `Mapper62::new` |
| 63 | `Mapper63::new` |
| 64 | `Mapper64::new` |
| 65 | `Mapper65::new` |
| 67 | `Mapper67::new` |
| 70 | `Mapper70::new` |
| 72 | `Mapper72::new` |
| 73 | `Mapper73::new` |
| 74 | `Mapper74::new` |
| 75 | `Mapper75::new` |
| 76 | `Mapper76::new` |
| 77 | `Mapper77::new` |
| 79 | `Mapper79::new` |
| 80 | `Mapper80::new` |
| 81 | `Mapper81::new` |
| 82 | `Mapper82::new` |
| 83 | `Mapper83::new` |
| 84 | `Ntdec2722Mapper::new` |
| 85 | `VRC7Mapper::new` |
| 86 | `Mapper86::new` |
| 88 | `Mapper88::new` |
| 90 | `Mapper90::new` |
| 91 | `Mapper91::new` |
| 93 | `Mapper93::new` |
| 129 | `Mapper58::new` |
| 132 | `Mapper132::new` |
| 133 | `Mapper133::new` |
| 140 | `Mapper140::new` |
| 155 | `MMC1Mapper::new` |
| 185 | `Mapper185::new` |
| 205 | `Mapper205::new` |
| 241 | `Mapper241::new` |
| 242 | `Mapper242::new` |
| 243 | `Mapper243::new` |
| 244 | `Mapper244::new` |
| 245 | `Mapper245::new` |
| 246 | `Mapper246::new` |
| 251 | `Mapper251::new` |
| 254 | `Mapper254::new` |
| 255 | `Mapper255::new` |
| 302 | `Mapper302::new` |
| 324 | `Mapper324::new` |
| 326 | `Mapper326::new` |
| 327 | `Mapper327::new` |
| 328 | `Mapper328::new` |
| 330 | `Mapper330::new` |
| 332 | `Mapper332::new` |
| 335 | `Mapper335::new` |
| 339 | `Mapper339::new` |
| 340 | `Mapper340::new` |
| 341 | `Mapper341::new` |
| 342 | `Mapper342::new` |
| 345 | `Mapper345::new` |
| 348 | `Mapper348::new` |
| 349 | `Mapper349::new` |
| 350 | `Mapper350::new` |

## Feature Summary

| Feature | Mappers |
| --------- | --------- |
| IRQ | 4, 5, 16, 18, 19, 21, 23, 24, 25, 26, 69, 344 |
| CHR Banking | 1, 3, 4, 5, 9, 10, 11, 13, 16, 18, 19, 21, 22, 23, 24, 25, 26, 29, 34*, 66, 68, 69, 78, 87, 206, 344 |
| Dynamic Mirroring | 1, 4, 5, 7, 9, 10, 15, 16, 18, 19, 21, 22, 23, 24, 25, 26, 68, 69, 71, 78, 344, 347 |
| Expansion Audio | 5, 19, 24, 26 |
