# TASKS

The queue. One block per task, in the order they were written. The loop takes the first `ready`
block whose blockers are all `done` and which is not `attended: true`.

Statuses: `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review`
(the implementer's last act) · `done` (only the verifier's, and the launcher re-runs the gate
behind it) · `needs-spec` (the contract does not answer a question the task hit).

Finished blocks archive to DECISIONS.md — `.harness/archive-done.sh` moves them at the top of each
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

## [T-001] the role prompts narrate their own history at the model
scope: roles/*.md, selftest.sh
blockedBy: none
status: ready
rows: `selftest.sh::no role prompt carries an incident narrative`, `selftest.sh::the trimmed role prompts still pass their evals`
criteria:
  - Every hard rule, named rail, protocol step and output format in each role prompt survives.
  - Removed: dated incident narratives, reproduction stories, cross-references to task ids, the
    duplicated agentskills.io paragraph, and rationale that restates the rule it follows.
  - `./selftest.sh` greps the role prompts for incident narrative and fails on a hit.
  - `evals/run.sh` passes all three against a real agent after the trim. A prompt trim is a
    behaviour change, and the evals are the only check that can see it.
notes: |
  Grounding: instructions in a context file are followed — a tool named in one is used ~1.6 times
  per task against almost never when unnamed — and the same study measured LLM-generated context
  files costing about 3% of success rate and over 20% in inference cost, concluding that human
  written context files "should describe only minimal requirements"
  (arXiv:2602.11988). So the risk of a long prompt is not that it is ignored; it is that every
  unnecessary requirement in it makes the task harder. This is the same finding AGENTS.md already
  cites at its own foot, applied to the files that are four to eight times longer.
