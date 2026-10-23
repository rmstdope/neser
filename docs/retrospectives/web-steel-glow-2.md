# web-steel-glow-2 — retrospective

- **Implementer:** Claude (interactive session, no bead: the navigator asked for the Steel Glow series without beads)
- **Date:** 2026-10-23
- **PR:** #3353

## A module-level const read before its line broke the dev server but passed every Playwright spec

**What happened.** `updateEmulationButtons()` (first called part-way down `web/src/app.ts`) was
given a call that read `zoomLevelEl`, a `const` declared about 700 lines further down. Under
`npx vite` (the dev server) the page threw `Cannot access 'zoomLevelEl' before initialization`
at load, so everything after that point never ran. All 60 Playwright specs still passed, because
they run against the production build, where the bundled module did not throw.

**Why.** `app.ts` is one long module whose top-level statements run in file order; a function
declared early can be called before a later `const` it reads is initialised (the temporal dead
zone). The dev server serves the module as written; the production bundle evidently does not
keep that ordering check. Not established which bundling step drops it.

**Cost.** Found only by probing the dev server by hand after the review; one more fix commit.

**Prevent by.** Code that `updateEmulationButtons` (or anything else called at start-up) reaches
must not read a module-level `const` declared further down `app.ts`: look the element up inside
the function, or declare it above the first call. Check new start-up code in the dev server
(`npx vite`, watch for `pageerror`), not only in the Playwright build.

**Seen before.** none found
