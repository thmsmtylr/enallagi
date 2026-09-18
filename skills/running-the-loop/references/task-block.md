# The task block format

```
## [T-001] one line, in the finding's own words
scope: src/thing.ts, src/thing.test.ts
blockedBy:
status: ready
rows: <the exit-criteria rows this turns green, character for character, or `none — harness`>
criteria:
  - <objective, and naming the command whose output changes when it is done>
notes: <what a reviewer should scrutinise; the implementer's and the verifier's own words>
```

| Field | Meaning |
| --- | --- |
| `scope:` | comma-separated globs. A directory is written `dir/**`, since a bare `dir` matches none of its files. The `one-scope` rail: touching anything outside them is a rejection. Write the line from the consumers the criteria's own commands list, never from a count. Those are the files `grep -rln <symbol>` finds over the tree, the rendered copy of every template named, and the test that hashes a file named. A path added later needs a one-line `widened:` reason, which the verifier grades. |
| `blockedBy:` | empty, `none`, or comma-separated task ids. The launcher will not select a task whose blockers are not all `done`. |
| `status:` | `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review` (the implementer's last act) · `done` (the verifier's, and the launcher re-runs the gate behind it) · `needs-spec` (the contract does not answer a question the task hit). |
| `rows:` | the exit-criteria rows the task turns green, copied character for character, or `none — harness` / `none — measurement`. This is how the one-row rail binds without a second queue. |
| `criteria:` | must be runnable by an agent that has read only the context file, the spec, __ENALLAGI_DIR__/LEARNINGS.md and this block. Criteria assuming conversational context are unrunnable. |
| `notes:` | the evidence, quoted as found, including another repository's identifiers. `criteria:` describes a finding from another repository by its shape, never by those identifiers, and so do the fixtures, test names and evals written from it. |
| `attended: true` | the task needs a human credential. No launcher auto-selects it. |
| `probe:` `command:` `output:` | required on a `proposed` block. The `anchored` rail: a finding with none of these is killed unread. |

A block is at most about thirty minutes of human-equivalent work. Split it if it is more.
