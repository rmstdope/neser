# nr-0ry — retrospective

- **Implementer:** Bishop
- **Date:** 2026-10-04
- **PR:** #3330

## Mapper unit tests passed while the CPU never saw the banking they tested

**What happened.** `mapper_121_prg_bank_switching_uses_protection_index_register` checked the
protection PRG banks through `read_prg` and passed. The CPU bus reads cartridge space through
`read_prg_open_bus` (`src/nes/bus/mapper_device.rs`), and mapper 121 overrode that to send
`$8000-$FFFF` straight to the plain MMC3. So the game ran the wrong code from power-on. The bead
guessed at CHR banking; the PRG cause turned up only by diffing NESER's and Mesen2's write traces
(a Lua `addMemoryCallback` script against a temporary `eprintln!` in the mapper).
**Why.** A mapper has two PRG read entry points, and its tests exercised only the one the bus does
not use for this range.
**Cost.** About an hour of tracing before the cause was found.
**Prevent by.** Mapper unit tests that check PRG-ROM banking read through `read_prg_open_bus`, the
bus's entry point, not `read_prg`; a note to that effect in
`skills/mapper-verification-roms` or next to `read_prg_open_bus` in
`src/nes/cartridge/mapper.rs`. A sweep for the same override shape found no other affected mapper.
**Seen before.** none found.

## A Mesen2 reference capture showed a frame that two recaptures did not

**What happened.** With the nr-nwy recipe (`--nes.RamPowerOnState=AllZeros`,
`--nes.DisableFrameSkipping=true`), the first Mesen2 captures at 1800 and 3600 showed a scrambled
screen. Two later runs of the same command, and a run of a multi-frame capture script, showed the
demo fight at those frames.
**Why.** Not established. The first captures were taken while another worktree's build was
running, so load is a candidate.
**Cost.** About 20 minutes chasing a "timing offset" that was not there.
**Prevent by.** `scripts/reference_capture/README.md` and the nr-nwy rerun recipe: capture every
Mesen2 checkpoint twice and use only frames where the two captures are identical, as
`nes-hardware-research` step 5 already asks for single diffs.
**Seen before.** none found.
