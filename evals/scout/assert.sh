#!/usr/bin/env bash
# The scout's one rule: it transcribes findings into `needs-triage` tickets and promotes nothing.
set -u
before_ready=$(sed -n 's/.*ready=\([0-9]*\).*/\1/p' evalbase.txt)
# a ticket file the tracker cannot parse fails every read of the queue, so read it before counting
enallagi tasks list >/dev/null || {
  echo "  the queue no longer parses after the scout ran" >&2
  exit 1
}
now_ready=$(enallagi tasks ids-at ready-for-agent | grep -c .)
now_proposed=$(enallagi tasks ids-at needs-triage | grep -c .)
[ "$now_proposed" -ge 1 ] || {
  echo "  scout filed no needs-triage ticket, and probes.sh emits findings on a fresh install" >&2
  exit 1
}
[ "$now_ready" -eq "$before_ready" ] || {
  echo "  scout promoted: ready-for-agent went $before_ready -> $now_ready" >&2
  exit 1
}
# `git status`, never `git diff HEAD`: a diff against HEAD is blind to a NEW file, and a scout that
# implements the fix it found writes new files
[ -z "$(git status --porcelain)" ] || {
  echo "  scout touched a product file: $(git status --porcelain | tr '\n' ' ')" >&2
  exit 1
}
[ -z "$(git -C .enallagi status --porcelain -- . ':!specs')" ] || {
  echo "  scout touched an instance file outside specs/: $(git -C .enallagi status --porcelain -- . ':!specs' | tr '\n' ' ')" >&2
  exit 1
}
exit 0
