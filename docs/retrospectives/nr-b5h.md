# nr-b5h — retrospective

- **Implementer:** Storm (build, review), Cyclops (merge after the navigator's answer)
- **Date:** 2026-09-27
- **PR:** #3256

## Removing the extra reallocations did not remove the stall

**What happened.** The bead's acceptance asked that a Zoom click on CI complete in under a
second, on the assumption that the 4.65–4.97 s stall was the sum of the several backing-store
assignments the zoom probes made. After the fix, a click assigns the backing store at most once,
yet CI still measured Zoom + at 2.2 s and Zoom - at 3.0 s. Instrumenting locally showed that one
necessary resize of a displayed WebGL canvas costs 110–350 ms locally and seconds on CI's
software GL, while the same resize on a never-displayed canvas takes 4–11 ms. The cost is
compositor-side and does not scale with the number of assignments. The bead parked on a scope
question, and the navigator relaxed the timing clause and filed nr-v5x for the remaining cost.
**Why.** The acceptance put a wall-clock target on a cost whose breakdown had not been measured.
The nr-dv5 diagnostic timed the whole click, not the single resize, so the share one resize
contributes was unknown when the bead was written.
**Cost.** One full producer pass parked after the build was green, a navigator decision, and a
second session to merge.
**Prevent by.** When a performance bead's acceptance names a time, its description gives the
measurement that splits the cost it removes from the cost it leaves (here: one resize timed
alone). nr-v5x starts from the numbers in nr-b5h's notes.
**Seen before.** nr-dv5 (the same zoom stall, seen there as a flaking click timeout).
