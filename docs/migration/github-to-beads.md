# GitHub issues to beads

On 2026-09-24 the open GitHub issues were migrated to the Cerebro work board (beads, prefix
`nr`). Closed issues stayed on GitHub as history. Each bead carries `external_ref gh-<n>` and its
description ends with a link back to the issue; each issue got a comment naming its bead and stays
open so Moira (the user-feedback agent) can keep its status comments in step with the bead.

Rules applied: `bug` label became type bug, epic titles became type epic, `testing`/`automation`
only labels became type task, everything else type feature. Labels `enhanced`, `enhancement` and
`bug` were dropped (they are types or noise on the board); the rest were kept as bead labels. All
beads were created at P4 (unranked) and the 11 open children of #2825 were parented under its
bead. #3180 was closed on GitHub instead (delivered by PR #3184).

| Issue | Bead | Type | Parent |
|---|---|---|---|
| #2825 | nr-658 | epic |  |
| #2716 | nr-iek | epic |  |
| #2725 | nr-ffk | epic |  |
| #2833 | nr-658.1 | feature | nr-658 |
| #2835 | nr-658.2 | feature | nr-658 |
| #2837 | nr-658.3 | feature | nr-658 |
| #2839 | nr-658.4 | feature | nr-658 |
| #2840 | nr-658.5 | feature | nr-658 |
| #2841 | nr-658.6 | feature | nr-658 |
| #2842 | nr-658.7 | feature | nr-658 |
| #2843 | nr-658.8 | feature | nr-658 |
| #2844 | nr-658.9 | feature | nr-658 |
| #2845 | nr-658.10 | feature | nr-658 |
| #2851 | nr-658.11 | task | nr-658 |
| #3085 | nr-273 | feature |  |
| #3139 | nr-nr7 | bug |  |
| #3185 | nr-09s | task |  |
