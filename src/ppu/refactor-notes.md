# PPU Refactor Notes

## Smells and Risks
- Monolithic per-cycle logic in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L33-L520) mixes timing, VBlank, background fetch, sprite evaluation, rendering, and sprite-0 hit detection, which makes it hard to reason about correctness and isolate fixes.
- Color emphasis and grayscale logic is duplicated between runtime rendering in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L437-L490) and helper logic in [src/ppu/rendering.rs](src/ppu/rendering.rs#L45-L139), increasing drift risk.
- Repeated mapper A12 priming logic appears in PPU address write, PPUDATA read, and PPUDATA write paths in [src/ppu/ppu.rs](src/ppu/ppu.rs#L300-L446), creating duplication and potential divergence.
- PPU implementation and extensive test fixtures live in the same file; the test module starts in [src/ppu/ppu.rs](src/ppu/ppu.rs#L855) and includes large integration-style tests such as [src/ppu/ppu.rs](src/ppu/ppu.rs#L2105-L2330), which bloats the module and complicates navigation.
- Magic numbers for scanline and dot timing are scattered in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L92-L170), making timing behavior harder to audit and align with specs.

## Refactoring Opportunities
- Extract `tick` phases into focused helpers (timing/frame wrap, vblank/NMI, background pipeline, sprite evaluation, pixel composition) and keep a small coordinator in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L33-L520).
- Centralize color emphasis and grayscale handling into a shared helper (e.g., `ppu::color` or `Rendering`) and reuse it in runtime and test code; see duplication in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L437-L490) and [src/ppu/rendering.rs](src/ppu/rendering.rs#L45-L139).
- Introduce a small helper for A12 priming and mapper notification to remove duplication in [src/ppu/ppu.rs](src/ppu/ppu.rs#L300-L446) (e.g., `notify_mapper_address_change(old, new)` or `prime_a12_filter(old, new)`).
- Move large rendering alignment tests to a dedicated test module (e.g., `tests/ppu_rendering.rs` or a `src/ppu/tests/` submodule) and extract a shared iNES ROM builder to reduce duplication in [src/ppu/ppu.rs](src/ppu/ppu.rs#L2105-L2330).
- Define timing constants (vblank start/end scanlines, pre-render scanline per TV system, sprite eval windows) to replace hard-coded values in [src/ppu/ppu/tick.rs](src/ppu/ppu/tick.rs#L92-L170).

## Testing Gaps
- Mid-scanline grayscale retroactive updates are not directly exercised; add coverage around `track_recent_pixel` and `apply_grayscale_to_recent_pixels` in [src/ppu/ppu.rs](src/ppu/ppu.rs#L214-L238).
- Palette mirroring edge cases ($3F10/$3F14/$3F18/$3F1C) lack explicit tests; see `mirror_palette_address` in [src/ppu/memory.rs](src/ppu/memory.rs#L225-L234).
- I/O bus decay timing does not appear to be tested; consider unit tests for bit decay in [src/ppu/registers.rs](src/ppu/registers.rs#L100-L119).
- Rendering-glitch PPUDATA address increments are untested; add tests for the `inc_address_with_rendering_glitch` path in [src/ppu/ppu.rs](src/ppu/ppu.rs#L374-L432) and the guard in [src/ppu/ppu.rs](src/ppu/ppu.rs#L549-L554).
- Sprite overflow bug path lacks targeted coverage; add a focused test for the overflow branch in [src/ppu/sprites.rs](src/ppu/sprites.rs#L122-L170).
