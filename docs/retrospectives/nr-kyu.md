# nr-kyu — retrospective

- **Implementer:** Gambit
- **Date:** 2026-09-28
- **PR:** #3300

## The sweep's NESER command is not the zero-RAM power-on it claims to be

**What happened.** The bead describes a zero-RAM power-on compared against Mesen2
(`--nes.RamPowerOnState=AllZeros`). Its NESER command,
`target/release/neser --headless --frames 3600 --capture-every 300 --nes-palette mesen …`, passes no
`--ram-init-mode`, and `neser --help` says the default is random on desktop builds. So NESER
powered on with random RAM while Mesen2 used zeros. I checked the three blank-screen ROMs with
`--ram-init-mode zero` before trusting any result. It changed nothing for them, but a game that
reads uninitialised RAM would show a difference that neither emulator's hardware model causes.
**Why.** `scripts/reference_capture/README.md` line 24 gives the NESER capture command without
`--ram-init-mode zero`, and the sweep copied it.
**Cost.** One extra comparison run per ROM, and some doubt about every "differs" in the sweep.
**Prevent by.** Add `--ram-init-mode zero` to the NESER command in
`scripts/reference_capture/README.md` (and to the sweep's command) whenever Mesen2 runs with
`--nes.RamPowerOnState=AllZeros`.
**Seen before.** none found

## NESER's CPU trace is silent in release and headless builds unless built as debug

**What happened.** `target/release/neser --headless --trace-cpu …` printed nothing. `trace_cpu!`
is live only in debug builds (`src/main.rs`: "only active in debug builds"), so I rebuilt with
`cargo build --bin neser` and used `target/debug/neser --headless --trace-cpu`. That worked, and
it found all three causes.
**Why.** The trace level is initialised only in debug builds, and `--help` does not say so.
**Cost.** A few minutes and a second build.
**Prevent by.** Say "debug builds only" in the `--trace-cpu` line of `neser --help`
(`src/nes/console/config/cli.rs`).
**Seen before.** none found
