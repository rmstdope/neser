# nr-10g — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-26
- **PR:** #3209

## Random-input golden vectors passed while the decompressor overflowed on long runs

**What happened.** The S-DD1 decompressor was pinned by 16 vectors from Mesen2's decoder, one per header mode, each 96 pseudo-random input bytes giving 64 output bytes. All 16 passed on the first run. The next test, a 65536-byte zero-length transfer in `zero_length_means_65536`, panicked with `attempt to add with overflow` in `GetCodeword`. At Golomb order 7, `80h + (1 SHL 7)` is 100h, which does not fit fullsnes' 8-bit run counter. Two sparse vectors (about one nonzero byte in 32, 512 output bytes) reproduced it against Mesen2 and now pin the 8-bit wrap.
**Why.** Pseudo-random input keeps the adaptive probability states low, because the least probable symbol keeps occurring. Only long runs of the most probable symbol climb the evolution table to its order-7 states, so vectors that covered every *header mode* still left whole *state* ranges untested.
**Cost.** Small: one extra RED/GREEN cycle. The risk was larger. A game-only check could have shipped it, since a debug build panics and a release build silently wraps, and here the wrap happens to be correct.
**Prevent by.** Golden vectors for an adaptive (context-modelling) decompressor should cover the model's state space, not only its modes: add low-entropy inputs (all-zero, and sparse bytes) with long outputs next to the random ones. The SPC7110 bead (same family, from nr-ffk/gh-2725) should start from this. The Mesen2 harness recipe used here (compile `Sdd1Decomp.cpp` with stub `pch.h`/`Serializer.h`/`ISerializable.h` headers and a stub `Sdd1Mmc` serving a buffer) should work for its `Spc7110Decomp` too.
**Seen before.** None found.
