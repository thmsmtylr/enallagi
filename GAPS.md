# GAPS

What is still missing from this harness. The file exists so the absences are decisions rather than
oversights — and so the list stays short enough to be true.

Everything the first edition of this file listed as broken is fixed, and the fixes are readable as
commits rather than as prose here: `git log --oneline` on this repository walks the `driver` probe,
`gate_scope`, `hash-uncovered`, `queue-uncovered`, `friction-repeat`, the `PROGRESS.md` rollover,
worktree isolation and the role evals, each with the assertion that proves it. What remains is below.

---

## 1. The driver drives nothing until you write it

`.harness/driver.example.sh` is the skeleton, `driver.sh` in this package is a worked example that
drives the harness itself, and `driverCommand` is empty by default. Only you know what your
artifact's surface is. Until you fill the skeleton in, eleven of the twelve probes read text and the
loop can only find work a grep over the repo can see. **This is still the ceiling.** Everything else
on this list is smaller than it.

## 2. Parallel lanes

`.harness/worktree.sh` isolates *one* lane — its own checkout, its own branch, `--ff-only` back, and
a refusal to merge if the parent moved. Running several at once is not shipped: the multi-worktree
launcher in the repo this came from was 23KB and unused for months. Add lanes when you have
file-disjoint scopes to fight over, not before.

## 3. A held-out suite

`verifier-not-implementer` buys a fresh session and a separate process, which is most of the value.
A suite the implementer never sees is more, and it is yours to write. Freeze your **exam** with your
tests while you are there — the corpus, the fixtures and the scoring rules belong in
`test-hashes.json`, or a lane can retune what it is graded on. `hash-uncovered` holds you to whatever
`harness-immutable` names.

## 4. Two roles have no eval

`evals/` covers the three that gate the queue — scout, adjudicator, verifier. The implementer and
the researcher have none. Both are two files and a fixture; the pattern is in place.

## 5. `test-hashes.json` is not written for you

A fresh install emits exactly one `hash-uncovered` finding, naming `loop.sh`. That is deliberate:
the hashes are yours to cut, and a rail that claims coverage it does not have is worse than one that
says so out loud. Cut them, and the gate covers its own scheduler.

## 6. Corpus-level validation

Spec Kit's `converge` validates a finished implementation back against spec, plan and tasks as a
whole. `queue-uncovered` does the coverage half — a criterion with no task, a task naming a row that
does not exist — and the verifier does the per-task half. Nothing does the whole corpus at once.

## 7. `STATE.md`

The article separates a rewritten-each-round state file from an append-only archive. Here `TASKS.md`
is the queue and `PROGRESS.md` is the state, trimmed by `archive-done.sh` past a few thousand lines.
That is a judgment call, not a defect, and it is written down in case you disagree.

## 8. Known probe edges

- `rejection-stale` flags a `done` block whose `notes:` contain the word REJECTED, which is what a
  block looks like after a rejection that was then addressed and verified. Archiving the block to
  `DECISIONS.md` clears it.
- `friction-repeat` matches normalised text, so a reworded repeat escapes it. The ceiling is marked
  in the source.
- `gate_scope` matches `scope:` globs with shell `case`, where `*` crosses `/`. It is permissive:
  `src/*` covers `src/a/b.ts`.

---

The harness was run against itself to build the three capabilities above it — install it into its own
repo, write the exit criteria, work the queue. That run is in this history, and it is the honest
argument for the thing: it found a task the loop should never have been offered (the block-format
example in `TASKS.md` was a real, takeable task), a gate that let uncommitted work reach `done`, and
zero-as-pass twice in code written that same day to prevent zero-as-pass. None of those came from
reading the code.
