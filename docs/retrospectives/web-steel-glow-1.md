# web-steel-glow-1 — retrospective

- **Implementer:** Claude (interactive session, no bead: the navigator asked for the Steel Glow series without beads)
- **Date:** 2026-10-23
- **PR:** #3352

## Our own class names were silently styled by DaisyUI, twice in one PR

**What happened.** The status line under the picture carried `class="status"`. DaisyUI 5 has a
`status` component (a small dot), so the empty line drew a grey dot, and a line holding an error
message became a 317 px square (`aspect-ratio: 1`). Renaming the line and colouring errors with a
`status-error` class then hit DaisyUI's `.status-error`, which sets the background to the same
colour as the text, so the message became an unreadable pink bar. Both were found by the review
sub-agent, not by the 55 green Playwright specs.

**Why.** Tailwind v4 emits a DaisyUI component's CSS whenever its class name appears anywhere in
the scanned sources, so any of our own class names that happens to match a DaisyUI component name
or modifier is styled by it. Nothing checked for that.

**Cost.** Two review rounds and two fix commits on the PR.

**Prevent by.** `web/src/ui/daisy_class_names.test.ts` now fails when a class used in
`web/index.html` or set from `web/src` code is also a DaisyUI class, unless it belongs to a
component used on purpose. For state, prefer a `data-*` attribute over a modifier class.

**Seen before.** none found
