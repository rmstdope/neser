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
| 206 | Namco 118/108 | | x | | | 8 | 8 | 1 |

### Notes

- **\*** Mapper 34 `has_chr_banking` is dynamic — true for NINA-001 sub-variant, false for BNROM.
- **\*\*** Mapper 68 `max_prg_ram_kb` is dynamic — depends on the cartridge header's PRG-RAM bank count.
- VRC2/VRC4 (mappers 21–25) share one implementation. VRC2a (mapper 22) lacks an IRQ counter; VRC4 variants have one.
- VRC6 mappers 24 and 26 differ only in address line swapping; capabilities are identical.

## Feature Summary

| Feature | Mappers |
| --------- | --------- |
| IRQ | 4, 5, 16, 19, 21, 23, 24, 25, 26, 69 |
| CHR Banking | 1, 3, 4, 5, 9, 10, 11, 13, 16, 19, 21, 22, 23, 24, 25, 26, 29, 34*, 66, 68, 69, 78, 206 |
| Dynamic Mirroring | 1, 4, 5, 7, 9, 10, 15, 16, 19, 21, 22, 23, 24, 25, 26, 68, 69, 71, 78 |
| Expansion Audio | 5, 19, 24, 26 |
