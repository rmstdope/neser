# nr-use — a held coin key reads as a jammed coin

## What happened
Vs. Duck Hunt could not be played on desktop because pressing 6 never credited a coin. The key
mapping and the `$4016` coin bit were both correct. The game itself rejects a coin line held for
ten frames or more as a jammed coin, and a human key press lasts that long.

## Why
The core modelled the coin switch as a level that followed the key. Real coin mechanisms give a
short pulse, and Mesen2 models one: `VsInputButtons::InsertCoinFrameCount = 4`. The first pass
on this bead fixed the Zapper trigger that the bead's title named. Nobody had checked that a coin
could be inserted at all, so verification failed on the coin instead.

## Cost
One failed verification and a second bugfix pass.

## Prevent by
Emulate a Vs. arcade input (coin, service) with the frame timing Mesen2's `VsInputButtons` gives
it, not as a level that follows the key. When a "cannot be played" game bead is filed, check
headlessly that the game gets from power-on into play before fixing the named input.

## Seen before
No.

# nr-use — a desktop-only key fix failed verification on the web shell

## What happened
The second pass made 6 insert a coin on desktop, and verification then ran on desktop and web.
Desktop passed. On web, pressing 6 did nothing, because the web shell had never bound a Vs. coin
key. Its key tables stop at Select/Start and 9/0, and `WasmNes` exposed no coin insert. The
acceptance said "On desktop", so the second pass never looked at `web/src/app.ts`.

## Why
The two shells have separate key tables: `src/frontends/native/keyboard/controller_mapping.rs`
and `keyToButtonController1/2` in `web/src/app.ts`. The core also reaches the page only through
`#[wasm_bindgen]` methods on `WasmNes`. A core input API added for desktop does not reach web
unless someone exports and binds it there. The Vs. service button (`-`) is still desktop-only.

## Cost
A third failed verification and a third bugfix pass.

## Prevent by
When a bead changes what a key or input does, grep `web/src/app.ts` and `src/frontends/web/wasm.rs`
for the binding as well as the native mapping. Say in the PR body whether web is covered, even
when the acceptance names only desktop. A host `#[test]` inside `wasm.rs` cannot call `load_rom`:
it reports to the page and aborts with "function not implemented on non-wasm32 targets". Insert
the cartridge through `web.core_mut()` and step frames with `run_until_frame_ready` instead.

## Seen before
The pass above: the first two passes each fixed only the input the report named.
