#!/usr/bin/env bash
# The portable floor gate: run the check, forgive only what `.check-baseline` already records.
#
# `green` is verified on DELTA, never on absolute zero. A tree whose exit criteria are not all
# covered yet cannot reach zero failures, and a gate whose passing state is unreachable is not
# strict, it is broken (LEARNINGS.md 2026-08-27). `.check-baseline` is the recorded red and it
# only ever shrinks: a failure listed there is inherited, a failure not listed there is a
# rejection, and adding a line to it is weakening a test by another name.
#
# Fails closed. A run whose failures it cannot NAME forgives nothing, because a gate asserts what
# it executed and never merely that nothing failed.
#
# Tokens like __CHECK__ are substituted by install.sh from harness.json.
set -u
ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
# `if`, never `A && B || C`: the `&&` arm here CAN fail — a $ROOT that was deleted or is
# unreadable makes `cd` non-zero — and this gate decides whether a red check is forgiven,
# so the branch may not be approximate (SC2015, T-008).
if [ -z "$ROOT" ] || ! cd "$ROOT"; then
  echo "check-gate: no project root" >&2
  exit 2
fi

OUT=$(__CHECK__ 2>&1)
RED=$?
[ "$RED" -eq 0 ] && exit 0

# One failure name per line, extracted with the project's own pattern.
FAILED=$(printf '%s\n' "$OUT" | sed -n '__FAIL_NAME_SED__' | sed 's/ \[[0-9.]*m*s\]$//' | sort -u)

if [ -z "$FAILED" ]; then
  echo "check RED, and no failure could be named — nothing to forgive:" >&2
  printf '%s\n' "$OUT" | tail -30 >&2
  exit 2
fi

BASELINE=$([ -f .check-baseline ] && sed 's/#.*//; s/[[:space:]]*$//' .check-baseline | grep -v '^$')
FORGIVEN=$(printf '%s\n' "$FAILED" | grep -xF -f <(printf '%s\n' "$BASELINE"))
UNFORGIVEN=$(printf '%s\n' "$FAILED" | grep -vxF -f <(printf '%s\n' "$BASELINE"))

[ -n "$FORGIVEN" ] && {
  echo "check-gate: forgiven by .check-baseline:" >&2
  printf '%s\n' "$FORGIVEN" | sed 's/^/  /' >&2
}
[ -z "$UNFORGIVEN" ] && exit 0

echo "check RED, not on the baseline:" >&2
printf '%s\n' "$UNFORGIVEN" | sed 's/^/  /' >&2
printf '%s\n' "$OUT" | tail -30 >&2
exit 2
