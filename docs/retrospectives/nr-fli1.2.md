# nr-fli1.2: a dev server stopped with a machine-wide `pkill`

## What happened

To validate `pnpm run dev`, the producer started the dev server as a background task, drove it with
a scratch Playwright script, and then stopped it with `pkill -f vite -U $(id -u)`. That pattern
matches every Vite and Vitest process the user owns. Other fleet sessions share the same machine and
user, so any `vite`, `vitest` or gate run of theirs alive at that moment was killed too.

## Why

Stopping the server was treated as a local clean-up, and the pattern was picked to be sure it was
caught. Nothing in the run marked that the process table is shared between fleet sessions.

## Cost

Not measured. No other session reported a failure, but a killed `pnpm test` would read as a red
gate to its producer, who could spend a fix attempt or a re-run on it.

## Prevent by

`produce-bead`, *Workspace*, next to the `smoke-port` paragraph: a server a session starts for
validation is stopped by its own handle. That means the background task's id (TaskStop), or a pid
the session recorded when it started the server (`$!`, or `lsof -ti tcp:<port>` on the session's
own port). It is never stopped by a name pattern (`pkill -f`, `killall`).

## Seen before

Nothing like it in `docs/retrospectives/`.
