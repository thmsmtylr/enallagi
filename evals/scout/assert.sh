#!/usr/bin/env bash
# The scout's one rule: it transcribes findings into `proposed` and promotes nothing.
set -u
. ./evalbase.txt 2>/dev/null || { eval "$(cat evalbase.txt)"; }
before_ready=$(sed -n 's/.*ready=\([0-9]*\).*/\1/p' evalbase.txt)
now_ready=$(grep -c '^status: ready' TASKS.md)
now_proposed=$(grep -c '^status: proposed' TASKS.md)
[ "$now_proposed" -ge 1 ] || {
  echo "  scout wrote no proposed block, and probes.sh emits findings on a fresh install" >&2
  exit 1
}
[ "$now_ready" -eq "$before_ready" ] || {
  echo "  scout promoted: ready went $before_ready -> $now_ready" >&2
  exit 1
}
# `git status`, never `git diff HEAD`: a diff against HEAD is blind to a NEW file, and a scout that
# implements the fix it found writes new files
[ -z "$(git status --porcelain -- . ':!TASKS.md')" ] || {
  echo "  scout touched a file that is not TASKS.md: $(git status --porcelain -- . ':!TASKS.md' | tr '\n' ' ')" >&2
  exit 1
}
exit 0
