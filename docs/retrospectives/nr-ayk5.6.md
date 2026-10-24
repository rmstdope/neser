# nr-ayk5.6 — retrospective

- **Implementer:** Storm
- **Date:** 2026-10-24
- **PR:** #3391

## A save state is not "the game": its hash differed between two games in step

**What happened.** The first build hashed `save_state_bytes()` to compare the two games. Review found two ways this could report a drift that wasn't there:
- Both cores save the queue of audio samples the page has not played yet. That queue differs between two pages in step, for example when one runs two frames in a tick to catch up.
- On the SNES, loading a state restarts the output resampler without its input queue (`native_samples` is transient). A resynced joiner's resampler phase then never matched the host's again, so every SNES resync would have been followed by another, forever.

**Why.** The determinism harness (nr-ayk5.1) proves that two consoles agree on their whole save state. It builds them without an audio output rate, though, and drains audio identically on both sides, so neither difference ever showed there. I trusted "save state equal ⇔ game equal" without checking how the page drives audio.

**Cost.** Two extra review rounds and two producer fixes, about an hour. Players would have paid more: endless SNES resyncs had this merged as first written.

**Prevent by.**
- `Emulator::netplay_state_hash` now leaves the audio output side out.
- `src/platform/determinism.rs` has `page_console` (a console with the page's 44.1 kHz output rate) and two tests that use it. Any later netplay comparison should build its consoles through `page_console`, not `loaded_console`.
- A `.cerebro/traps.md` entry for the navigator to consider: "A save state includes audio output plumbing (queued samples, resampler phase) that differs between consoles in step and is not restored exactly; compare games with `netplay_state_hash`, never raw save-state bytes."

**Seen before.** none found

## A tampered packet the receiver had already heard did nothing

**What happened.** The two-browser test changes a state hash in one of the host's packets. At first it changed the next packet that carried any hash, and nothing happened. Every packet repeats the newest hash, and the receiver keeps the first copy it hears for each frame, so the changed copy was a repeat and was ignored.

**Why.** The lockstep wire format repeats data until it is acknowledged. That is its strength against loss, and it also means only the first copy of anything counts.

**Cost.** One local spec run and about ten minutes.

**Prevent by.** The init script in `web/integration/specs/play-together.integration.spec.ts` now changes only the first packet with a new hash frame, and its comment says why. Any later tampering with lockstep packets must target first copies.

**Seen before.** none found
