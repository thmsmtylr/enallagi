# DECISIONS

Earned rules first, then rejected findings, then archived blocks.

`## Earned rules` is every rule this loop paid for, one dated line each, written by the adjudicator.
Read it with `sed -n '/^## Earned rules/,/^## Rejected findings/p' __ENALLAGI_DIR__/DECISIONS.md` at the
start of every task. A line carries the `enallagi eval --gate <name>` run that admitted it or the
command that showed what it cost, and names a file, a command or a hook.

`## Rejected findings` is every proposal the adjudicator killed, one line each, with the command
that refutes it. Read it with `sed -n '/^## Rejected findings/,/^## \[T-/p' __ENALLAGI_DIR__/DECISIONS.md` and
never read past that range.

Below it, completed task blocks, verbatim, moved out of __ENALLAGI_DIR__/TASKS.md once `done` by
`enallagi run`. Each block is the implementer's and the verifier's own words, never summarised on
the way in.

## Earned rules

<!-- - [YYYY-MM-DD] <what went wrong> → <the rule instead> (`<the --gate run or the command>`) -->

## Rejected findings

<!-- - [YYYY-MM-DD] <the claim in one sentence> — refuted by `<command>`: <the output that refutes it> -->
