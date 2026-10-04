# nr-tlf: retrospective

## A `#[cfg(test)]` accessor in core code turned two clippy legs red

**What happened.** To let the native keyboard tests read a Power Pad's and an SNES pad's state,
I added two test-only accessors, `Bus::controller_state` and `InputPorts::port1_state`, under
plain `#[cfg(test)]`. The host test run and host clippy passed. The full gate then failed at
`cargo clippy --target wasm32-unknown-unknown … --features wasm --all-targets`
(`method controller_state is never used`). The `--features frontend` leg fails the same way.

**Why.** `frontends::native` is compiled only with `feature = "native"`. In the wasm and the
frontend-only builds the accessors are compiled into the test target, but their only callers
are not, so `-D warnings` rejects them as dead code.

**Cost.** One full gate run (about 15 minutes) and one extra commit. The delta review also
flagged it.

**Prevent by.** A test helper added in a core module (`src/nes`, `src/snes`, `src/platform`)
for tests that live under `src/frontends/native` is gated `#[cfg(all(test, feature = "native"))]`,
the condition its callers compile under. Run the two non-host clippy legs from CLAUDE.md
before the full gate whenever a `#[cfg(test)]` item is added outside the module whose tests
use it.

**Seen before.** None found.
