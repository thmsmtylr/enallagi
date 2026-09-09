# <project>

<!-- The core context file, read by every agent on every session. Keep it SHORT; detail belongs in a
     skill or a reference file the agent loads on demand. -->

## Commands

- Verify, and this is what done means: `__CHECK__`
- Uncached, for any number you quote to a human: `__CHECK_FORCE__`
- One loop iteration: `harness run --iterations 1`; `touch STOP` halts it
- What the tree says about itself: `harness probe`

## How work moves

`__SPEC__` is what we build. **TASKS.md** is the queue, **PROGRESS.md** the loop's record one
iteration at a time, **LEARNINGS.md** the mistakes already paid for, **DECISIONS.md** the archive.
Read LEARNINGS.md at the start of every task.

A task is `proposed` → `ready` → `review` → `done`. The scout proposes only from a `FINDING` line,
the adjudicator promotes or kills, the implementer sets `review` and never `done`, the verifier
promotes and never fixes, and the launcher re-runs the gate behind the verdict. Touch only the files
on the task's `scope:` line; an out-of-scope need is a note and a stop.

## The rails

Named, never numbered, and each names what enforces it.
The full table is `__HARNESS_DIR__/RAILS.md`. The five that decide most reviews:

- `green` — done means `__CHECK__` passes, verified on **delta** against `.check-baseline`, which
  only ever shrinks. Never weaken a test or a lint rule to get there.
- `citable` — every claim carries its source: a URL with the date, a `file:line`, or the command and
  its output.
- `measure-first` — no number lands that was not run and stamped. Never tune a constant to pass.
- `one-scope` — only the files the task's `scope:` names.
- `blocked-is-allowed` — stopping with a written reason is a success. Marking done without the
  pasted command output never is.

## Style

<!-- Yours. Keep it to what an agent would otherwise get wrong. -->

---

<!-- Add a line here only when an agent got it wrong without one. Detail belongs in a skill or a
     reference file the agent loads on demand, not here. -->
