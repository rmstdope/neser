# nr-zdy.4 — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-27
- **PR:** #3245

## The agreed experience relied on Game Boy Load State, which the web did not have

**What happened.** The acceptance said "Load State restores the game exactly as saved, including the console it was saved on", and "The states" had a Load State case. On the web, Save/Load State was offered only for NES and SNES games (`web/src/save-state/save_state_support.ts`). I found this during the build, when a wasm test of the restore hung. I first shipped without it and recorded the gap under *Out of scope*. The review flagged that as a deviation from the agreed experience. The navigator then chose to build Game Boy save states in this bead.

**Why.** The UX stage agreed behaviour on top of a control that does not exist for that console. Nothing in `agree-experience` asks whether each control a state names is actually present for the console in question.

**Cost.** One review round, one question to the navigator, and a second build/gate/review cycle, about two hours of wall-clock.

**Prevent by.** In `agree-experience`, when a state names an existing control (Load State, a button, a shortcut), check in the running app, or with `grep` on its visibility rule, that the control is shown for the console the bead is about, and write in the record that it was checked. In `produce-bead` step 2, the producer can check the same thing while writing the plan: every control the acceptance names, looked up for the bead's console.

**Seen before.** None found.

## A wasm stack overflow showed up only as "Failed to detect test as having been run"

**What happened.** `WasmGb::load_state_bytes` (and main's own `GameBoy::load_state_bytes`) never returned in wasm. `wasm-pack test` printed only "Failed to detect test as having been run. It might have timed out.", or "some tests failed" with no test named. Natively the same sequence passed in 0.01 s. Probes found that a typed `GbSaveState` parse died where a `serde_json::Value` parse of the same bytes worked, and that an 8 MB stack (`-C link-arg=-zstack-size=8388608`) made it pass.

**Why.** Deserializing `GbSaveState` carried the 50 KB bus state (32 KB of work RAM) by value through serde's frames. That overflowed wasm's default 1 MB stack, which has no guard page, so memory was corrupted silently instead of trapping. Boxing `GbSaveState.bus` and `BusState.wram` fixed it; the JSON is unchanged.

**Cost.** About an hour of probing. Also, a first full `wasm-pack test` run with a mismatched ChromeDriver failed with an opaque "http status: 404"; `scripts/chromedriver_match.py`, as used by the gate, names the fix, but running `wasm-pack test` by hand skips it.

**Prevent by.** When a wasm test hangs or "fails to detect" while the native equivalent passes, rerun it with `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='<the target rustflags from .cargo/config.toml> -C link-arg=-zstack-size=8388608'`. If it passes, it is stack depth. This belongs as a line in `.cerebro/traps.md`. A gate leg that runs the Game Boy wasm tests at 512 KB would guard the headroom; that is a CI decision.

**Seen before.** None found.
