# nr-1i3: retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-27
- **PR:** #3231

## Mesen2 captures timed out without a word because Lua file access was off

**What happened.** The first batch of 13 Mesen2 reference captures
(`scripts/reference_capture/mesen2_capture.lua`) saved nothing. Each run used its whole
`--timeout=60` and exited without printing `SAVED`, and nothing said why.
`~/Library/Application Support/Mesen2/settings.json` had `"AllowIoOsAccess": false`, so the
script could not open its output file. It printed no error, and the run only ended when the
timeout killed it. A second, smaller trap came up in the same batch: the
`until ! pgrep -f "Mesen --testRunner"` guard from the skill matched the `zsh -c` command line
that contained the guard itself, so a capture loop written inline in one Bash call could block
on its own shell.
**Why.** The README says to turn `AllowIoOsAccess` on and restore it afterwards. Some other
session had restored it (correctly), and a missing permission and a slow ROM look the same
from outside.
**Cost.** About 15 minutes: 13 runs of 60 s each, and one manual run to find the cause.
**Prevent by.** `mesen2_capture.lua` could test `emu.getScriptDataFolder()` (empty when I/O is
disabled, per `snes-hardware-research`) or the `io.open` result, and `print` an error and
`emu.stop(1)` at once. In `scripts/reference_capture/README.md`, the `pgrep` guard could match
the binary path `Mesen.app/Contents/MacOS/Mesen --testRunner`, which a shell's command line
does not contain unless it spells the path out.
**Seen before.** None found (`git grep AllowIoOsAccess -- docs/retrospectives/` is empty).

## The wasm gate step failed on a ChromeDriver/Chrome version mismatch, and the worktree had no `.venv`

**What happened.** `wasm-pack test` failed with a webdriver `404` and the driver killed
(`SIGKILL`): the cached ChromeDriver was 154 and the installed Chrome was 153. Once a matching
ChromeDriver 153 was put on `PATH`, 112 tests passed. The Python steps then fell back to the
main checkout's `.venv`, which is missing `requests` and `textual` (28 errors) and `mypy`. A
worktree `.venv` built with `pip install --group scripts/pyproject.toml:test --group
scripts/pyproject.toml:dev` passed all 453 tests.
**Why.** As the earlier retrospectives say.
**Cost.** One wasted full-gate run, about 25 minutes under load.
**Prevent by.** As the earlier retrospectives say.
**Seen before.** nr-6e9, nr-hab.1, nr-273, nr-630 and others (ChromeDriver and `.venv`).
