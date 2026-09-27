# nr-hg7 retrospective

## The bead's suggested signal, `emu.getScriptDataFolder()`, cannot detect Mesen2 file access being off

**What happened.** The bead, following `docs/retrospectives/nr-1i3.md`, proposed
`emu.getScriptDataFolder()` ("empty when I/O is disabled, per `snes-hardware-research`") as the
check. A probe under `Mesen --testRunner` (Mesen2 2.1.1) printed `true` for it both with
`AllowIoOsAccess` off and in a portable-folder attempt. The reliable signal is that the globals `io`
and `os` are `nil` with the setting off. That is also why the original script failed silently:
`os.getenv` raised at load, before any frame callback was registered.
**Why.** The nr-1i3 retrospective attributed the claim to `snes-hardware-research`, but no skill
contains `getScriptDataFolder` (`grep -rn getScriptDataFolder .claude/skills` is empty). The
claim was never measured.
**Cost.** About 5 minutes, spent on a probe run before the plan was written. It would have cost
more if the guard had been built on the suggested signal: the guard would never have fired.
**Prevent by.** A probe on the real binary, as recorded in this bead's plan (*Context*), before a
Mesen2 Lua API claim becomes a design. Also, a correction to `docs/retrospectives/nr-1i3.md`'s
*Prevent by*, which still names the unusable signal (left for the navigator; retrospectives
record, they do not fix).
**Seen before.** `docs/retrospectives/nr-1i3.md` is where the claim came from; there is no
earlier sighting.
