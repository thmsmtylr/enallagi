# The ticket file

One file per ticket, `__ENALLAGI_DIR__/specs/<spec>/issues/T-NNN-<slug>.md`, where `<slug>` is the
title's first words, lowercase, joined by `-`. `enallagi tasks block <id>` prints one ticket as the
launcher reads it; `ls __ENALLAGI_DIR__/specs/*/issues/<id>-*` names its file.

```
# T-001: one line, in the finding's own words

**Status:** ready-for-agent
**Blocked by:** None

## Acceptance criteria

- [ ] <objective, and naming the command whose output changes when it is done>

## Comments

<what a reviewer should scrutinise; the implementer's and the verifier's own words>

## Harness

scope: src/thing.ts, src/thing.test.ts
rows: <the exit-criteria rows this turns green, character for character, or `none — harness`>
```

The first line is exactly `# T-NNN: title`. A file the launcher cannot parse stops every read of the
queue. No line under `## Comments` opens with `## `: that ends the section.

| Part | Meaning |
| --- | --- |
| `**Status:**` | `needs-triage` (the scout's or the verifier's filing, inert to the loop) · `ready-for-agent` · `ready-for-human` (needs a person or a human credential; no launcher takes it) · `review` (the implementer's last act) · `open` (the launcher's, once it opens the ticket's pull request under `[pr] per_task`; no lane takes it) · `done` (the verifier's, and the launcher re-runs the gate behind it; under `[pr] per_task` the launcher's, when the pull request merges) · `needs-info` (the spec does not answer a question the ticket hit; the run halts) · `wontfix` (killed, with its reason; the file stays). |
| `**Blocked by:**` | `None`, or comma-separated ticket ids. The launcher takes no ticket whose blockers are not all `done` or `open`. |
| `## Acceptance criteria` | `- [ ]` items, runnable by an agent that has read only the context file, the spec, __ENALLAGI_DIR__/LEARNINGS.md and this ticket. Criteria assuming conversational context are unrunnable. |
| `## Comments` | the evidence, quoted as found, including another repository's identifiers, then each role's notes and verdicts in turn. The criteria describe a finding from another repository by its shape, never by those identifiers, and so do the fixtures, test names and evals written from it. |
| `scope:` | comma-separated globs. A directory is written `dir/**`, since a bare `dir` matches none of its files. The `one-scope` rail: touching anything outside them is a rejection. Write the line from the consumers the criteria's own commands list, never from a count. Those are the files `grep -rln <symbol>` finds over the tree, the rendered copy of every template named, and the test that hashes a file named. A path added later needs a one-line `widened:` reason, which the verifier grades. |
| `rows:` | the exit-criteria rows the ticket turns green, copied character for character, or `none — harness` / `none — measurement`. This is how the one-row rail binds without a second queue. |
| `probe:` `command:` `output:` | required on a `needs-triage` ticket. The `anchored` rail: a finding with none of these is killed unread. |
| `gate:` | written by the launcher when a gate sends the ticket back; the newest is first. |
| `verdict:` | `verified by verifier`, `rejected by verifier: <reason>` or `answered by implementer`, replaced each time. The `verdict` gate checks it against `**Status:**`. |
| `spec:` `standards:` | directly under `verdict:`, the verifier's findings on each review axis, or `none`. A `done` missing either goes back to `ready-for-agent`. |
| `ran:` `exit:` | pairs under the verdict: a command a claim rests on and the exit it gave. The `claims` gate re-runs them. |
| `pr:` `branch:` `polled:` | written by the launcher: the open pull request's URL, its head branch, and the poll's last key (head sha, newest comment and review, check rollup). |
| `deferred:` | the verifier's `needs-triage` filing for a defect outside the criteria, by id and reason. The `commit-verdict` gate reads it. |

A lane edits only its own ticket file. The one other file it may add is a new ticket at
`needs-triage`; the scope gate rejects any other change under `__ENALLAGI_DIR__/specs/`. A killed
ticket is set to `wontfix` with its reason, never deleted, and its kill line is recorded in
`__ENALLAGI_DIR__/.out-of-scope/<concept>.md`.

A new ticket takes one past the highest id ever used, over every ticket file and every id
__ENALLAGI_DIR__/DECISIONS.md names: `{ ls __ENALLAGI_DIR__/specs/*/issues; cat __ENALLAGI_DIR__/DECISIONS.md; } | grep -o 'T-[0-9]*' | sort -t- -k2 -n | tail -1`.
A spent id is never taken again. `enallagi probe` → `queue-hygiene` reports a ticket that reuses one.

A ticket is at most about thirty minutes of human-equivalent work. Split it if it is more.
