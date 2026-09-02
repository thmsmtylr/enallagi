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
| `scope:` | comma-separated globs. The `one-scope` rail: touching anything outside them is a rejection. An out-of-scope need is a note on the task and a stop, never a quiet edit. |
| `blockedBy:` | empty, `none`, or comma-separated task ids. The launcher will not select a task whose blockers are not all `done`. |
| `status:` | `proposed` (the scout's, inert to the loop) · `ready` · `blocked` · `review` (the implementer's last act) · `done` (the verifier's, and the launcher re-runs the gate behind it) · `needs-spec` (the contract does not answer a question the task hit). |
| `rows:` | the exit-criteria rows the task turns green, copied character for character, or `none — harness` / `none — measurement`. This is how the one-row rail binds without a second queue. |
| `criteria:` | must be runnable by an agent that has read only the context file, the spec, LEARNINGS.md and this block. Criteria assuming conversational context are unrunnable. |
| `attended: true` | the task needs a human credential. No launcher auto-selects it. |
| `probe:` `command:` `output:` | required on a `proposed` block. The `anchored` rail: a finding with none of these is killed unread. |

A block is at most about thirty minutes of human-equivalent work. Split it if it is more.
