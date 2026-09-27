# nr-v5x — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-09-27
- **PR:** #3278

## `gl.finish()` returned at once, so the stall was wrongly ruled out as queued GPU work

**What happened.** nr-b5h's investigation timed `gl.finish()` just before the slow canvas resize,
saw it return at once, and concluded the ~2 s was not queued GL work but "compositor work". A
Chrome trace (`browser.startTracing` with the `gpu`/`blink` categories) of the same click on
SwiftShader shows the opposite: the resize's own `GetGLError` and `CheckFramebufferStatus` each do a
`CommandBufferHelper::Finish`, and the first one waits for every frame the render loop has in
flight. In the same page `gl.readPixels` (a real sync) took 50–180 ms, and a resize right after it
took 12–30 ms.
**Why.** Chrome's WebGL `finish()` did not wait for the GPU process here; `readPixels` did.
**Cost.** One bead (nr-b5h) parked on a wrong premise, and about an hour of this one re-deriving it.
**Prevent by.** When a WebGL call is suspected of waiting on the GPU, drain with a 1-pixel
`gl.readPixels`, never `gl.finish()`, or take a Chrome trace and look for
`CommandBufferHelper::Finish` under the slow call.
**Seen before.** nr-dv5 (the zoom stall first measured, cause not traced).

## Playwright's click time was read as how long the page blocks

**What happened.** The zoom spec logged the wall-clock of `await button.click()` as the zoom cost.
On CI that figure varies 1.5–5.2 s for the same code on main. It includes Playwright's
actionability waits (scroll into view, the element stable across animation frames), and those
frames are slow on software GL under the NTSC filter. On this PR's CI run the Zoom + click took
948 ms, of which its handler ran 623 ms.
**Why.** Established by the capture/bubble listener pair this PR added to the spec.
**Cost.** One CI round whose single timing looked worse than main's (4.1 s) for a change that
removed two thirds of the synchronous GPU waits.
**Prevent by.** Time page-side work in the page (the spec's window capture/bubble listeners, or a
trace), and treat a Playwright action's duration only as an upper bound.
**Seen before.** none found.

## Wall-clock A/B runs on the fleet machine were noise

**What happened.** An A/B of `antialias: false` on this machine made every filter look 2–4× slower
("None" went from 120 to 30 fps), while load average was 72 on 18 cores from other sessions'
builds.
**Why.** Other fleet sessions were compiling at the same time; SwiftShader runs on the CPU.
**Cost.** One misleading run, about ten minutes.
**Prevent by.** Check `uptime` before a timing comparison, and prefer a load-independent measure:
here, counting `CommandBufferHelper::Finish` events per click in a trace (6 before, 2 after).
**Seen before.** none found.
