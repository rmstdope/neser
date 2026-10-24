# Steel Glow — the agreed web UI overhaul

Drawings agreed with the navigator on 2026-10-23. Open them in a browser (they load fonts from
Google Fonts and draw placeholder game pictures; no build is needed).

- `restyle-only.html` — the Steel Glow look on today's layout (step 1).
- `desktop.html` — the reworked desktop app, one screen per state.
- `phone.html` — the handheld phone layout, portrait and landscape.
- `parts.html` — dialogs, autorun, controls, brief messages, colours and controls.

Every change marked *new* in the drawings was accepted. It ships in small steps, one pull request
each, in this order: the look; the control bar under the picture; the settings panel; the library
start screen (picking a game starts it, Eject returns to it); recent games; short lists for filter,
palette, colours and reset; the save-state preview; the docked debugger; the keys and controls
panel; errors where you are; the phone handheld layout.

The game pictures, ROM names and some example messages in the drawings are placeholders.
