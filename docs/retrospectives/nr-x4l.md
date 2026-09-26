# nr-x4l — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-09-26
- **PR:** #3201

## A patched SameBoy tester silently came out identical to the stock one

**What happened.** To test whether CGB captures differ from SameBoy only by colour correction, I
patched `Tester/main.c` in `~/repos/SameBoy` and ran the recipe's `make tester -j8`. It printed
nothing alarming under `>/dev/null`, and `build/bin/tester/sameboy_tester` was byte-identical to
the stock binary: the patched tester still gave the 29.6 % diff, which read as "not only colour".
Building in a fresh copy showed `make tester` stops at the boot-ROM step (`rgbgfx: No such file
or directory`) before linking; `make -k build/bin/tester/sameboy_tester -j8` links it.

**Why.** Without rgbds the `tester` target fails at its boot-ROM prerequisite, so the old binary
is left in place. The recipe's comment ("the tester itself builds") was true only for a tree whose
tester had been built some other way.

**Cost.** About ten minutes and one wrong intermediate result, caught only because `cmp` showed
the two binaries identical.

**Prevent by.** `scripts/reference_capture/README.md`, "SameBoy (GB and CGB)", now gives
`make -k build/bin/tester/sameboy_tester -j8` (this PR). When rebuilding any reference tool from
patched source, `cmp` the new binary against the old before trusting its output.

**Seen before.** None found.
