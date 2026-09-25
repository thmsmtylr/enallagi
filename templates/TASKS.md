# TASKS

The queue. One block per task, in the order they were written. The loop takes the first `ready`
block whose blockers are all `done` and which is not `attended: true`.

Statuses: `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review`
(the implementer's last act) · `done` (only the verifier's, and the launcher re-runs the gate
behind it) · `needs-spec` (the contract does not answer a question the task hit).

Finished blocks archive to __ENALLAGI_DIR__/DECISIONS.md — `enallagi run` moves them at the top of each
iteration, leaving a stub with the fields the loop still reads. Keep this file small.

## Block format

```
## [T-###] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy:
status: ready
rows: <the exit-criteria rows this turns green, character for character, or `none — harness`>
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: <what a reviewer should scrutinise; the implementer's and the verifier's own words>
```

Keep the example above unnumbered: `enallagi tasks ready` scans for `## [T-<digits>]` and does not
know a code fence from a block. `blockedBy:` is empty, `none`, or a comma-separated list of ids.
A directory in `scope:` is written `dir/**`. A bare `dir` matches only itself, never a file beneath it.
`attended: true` marks a task needing a human credential and no launcher auto-selects it.

---
