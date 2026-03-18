# Mapper Verification ROM Repository Architecture

Use this reference when creating or updating ROMs in `roms/automated_tests/mapper_verification/`.

## Core layout

- `common/`
  - `shell.s`: shared reset flow, pass/fail reporting, IRQ handler, `$6000` status-byte protocol
  - `nes_init.s`: console-safe NES initialization
  - `console.s`: text console rendering
  - `test_macros.inc`: `start_test`, `pass_test`, `fail_test`, `all_passed`, assertions
  - `nes20_header.inc`: NES 2.0 header macro
  - `mmc3_core_macros.inc`, `vrc4_core_macros.inc`, `taito_core_macros.inc`, `jy_company_macros.inc`: shared mapper-family helpers

- `tests/`
  - Shared aspect implementations such as `test_prg_banking.s`, `test_chr_banking.s`, `test_irq.s`, `test_prg_ram.s`, `test_block_select.s`
  - `test_combined.s`: imports and runs enabled aspects behind `HAS_TEST_*` flags

- `defs/`
  - One `mNNN.S_defs.inc` file per mapper or mapper/submapper variant
  - Holds mapper identity, ROM/RAM sizes, capability flags, and register macros

- `configs/`
  - One `mNNN.S.cfg` file per mapper or mapper/submapper variant
  - Defines PRG/CHR memory layout, fixed vs switchable banks, signatures, font placement, and vectors

- `bin/`
  - Built ROMs are committed
  - `bin/rom_singles/` contains single-aspect ROMs
  - top-level `bin/` contains combined ROMs

## Build flow

Run commands from `roms/automated_tests/mapper_verification/`.

Useful commands:

```bash
make
make singles
make combined
make clean
make bin/rom_singles/m041.0_prg_banking.nes
make bin/m041.0.nes
```

The Makefile writes `bin/obj/mapper_config.inc` dynamically so shared test code can include the current mapper defs file as `mapper_config.inc`.

## Normal workflow for adding a mapper

1. Research the mapper specification with `nes-hardware-research`.
   - For verification-ROM work, stop at NESdev/wiki-backed results.
   - Do not use emulator source code as evidence.

2. Reuse existing aspects first.
   - Match the mapper against existing generic tests before considering a new test file.

3. Add `defs/mNNN.S_defs.inc`.
   - Set `MAPPER_NUM`, `SUBMAPPER_NUM`, ROM sizes, RAM sizes, capability flags, bank sizes, and standard macros.
   - Reuse shared family macro includes when applicable.

4. Add `configs/mNNN.S.cfg`.
   - Reflect the mapper's real banking scheme and where signatures, vectors, and font data must land.

5. Add Makefile targets.
   - Add `ROM_RULE` entries for every single-aspect ROM you want.
   - Add one `COMBINED_RULE` entry with the mapper's supported aspects.

6. Add shared-code hooks only if required.
   - `common/shell.s` may need mapper-specific early init for font mapping, RAM enablement, or mode setup.
   - Shared test files may need narrowly-scoped generic hooks if the mapper cannot be expressed through existing macros alone.

7. Update Rust integration tests.
   - Edit `src/integration_tests/mapper_tests.rs`.
   - Use `setup_rom_test!` for `$6000` status ROMs.
   - Use `setup_rom_console_test!` for console-verified ROMs.

8. Build and keep `bin/` outputs current.

## Workflow for adding a new reusable test aspect

1. Create `tests/test_<aspect>.s`.
2. Follow the existing pattern:
   - include `test_macros.inc`
   - include `mapper_config.inc`
   - export `run_tests` and `test_title_string` for single-ROM builds
   - export `run_<aspect>` for combined builds
3. Update `tests/test_combined.s`.
   - add `.ifdef HAS_TEST_<ASPECT>`
   - import `run_<aspect>`
   - print a section heading
   - call `run_<aspect>`
4. Add Makefile targets for any mapper that should build the aspect.
5. Prefer new defs macros or capability flags over hardcoding mapper numbers in the new test.

## Result-channel rule

- Use `$6000` status-byte verification when the mapper can safely expose writable memory at `$6000-$7FFF`.
- Use `CONSOLE_VERIFICATION = 1` in the defs file when that range is used for registers or otherwise unsafe.
- Console-verification mappers must still follow the shared shell and console reporting flow.

Examples already using console verification include mapper defs such as:

- `defs/m037.0_defs.inc`
- `defs/m040.0_defs.inc`
- `defs/m041.0_defs.inc`
- `defs/m042.0_defs.inc`
- `defs/m043.0_defs.inc`
- `defs/m045.0_defs.inc`
- `defs/m046.0_defs.inc`
- `defs/m047.0_defs.inc`

## Reuse-first checklist

- Can an existing `tests/test_*.s` file already cover this behavior?
- Can a new capability flag or macro in `defs/` express the mapper difference?
- Can a shared family macro file absorb the mapper-family variation?
- Can the linker config solve the placement problem without changing test logic?
- Is a mapper-specific branch truly required, or is it compensating for missing abstraction?

If you answer "yes" to any reuse question, prefer that route over writing a one-off mapper-specific ROM.
