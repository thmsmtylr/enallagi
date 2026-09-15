# DECISIONS

Rejected findings first, then archived blocks.

`## Rejected findings` is every proposal the adjudicator killed, one line each, with the command
that refutes it. Read it with `sed -n '/^## Rejected findings/,/^## \[T-/p' __HARNESS_DIR__/DECISIONS.md` and
never read past that range.

Below it, completed task blocks, verbatim, moved out of __HARNESS_DIR__/TASKS.md once `done` by
`harness run`. Each block is the implementer's and the verifier's own words, never summarised on
the way in.

## Rejected findings

<!-- - [YYYY-MM-DD] <the claim in one sentence> — refuted by `<command>`: <the output that refutes it> -->
