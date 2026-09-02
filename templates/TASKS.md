# TASKS

The queue. One block per task, in the order they were written. The loop takes the first `ready`
block whose blockers are all `done` and which is not `attended: true`.

Statuses: `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review`
(the implementer's last act) · `done` (only the verifier's, and the launcher re-runs the gate
behind it) · `needs-spec` (the contract does not answer a question the task hit).

Finished blocks archive to DECISIONS.md — `__HARNESS_DIR__/archive-done.sh` moves them at the top of each
iteration, leaving a stub with the fields the loop still reads. Keep this file small: every
process an iteration spawns re-reads all of it.

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

`T-###` above is written unnumbered on purpose: `## [T-<digits>]` is the exact shape
`loop.sh`'s `ready_unattended` scans for, and it does not know this one is inside a code fence —
a numbered example here is a task the loop will take. `blockedBy:` is empty, `none`, or a
comma-separated list of ids. `attended: true` marks a task
needing a human credential and no launcher auto-selects it.

---

## [T-001] <the first task>
scope:
blockedBy: none
status: ready
rows: none — harness
criteria:
  - <what has to become true, and how you would see it>
notes:
