---
name: release
description: "Cuts this project's release: a maintenance, minor or major release of NESER, from the next version number and player-readable release notes approved by the navigator, through the release pull request, to the tag that starts the release workflow and the GitHub Release it publishes. Load it whenever the navigator asks for a release, to release, to ship or publish a version, to tag a version, or asks what the next version would be — Cerebro's release handoff looks for exactly this skill."
---

# Cutting a release

A release is three things a player sees — a version number, release notes on the GitHub Releases
page, and the sentence the web frontend scrolls while no game is loaded — and one thing the
machines do: a `v*.*.*` tag that starts `.github/workflows/release.yml`, which builds every
platform archive, publishes to crates.io and creates the GitHub Release. This skill produces the
three, with the navigator's approval, and then pushes the tag. The navigator asks for a release
and approves what it says; everything else is decided here.

`scripts/prepare_release.py` makes the edits that have to be exact. The judgement — what the
release is about, what a player would call each change — is yours, and the navigator's word is
final on it.

## 1. Which kind, and from where

The navigator names the kind; if they only said "release", ask with the question tool:

| Kind | Version | When |
|---|---|---|
| maintenance | third digit +1 (`1.2.0` → `1.2.1`) | fixes only, nothing new to learn |
| minor | second digit +1, third to 0 (`1.2.1` → `1.3.0`) | new features, new systems, new controls |
| major | first digit +1, others to 0 (`1.3.0` → `2.0.0`) | the navigator's call: a milestone |

Then establish the ground, from the main checkout (never from another agent's worktree):

```bash
git fetch origin --tags
LAST=$(git describe --tags --abbrev=0 origin/main)          # the previous release
SINCE=$(git log -1 --format=%cI "$LAST")                      # when it was cut
python -m scripts.prepare_release --kind <kind> --print-version   # the version this release gets
gh run list --branch main --workflow ci.yml --limit 1 --json conclusion,headSha   # main must be green
gh pr list --state open --search "chore(release)" --json number,title                # no release in flight
```

Main red or a release PR already open: stop and say so. A release never ships a red main, and two
releases in flight would race the version.

## 2. What shipped, in the player's words

Read everything merged since the last tag. Three views, because each misses something:

```bash
git log "$LAST..origin/main" --format='%h %s' --no-merges               # every squash: subject names the bead
gh pr list --state merged --search "merged:>=${SINCE%%T*}" --limit 300 --json number,title,mergedAt
.cerebro/cerebro/scripts/work-beads --status closed --closed-after "$SINCE"   # beads, with their titles
```

The commit subject carries the bead id (`feat(nr-abc): …`); `bd show <id> --json` gives the
acceptance, which says what a person sees, and the labels, which say the system. That is where
the wording comes from: a release note says what changed for a player ("SNES games with the SA-1
chip run: Super Mario RPG, Kirby Super Star"), never what changed in the code ("SA-1 bus
arbitration and the IRAM mirror"). Group the work:

- **Highlights**: the three to five changes a player would tell a friend about. Their one-sentence
  join becomes the scroll text, so make each a short clause that reads well after a dash.
- **New**: every feature, one line each, naming the system and the frontend when it is not all
  of them.
- **Fixed**: every fix a player could have noticed, as the symptom it removes.
- **For developers**: tooling, CI, tests, refactoring — summarised in a few lines, not listed.

Write the notes in the format of `docs/releases/README.md`, to a scratch file outside the tree.
Then write the highlights sentence: the Highlights joined, full stops between, no double quotes
(it becomes a TypeScript string literal), under about 200 characters so the scroller reads it in
one pass.

Hundreds of commits since the last tag is normal here; read them all rather than the last page.
Sixty `docs(` commits are mockups and retrospectives and belong in no player section at all.

## 3. The navigator approves

Show, in one message: the kind and the version, the scroll sentence exactly as it will read with
today's date, and the full notes. Then ask with the question tool: approve as is, or edit (free
text). Apply every edit and show the result again until they approve. Nothing below happens on an
unapproved draft: the notes and the sentence are committed and tagged, and a tag is forever.

## 4. Land it through a pull request

Main only takes pull requests with green CI, and the release commit is a chore the navigator has
just read, so it gets CI and their approval, not a reviewer sub-agent (agreed 2026-09-27, nr-e08).

```bash
.cerebro/cerebro/scripts/prepare-worktree --path .cerebro/worktrees/release-v<version> \
    --branch release-v<version> --from origin/main
cd .cerebro/worktrees/release-v<version>
python -m scripts.prepare_release --kind <kind> --highlights "<sentence without the date and version>" --notes <scratch notes file>
git status --porcelain            # exactly: Cargo.toml, Cargo.lock, web/src/app.ts, docs/releases/v<version>.md
cargo metadata --format-version 1 --offline > /dev/null   # the lockfile still parses with the new version
./scripts/gate-full.sh            # every pull request runs the full gate before it is opened
git add -A && git commit -m "chore(release): v<version>"
git push -u origin release-v<version>
gh pr create --title "chore(release): v<version>" --body "Release v<version>: version bump, scroll text and docs/releases/v<version>.md, approved by the navigator in the release session."
```

`--highlights` takes only the changes; the script prepends the date and the version. Then wait
for CI inside a tool call, not by ending the turn:

```bash
until gh pr checks <n> 2>/dev/null | grep -qv pending; do sleep 60; done; gh pr checks <n>
gh pr merge <n> --squash --delete-branch
```

A red check is read before it is believed; a flake is re-run once; anything else stops the
release and goes to the navigator. Never `--auto`, never a push to main.

## 5. Tag the merge commit, then watch the workflow

The branch's own commits are gone after the squash; the tag goes on the commit main now has:

```bash
git fetch origin main
MERGE=$(git log origin/main -1 --format=%H --grep "chore(release): v<version>")
git tag -a v<version> "$MERGE" -m "NESER v<version>"
git push origin v<version>
until RUN=$(gh run list --workflow release.yml --branch v<version> --limit 1 --json databaseId -q '.[0].databaseId') && [ -n "$RUN" ]; do sleep 30; done
gh run watch "$RUN" --exit-status
gh release view v<version> --json url,body -q '.url + "\n" + .body' | head -5   # the body is the approved notes
```

The workflow uses `docs/releases/v<version>.md` as the Release body, so the notes need no edit
afterwards; if the body is git-cliff's commit list instead, the file name and the tag disagree,
and the fix is `gh release edit v<version> --notes-file docs/releases/v<version>.md`. A failed
workflow is reported with its run URL; the tag stays, the fix is a bead (suggest P0), and the
navigator decides whether to re-run or cut a maintenance release on top.

## 6. Say what shipped

One message: the version, the Release URL, the highlights, and anything that did not go to plan.
Moira moves the released issues on her next pass and Cerebro counts from the new tag; neither
needs prompting.

## Traps

- **The scroll text is a string literal.** Double quotes in the highlights break `app.ts`; the
  script refuses them. Keep an apostrophe rather than a typographic one.
- **The notes file is the tag.** `docs/releases/v1.3.0.md` serves tag `v1.3.0` and nothing
  else; the script names it from the version it computed, so do not rename it.
- **`Cargo.lock` carries the version too.** The script edits it; a `cargo build` after a manual
  bump would rewrite it and dirty the tree.
- **`gh pr merge --delete-branch` sometimes reports failure after the merge succeeded.** Check
  `gh pr view <n> --json state` before doing anything twice.
- **A hand-driven session is the navigator.** When a person runs this by hand, their approval in
  the terminal is the approval; the question tool still asks it.
