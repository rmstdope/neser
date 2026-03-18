---
name: mapper-verification-roms
description: Create and update mapper verification ROMs in roms/automated_tests/mapper_verification using shared tests, per-mapper defs/configs, and specification-driven behavior.
---

# Mapper Verification ROMs

## Introduction

Use this skill every time you create or update a ROM under `roms/automated_tests/mapper_verification/`. This ROM suite is configuration-driven: shared test behavior belongs in `tests/` and `common/`, while mapper-specific behavior belongs in `defs/` and `configs/`. Treat these ROMs as hardware-verification artifacts, not emulator-behavior snapshots.

## Instructions

1. Start from specification, never from implementation.
   - If mapper behavior is unclear, invoke `nes-hardware-research`.
   - For verification-ROM work, use only the NESdev/wiki-backed specification results from that research.
   - Never use this emulator's source code or any emulator implementation as the reference for ROM behavior.

2. Reuse before adding new assembly.
   - Check whether an existing aspect in `tests/test_*.s` already covers the behavior.
   - Prefer extending shared tests, shared macros, or capability flags over creating mapper-specific copies.

3. Keep responsibilities separated.
   - Put reusable test flow, assertions, and reporting in `common/` or `tests/`.
   - Put mapper register interfaces, capability flags, and mapper-specific constants in `defs/mNNN.S_defs.inc`.
   - Put PRG/CHR layout and signature placement in `configs/mNNN.S.cfg`.

4. Prefer generic mapper-family helpers.
   - Reuse shared family macro files such as `mmc3_core_macros.inc`, `vrc4_core_macros.inc`, `taito_core_macros.inc`, and `jy_company_macros.inc` when the mapper fits those families.
   - Only add mapper-specific code when the shared abstractions cannot express the hardware cleanly.

5. Use the right verification channel.
   - Default to the `$6000` status-byte protocol.
   - If `$6000-$7FFF` is used for registers, PRG-ROM, or other unsafe accesses, set `CONSOLE_VERIFICATION = 1` and keep the ROM console-verifiable instead.

6. Follow the repository build wiring.
   - Add or update `ROM_RULE` targets for single-aspect ROMs.
   - Add or update `COMBINED_RULE` targets for combined ROMs.
   - Keep naming aligned with the existing `mNNN.S_aspect.nes` and `mNNN.S.nes` conventions.

7. If you add a new test aspect, make it reusable.
   - Create `tests/test_<aspect>.s` using the same single-ROM and combined-ROM pattern as existing aspects.
   - Update `tests/test_combined.s` so the new aspect can be imported and called behind `HAS_TEST_<ASPECT>` gates.
   - Wire the new aspect into only the mappers that actually support or need it.

8. Keep Rust runner wiring in sync.
   - Update `src/integration_tests/mapper_tests.rs` with `setup_rom_test!` or `setup_rom_console_test!` when ROM coverage changes.
   - Build the affected ROMs and keep committed `bin/` outputs up to date for CI.

9. Bias toward reusable assembly.
   - Do not write mapper-specific assembly when a mapper-specific defs/config pair plus small generic hooks will do.
   - If a mapper-specific branch in shared code is truly necessary, keep it narrow and document why the generic path is insufficient.

## References

- `references/repo-architecture.md`: repository layout, build flow, extension points, and file-level guidance for adding mappers or aspects.

## Examples

- Adding coverage for a mapper that already fits `test_prg_banking.s`:
  create `defs/mNNN.S_defs.inc`, create `configs/mNNN.S.cfg`, add Makefile targets, and wire Rust integration tests. Do not clone `test_prg_banking.s`.

- Adding a new reusable aspect such as a new write-protection behavior:
  add `tests/test_<aspect>.s`, hook it into `test_combined.s`, expose mapper-specific control through defs macros, and opt mappers in through Makefile aspect lists.

- Adding a mapper that cannot safely use `$6000`:
  keep the shared shell and test flow, but set `CONSOLE_VERIFICATION = 1` in the defs file and use `setup_rom_console_test!` in Rust.
