# LEARNINGS

<!-- One line per costly mistake: - [date] <what went wrong> → <rule instead> -->
<!-- Read at the start of every task. Rails are named in CLAUDE.md, never numbered. -->
<!-- Every entry names a file, a command or a hook. One that names nothing is unenforceable, -->
<!-- and probes.sh `learning-unenforced` will say so. -->

- [seed] **ZERO IS NOT PASS.** A build tool that ran no task, a hook that matched no file, a filter
  that selected no package and a glob that found nothing all exit 0. In every case the absence of a
  failure is indistinguishable from the absence of a check → a gate asserts what it EXECUTED, never
  merely that nothing failed. `.claude/hooks/check-gate.sh` fails closed when it cannot name what
  failed, for this reason.
- [seed] A build cache hashes its declared inputs, so a document a test reads that is not declared
  gives a green nobody ran → a document a test reads is declared as a cache input in the same commit
  (`turbo.json` `globalDependencies`, or your tool's equivalent), and a number quoted to a human comes
  from `__CHECK_FORCE__`, never the cached form.
- [seed] One checkout is one writer. Two sessions editing TASKS.md in one working directory ships
  two blocks with the same id → work in a worktree, and never `git add -A` when a second session may
  hold the same file. Stage the paths the task named.
- [seed] An implementation that is not committed is a lost iteration: a lane killed at a wait ceiling
  leaves the work in the tree with the task still reading `ready` → `__HARNESS_DIR__/loop.sh` makes the commit
  the last required step in its implement prompt, and warns when an iteration left `PROGRESS.md`
  unchanged (`git status --porcelain` after a lane is the check).
- [seed] Never use a harness control token as an English word in an agent prompt. "STOP" as prose
  once had an agent run `touch STOP` and halt a six-task run after one → say `halt the task` when
  you mean one task.
