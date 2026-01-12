# TODO

## Next priorities (post-Blargg green)

- <input type="checkbox" checked disabled> Add an automated `nestest.nes` “golden trace” test (CPU regs + PC + flags per instruction) to catch subtle CPU regressions.
- <input type="checkbox" disabled> Build a PPU “pixel hash” regression harness (render N frames, hash framebuffer) for a small curated ROM set.
- <input type="checkbox" disabled> Implement MMC5 next-tier features (in order): mirroring (`$5105`), ExRAM (`$5C00-$5FFF`), scanline IRQ (`$5203/$5204`), then CHR banking/split behavior.
- <input type="checkbox" checked disabled> Expand Blargg PPU coverage by wiring in more ROMs from `blargg_ppu_tests_2005.09.15b/` and additional `ppu_vbl_nmi` variants.
- <input type="checkbox" disabled> Add CI-friendly test profiling: split long-running ROM tests into a separate “slow” test group.
- <input type="checkbox" disabled> Improve APU correctness beyond reset/mixer: frame counter edge cases, DMC sample fetching + IRQ behavior, channel enable timing; add an “audio signature” regression test (sample checksum over N frames).
- <input type="checkbox" disabled> Improve mapper correctness for common commercial titles: MMC1 quirks, MMC3 IRQ/A12 filtering details, PRG-RAM persistence semantics.
- <input type="checkbox" disabled> Add a dedicated “DMA torture” regression set: OAM DMA odd/even alignment, CPU dummy reads/writes interactions, DMC DMA overlaps, verify cycle stealing effects on CPU instruction timing.
- <input type="checkbox" disabled> Implement save-state support (CPU/PPU/APU + mapper state + RAM) for faster and reproducible debugging.
- <input type="checkbox" disabled> Create a compatibility matrix for `roms/games/*` and add one smoke test per game (boot to title + basic input sanity), logging a short failure signature.
