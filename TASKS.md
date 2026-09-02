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

## [T-001] TASKS.md is parsed by seven awk programs inside the launcher
scope: harness/tasks.py, harness/loop.sh, install.sh, selftest.sh, README.md
blockedBy: none
status: ready
rows: `selftest.sh::the launcher parses no task blocks itself`, `selftest.sh::tasks.py answers the queue against fixture files`, `selftest.sh::a heading inside a code fence is not a task`
criteria:
  - `harness/tasks.py` parses TASKS.md once and exposes the queue questions as a CLI:
    `ready-unattended`, `ids-at`, `field`, `block`, `set-status`, `unblock`, `rejections`.
  - `loop.sh` contains no `## [T-` pattern and no awk or sed program over TASKS.md.
  - `tasks.py --selftest` asserts the resolver against fixture queues and is run by `selftest.sh`.
  - A `## [T-###]` heading inside a fenced code block is not treated as a task. The awk could not
    see fences, which is how the block-format example in the template became a takeable task.
  - `./selftest.sh` passes.
notes: |
  Grounding: asdf-vm/asdf (25.6k stars) rewrote from Bash to Go at v0.16 and the maintainer's
  account names this exact failure — "Bash is very limiting when it comes to data structures. By
  default everything in Bash is just a string", and "all the essential complexity of asdf got lost
  in the accidental complexity of the Bash implementation"
  (stratus3d.com/blog/2025/02/03/asdf-has-been-rewritten-in-go/). They kept shell for plugins.
  Two parser defects in this repository came from the awk layer: BSD sed reading `[ \t]` as
  space-backslash-t, and `ready_unattended` offering a lane the `## [T-042]` example inside a code
  fence. python3 is already a hard dependency: probes.sh is 92% python behind a 42-line shim.

## [T-002] the launcher is one 632-line file
scope: harness/lib/*.sh, harness/loop.sh, install.sh, selftest.sh, README.md
blockedBy: T-001
status: blocked
rows: `selftest.sh::the launcher is split into sourced modules`
criteria:
  - `loop.sh` sources `agent.sh`, `queue.sh`, `gates.sh` and `telemetry.sh` from `__HARNESS_DIR__/lib/`.
  - `loop.sh` itself is under 300 lines and holds the stage sequence, the halts and the selftest.
  - `install.sh` writes the lib directory and `bash -n`s every file in it.
  - `./selftest.sh` passes, including `loop.sh --selftest`.
notes: |
  Grounding: pyenv (45.1k stars) is pure shell and healthy because it is `libexec/` full of small
  single-purpose scripts with Bats over them; nvm (94.8k) is the single-file model. Both bash
  harnesses found in the field (lSAAGl/loop-harness, gabrielkoerich/orchestrator-sh) split into
  lib/ as well. Bats is deliberately not adopted: it is a real dependency and the README claims
  none beyond bash and python3.
