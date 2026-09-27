# Release notes

One file per release, `v<major>.<minor>.<patch>.md`, written by the release skill
(`.claude/skills/release/SKILL.md`) from everything merged since the previous tag and approved by
the navigator before it is committed. The release workflow (`.github/workflows/release.yml`) uses
the file whose name matches the pushed tag as the GitHub Release body, so what is here is what
players read on the Releases page. A tag without a file falls back to git-cliff's commit list.

A file is written once, with its release, and not edited afterwards; a correction is a new
release.

## Format

    # NESER v1.3.0 — September 27, 2026

    One or two sentences a player understands: what this release is about.

    ## Highlights
    - The three to five changes that matter most, one line each. The same list, joined into a
      sentence, is the web frontend's idle scroll text for this release.

    ## New
    - Every feature a player or developer can use, in their words. Say which system
      (NES, Game Boy, Game Boy Color, Game Boy Advance, SNES) and which frontend (desktop, web)
      when it is not all of them.

    ## Fixed
    - Every fix a player could have noticed, as the symptom it removes ("Kirby Super Star no
      longer freezes at the title screen"), not the cause.

    ## For developers
    - Tooling, test suites, CI and refactoring, summarised rather than listed.

    Everything merged: <first PR>–<last PR>, <n> pull requests since v1.2.0.

Bead ids and commit hashes stay out of the player sections; the closing line and the git history
carry the trace.
