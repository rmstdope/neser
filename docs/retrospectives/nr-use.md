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
