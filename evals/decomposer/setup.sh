#!/usr/bin/env bash
# One accepted spec, `calc`, with no tickets and three exit criteria; the usage row needs both
# operations, so a right decomposition has a blocking edge. The decomposer must leave drafts that
# `enallagi tasks check calc` passes, and leave spec.md as it found it.
set -u
fail() {
  echo "  setup.sh: $1" >&2
  exit 1
}
mkdir -p .enallagi/specs/calc
cat >.enallagi/specs/calc/spec.md <<'SPEC'
# calc — specification

**Status:** ready-for-agent

## Problem Statement

There is no way to do arithmetic from this repository's shell scripts.

## Solution

A script `calc.sh` takes an operation and two integers and prints the result. `calc.sh add 2 3`
prints `5`, `calc.sh sub 5 3` prints `2`. With no arguments it prints a usage line naming every
operation it supports, so the usage line can only be written once both operations exist.

### Exit criteria

| Behaviour | Test |
|---|---|
| `calc.sh add 2 3` prints `5` | `tests/calc.test.sh::adds` |
| `calc.sh sub 5 3` prints `2` | `tests/calc.test.sh::subtracts` |
| `calc.sh` with no arguments prints a usage line naming `add` and `sub` | `tests/calc.test.sh::usage_lists_ops` |

## Out of Scope

- Any operation but `add` and `sub`.
SPEC
{ git -C .enallagi add -A && git -C .enallagi commit -qm 'eval: an accepted spec' >/dev/null; } ||
  fail "the spec commit failed"
# no drafts, and each of the three rows named by none
fresh=$(enallagi tasks check calc)
[ "$(printf '%s\n' "$fresh" | grep -c 'no drafts\|named by no draft')" = 4 ] ||
  fail "tasks check does not read the fresh spec as undecomposed: $fresh"
