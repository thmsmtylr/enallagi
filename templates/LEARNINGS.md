# LEARNINGS

<!-- The `[seed]` rules this install shipped, read at the start of every task. `enallagi init` seeds this file from the binary, so a line appended here is gone at the next install and `enallagi probe` `learning-ungated` reports it.
     A rule the loop earns goes under `## Earned rules` in __ENALLAGI_DIR__/DECISIONS.md, written by the adjudicator. Capped (learningsCap) across the two files: at the cap, adding a rule means removing one. Every entry names a file, a command or a hook, or `learning-unenforced` reports it. Rails are named in CLAUDE.md, never numbered. -->

- [seed] **ZERO IS NOT PASS.** A build tool that ran no task, a hook that matched no file, a filter
  that selected no package and a glob that found nothing all exit 0. In every case the absence of a
  failure is indistinguishable from the absence of a check → a gate asserts what it EXECUTED, never
  merely that nothing failed. `src/hooks.rs`'s `enallagi hook verify-done` fails closed when it
  cannot name what failed, for this reason.
- [seed] A build cache hashes its declared inputs, so a document a test reads that is not declared
  gives a green nobody ran → a document a test reads is declared as a cache input in the same commit
  (`turbo.json` `globalDependencies`, or your tool's equivalent), and a number quoted to a human comes
  from `__CHECK_FORCE__`, never the cached form.
- [seed] One checkout is one writer. Two sessions editing __ENALLAGI_DIR__/TASKS.md in one working directory ships
  two blocks with the same id → work in a worktree, and never `git add -A` when a second session may
  hold the same file. Stage the paths the task named.
- [seed] An implementation that is not committed is a lost iteration: a lane killed at a wait ceiling
  leaves the work in the tree with the task still reading `ready` → `enallagi run` makes the commit
  the last required step in its implement prompt, and warns when an iteration left `__ENALLAGI_DIR__/PROGRESS.md`
  unchanged (`git status --porcelain` after a lane is the check).
- [seed] Never use a harness control token as an English word in an agent prompt. "STOP" as prose
  once had an agent run `touch STOP` and halt a six-task run after one → say `halt the task` when
  you mean one task.
