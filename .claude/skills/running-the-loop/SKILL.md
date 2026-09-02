---
name: running-the-loop
description: Use when working in a repository that has a .harness directory - explains the task queue, the four roles and their authority, what "done" means here, and the rails a change has to survive. Invoke before taking a task from TASKS.md, before marking anything done, and before proposing new work.
---

# Running the loop

This repository is driven by an autonomous goal loop. Work moves through a queue, each task runs in
a fresh session, and every claim is gated on a command someone else can run.

## Before you touch anything

Read `AGENTS.md`, `LEARNINGS.md`, and the task's own block in `TASKS.md`. Read the **tail** of
`PROGRESS.md` — it is append-only and newest-last, so reading it from the top gives you the oldest
entries and none of the handoff.

Restate the task's acceptance criteria in one sentence before writing code. If you cannot, the task
is `needs-spec` and the question goes in its `notes:`.

## The four roles, and why they are separate

An agent that finds its own work and then grades it is not a loop. Each role has one job and one
thing it may never do:

| Role | Does | May never |
| --- | --- | --- |
| scout | turns `FINDING` lines from `hooks/probes.sh` into `status: proposed` blocks | have a finding of its own, promote, or fix |
| adjudicator | promotes a proposal to `ready` with runnable criteria, or kills it with the command that refutes it | write a proposal, or edit a file a block names |
| implementer | one task, inside its `scope:` globs, test first | mark anything `done` |
| verifier | fresh session, adversarial, promotes to `done` or rejects with reproducible reasons | fix code |

The full prompts are in `.harness/roles/`. Read the one for the role you are playing.

## What done means

`./selftest.sh` passes, verified on **delta** against `.check-baseline` — a failure listed there is
inherited, a failure not listed there is a rejection, and the file only ever shrinks. Adding a line
to it is weakening a test by another name.

Paste the exact command and its output. A task may never be marked done without them. The launcher
re-runs the gate behind the verifier and forces back to `ready` any `done` the tree cannot support,
so write the verdict you can defend against a command someone else runs.

It re-runs the **scope** check too: the iteration's own commits, diffed against the task's `scope:`
globs. A file the scope line does not name sends the task back to `ready`, and so does a diff
touching the launcher, the hooks, the check script, `.check-baseline` or `test-hashes.json` under a
task whose `rows:` is not `none — harness` — one round changes a product lever or the measure of
that lever, never both.

Stopping with `BLOCKED` and a written reason is a success, not a failure.

## The round's question

Every iteration's `PROGRESS.md` entry ends with `friction:` — one thing that cost time and a rule or
a check could prevent, or `none`. The first occurrence is evidence and stays there. The **second**
occurrence of the same thing becomes a line in `LEARNINGS.md`, and `probes.sh` → `friction-repeat`
keeps emitting it until it is. One rule per surprise rewrites the operating manual every week, which costs more than the friction it removes.

## The rails

`.harness/RAILS.md` has all of them with their enforcement. The ones that decide most
reviews: `green`, `citable` (every claim carries its source), `measure-first` (no number that was
not run and stamped), `one-scope` (only the files the task names), `blocked-is-allowed`.

## References

- `references/task-block.md` — the block format and every field's meaning
