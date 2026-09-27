# nr-yvv — retrospective

## A toast raised from the core crashed the web build on load

**What happened.** The plan put the "Super Scope connected" message in `Snes::load_rom`, through `AppContext::add_toast`. Every host test passed. In Chrome, loading Metal Combat panicked with `time not implemented on this platform`, and the next `drain_toasts` call then failed with `recursive use of an object detected`.

**Why.** `AppContext::add_toast` stamps each toast with `std::time::Instant::now()`, which panics on `wasm32-unknown-unknown`. So any core code that raises a toast on a path the web also takes is a crash waiting for its first web user. The S-RTC "enhancement hardware" warning in `Snes::load_rom` is one such path already. The wasm test suite did catch it, but only as an opaque ChromeDriver `http status: 404`, which looks the same as the known driver-version mismatch (nr-09s, nr-630).

**Cost.** About twenty minutes: one browser session, one fix commit, a plan amendment, and one gate run that could not be trusted.

**Prevent by.** Either give `AppContext`'s toast clock a wasm-safe source, or stop web-reachable core paths from calling `add_toast`, for example with a note in `src/platform/app_context.rs` or an entry in `.cerebro/traps.md`. The remaining S-RTC warning in `Snes::load_rom` would crash the web the same way.

**Seen before.** None found.

## The automated Chrome cannot exercise pointer lock or run frames

**What happened.** Walking the web build through Chrome automation: `requestPointerLock` always fired `pointerlockerror`, and `document.hidden` was `true`, so `requestAnimationFrame` never advanced the emulator past the first frames it had already drawn.

**Why.** Not established. Both look like limits of the automated tab rather than the page.

**Cost.** About fifteen minutes. The captured state was checked by stubbing `pointerLockElement`/`requestPointerLock` in the page. In-game shooting could not be checked in the browser and is left to verification.

**Prevent by.** A web walk-through that needs pointer lock or running frames goes to the verifier with a person at the keyboard. The producer's browser check covers only what does not depend on either.

**Seen before.** None found.
