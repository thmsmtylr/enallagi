# <project>

<!-- The core context file. Read by 20+ coding agents; the tool-specific files beside it are
     one-line pointers to this one. Keep it SHORT: context files are followed but over-specifying
     costs success — see the note at the bottom before you add to it. -->

## Commands

- Verify, and this is what done means: `__CHECK__`
- Uncached, for any number you quote to a human: `__CHECK_FORCE__`
- One loop iteration: `__HARNESS_DIR__/loop.sh 1`; `touch STOP` halts it
- What the tree says about itself: `__HARNESS_DIR__/hooks/probes.sh`

## How work moves

`__SPEC__` is what we build. **TASKS.md** is the queue, **PROGRESS.md** the loop's record one
iteration at a time, **LEARNINGS.md** the mistakes already paid for, **DECISIONS.md** the archive.
Read LEARNINGS.md at the start of every task; it is short on purpose.

A task is `proposed` → `ready` → `review` → `done`. The scout proposes only from a `FINDING` line,
the adjudicator promotes or kills, the implementer sets `review` and never `done`, the verifier
promotes and never fixes, and the launcher re-runs the gate behind the verdict. Touch only the files
on the task's `scope:` line; an out-of-scope need is a note and a stop.

## The rails

Named, never numbered, and each names what enforces it — a rail with no enforcement is a wish.
The full table, with the evidence behind each one, is `__HARNESS_DIR__/RAILS.md`. The five that
decide most reviews:

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

<!-- On length: across 138 real tasks and four agents, context files raised inference cost by over
     20%; LLM-generated ones cost about 3% of success rate and human-written ones bought about 4%
     (Gloaguen et al., arXiv:2602.11988). Instructions ARE followed — a tool named here is used
     ~1.6 times per task versus almost never when unnamed — so the risk is not that this file is
     ignored, it is that every unnecessary requirement in it makes the task harder. Add a line here
     only when an agent got it wrong without one. Detail belongs in a skill or a reference file the
     agent loads on demand, not here. -->
