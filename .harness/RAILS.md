# The rails, in full

Loaded on demand. `AGENTS.md` carries the five that decide most reviews; this file carries
all of them with the enforcement and the evidence, so the context file every agent loads on every
session stays short.

Rails are named, never numbered. A prompt citing `the bounded rail` survives an edit to this file's
ordering; one citing "rule 11" does not. **Every rail names what enforces it, because a rail with no
enforcement is a wish** — and a rail enforced by an agent's judgment says so, rather than naming a
mechanism that does not run.

## Rails

Rails are named, never numbered. A prompt citing `the bounded rail` survives an edit to this file's
ordering; one citing "rule 11" does not. **Every rail names what enforces it, because a rail with no
enforcement is a wish** — and a rail enforced by an agent prompt says so, rather than naming a
mechanism that does not run.

### Product rails

Yours to write. One row per thing that, if broken, makes this a different and worse product. Each
names a test by `file::test name`, copied character for character from SPEC.md's exit criteria, so
that a rename on either side is a rename on both.

<!-- Uncomment and fill in. Until a row exists here the product rails are unwritten, which is
     a real state to be in early and a bad one to stay in.

| Rail | What it means | Enforced by |
| --- | --- | --- |
| `<name>` | <what it means, and the path that gets forgotten> | the test that proves it, written file::name in backticks |
-->

### Process rails

Each exists because something specific went wrong.

| Rail | What it means | Enforced by |
| --- | --- | --- |
| `green` | Done means `./selftest.sh` passes. Never weaken, skip or delete a test or a lint rule to get there. **Until every exit-criteria row has a test the tree cannot reach zero failures, so done is verified on delta against `.check-baseline`**: a failure listed there is inherited, a failure not listed there is a rejection, and the file only ever shrinks. Adding a line to it is weakening a test by another name. | `.harness/loop.sh` → `gate_verdict` → `check-gate.sh` · `.check-baseline` (plus the `Stop` hook, where your tool has hooks) |
| `citable` | Every claim in code, a comment, a task note, a verdict or a document carries its source: a URL with the date checked, a `file:line`, or the command and its output. No source, no claim. | judgment — the verifier |
| `measure-first` | Nothing that moves a number lands before a measurement, taken on the current tree, of the thing it claims to fix. A failing threshold is not evidence the threshold is wrong. Stamp the revision a number came from. | judgment — the verifier |
| `contracts` | Every shape crossing a boundary is declared once in `harness.default.json`, parsed at the write path, frozen for the milestone. Never invent an interface, import it. | `typecheck` |
| `one-scope` | Touch only files inside the task's `scope:` globs. An out-of-scope need is a note on the task and a stop, never a quiet edit. | `.harness/loop.sh` → `gate_scope`, diffing the iteration's own commits against `scope:` · judgment — the verifier |
| `minimal` | Walk the ladder: does it need to exist → already here → stdlib → the platform → an installed dependency → one line → only then a minimal build. Product rails are never on the chopping block. | judgment — the verifier |
| `no-invented-strategy` | Never state a business model, sequence, price or market position that was not given to you. An open question is written as an open question. A plausible inference stated as fact is worse than a blank, because the next agent plans against it. | judgment — the verifier |
| `tidy` | The tree holds the product and the documents that govern it. A throwaway experiment is deleted the moment its number exists. Gitignored is not absent. Deleted work stays citable by sha: `git show <sha>:<path>`. | `probes.sh` → `litter` |
| `anchored` | A finding enters the queue only with the probe, the command and the output that produced it; an unanchored finding is a rejection, not a task. | judgment — the adjudicator |

> **Hooks are an adapter, not the enforcement.** They are configured per tool — Claude and Gemini in
> `settings.json`, Copilot in `.github/hooks/*.json`, Cursor in `hooks.json` — and **Codex has none**
> (arXiv:2602.14690, Table 1). Every rail below that could rest on a hook rests on the launcher
> instead, which runs everywhere. Where your tool has hooks, they are a second line that catches a
> violation in-session rather than at the gate.

### Loop rails

| Rail | What it means | Enforced by |
| --- | --- | --- |
| `tests-immutable` | Every file named in the exit criteria is immutable. The SHA-256 in `test-hashes.json` and the `PreToolUse` hook **detect** an edit and force it to land visibly; they do not prevent one, because the check runs as the same principal as the lane over data the lane can write. **The authority is the verifier**: a `test-hashes.json` key re-cut for a file not on the task's `scope:` line is a rejection. | the verifier · `test-hashes.json` (plus the `PreToolUse` hook, where your tool has hooks) |
| `harness-immutable` | The build config, every preload, the check script **and `.harness/loop.sh` itself** are covered by the same hashes. The gate cannot be edited by the thing it gates, and the scheduler is part of the gate. Freeze the **exam** as well as the tests: whatever files hold your approved corpus, your fixtures and your scoring rules belong here too, or a lane can retune what it is graded on. | the verifier · `test-hashes.json`, and `probes.sh` → `hash-uncovered` for every file this row names |
| `one-row` | A task is at most the exit-criteria rows it names under `rows:`. Split it if it is more. Write `PROGRESS.md` at the end of every iteration; re-read its tail, SPEC.md and `git log --oneline -20` at the start of the next. | `.harness/loop.sh` |
| `minutes-not-hours` | No task is more than about 30 minutes of human-equivalent work. Split it if it is. | judgment — the adjudicator, at promotion |
| `blocked-is-allowed` | A task may stop with `BLOCKED` and a written reason, and that is a success. A task may **never** be marked done without the exact command and its pasted output. | `.harness/loop.sh` → `gate_verdict` |
| `no-clarification-left` | If any `[NEEDS CLARIFICATION]` marker exists in SPEC.md, the loop does not start. | `.harness/loop.sh` |
| `harness-lane` | One round changes a product lever or the measure of that lever, never both. A diff touching the launcher, the hooks, the check script, `.check-baseline` or `test-hashes.json` belongs to a task that declared `rows: none — harness`. | `.harness/loop.sh` → `gate_scope` |
| `friction` | Every iteration's PROGRESS.md entry ends with the round's question — what cost time that a rule or a check could prevent. The first occurrence is evidence and stays in PROGRESS.md; the **second** occurrence of the same thing becomes a line in LEARNINGS.md. A rule per surprise is how a harness rewrites its operating system every week and gets worse. | judgment — the verifier, for the line's presence · `probes.sh` → `friction-repeat`, for the second occurrence |
| `verifier-not-implementer` | Final acceptance runs in a fresh session that sees only the diff and the exit criteria. It never sees the implementation conversation. | `.harness/loop.sh` · `.harness/roles/verifier.md` |

## Task protocol

- Tasks live in TASKS.md: id, `scope:` globs, `blockedBy:`, `rows:`, objective acceptance criteria,
  status of proposed, ready, blocked, review, done or needs-spec.
- **The scout proposes and the adjudicator promotes or kills.** `proposed` is the scout's output and
  nobody else's — one block per `FINDING` line from `.harness/hooks/probes.sh`, carrying `probe:`, the
  command and its output — and the adjudicator either writes runnable criteria and sets `ready`, or
  kills it to `## Rejected findings` in DECISIONS.md. A `proposed` block is inert to the loop.
- `rows:` names the exit-criteria rows the task turns green, copied character for character. A task
  with no rows writes `rows: none` and says whether it is harness or measurement.
- Take the first ready task whose blockers are done. Restate its acceptance criteria in one
  sentence before coding; if you cannot, mark it `needs-spec` with the question.
- Implementer sets `review`, never `done`. Verifier promotes to `done` or rejects to `ready` with
  reproducible reasons, and the launcher then re-runs the gate itself. A lint failure alone is a
  rejection.
- A task must be completable by an agent that has read only `AGENTS.md`, SPEC.md, LEARNINGS.md and
  its own task block. Criteria assuming conversational context are unrunnable.
- `attended: true` marks a task needing a human credential. No launcher picks one up on auto-select.
- **One checkout is one writer.** Two sessions editing TASKS.md in one working directory is how a
  merge lands under a session that read the tree forty minutes earlier.

## Commands

- Verify — this is what done means: `./selftest.sh`
- Run the loop: `.harness/loop.sh [iterations]`, `touch STOP` to stop it before the next stage
- See what the tree says about itself: `.harness/hooks/probes.sh`
- Watch a run from a second terminal: `.harness/watch.sh`
