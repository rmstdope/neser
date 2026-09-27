# nr-cl1 — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-27
- **PR:** #3284

## A spec-correct MMC6 fix turned three existing m004.1 verification ROMs red

**What happened.** With the MMC6 implemented per nesdev, the fast gate passed. The full gate then
failed `test_mv_m004_1_irq`, `test_mv_m004_1_write_protect` and `test_mv_m004_1_combined`: all three
timed out waiting for a `$6000` status byte, and `write_protect` expected MMC3-style write-protect.
**Why.** `defs/m004.1_defs.inc` described submapper 1 as "MMC3 with NEC IRQ" with 8 KB of RAM at
`$6000`. NES 2.0 submapper 004:1 is the MMC6 (Sharp IRQs, 1 KB at `$7000`, no RAM at `$6000`), and
NEC is 004:4. The ROM encoded a wrong reading of the submapper table, and the emulator's missing
MMC6 support agreed with it, so both stayed green together.
**Cost.** One full-gate run (about 25 minutes) and a sub-agent rewrite of the m004.1 ROMs (about
25 minutes, including installing cc65). The rewrite had to go to an agent that had not read the
implementation, because the ROM author may not.
**Prevent by.** In `skills/mapper-verification-roms`, when a ROM is added for a submapper, cite the
NES 2.0 submapper table's line for it (https://www.nesdev.org/wiki/NES_2.0_submappers) in the
`defs/mXXX.S_defs.inc` header, so a reviewer can check the board against the table.
**Seen before.** none found.
