#!/usr/bin/env bash
# The bootstrap record: one line per dogfood round, read out of `git log` and nothing else.
# A round runs from a `chore(dogfood):` commit to the commit that strips the instance again.
# Nothing here is hand-written, so a round nobody ran cannot appear and a round that ran cannot
# be left out.
#
#   docs/bootstrap.sh          print the record
#   docs/bootstrap.sh --check  exit non-zero when the history stops supporting the claim
#
# Counts are of COMMITS, not of tasks: one `verify:` commit can carry several rows, and a task
# can take more than one commit. The line says what was counted.
set -u

CHECK=""
case "${1:-}" in
"") ;;
--check) CHECK=1 ;;
*)
  echo "usage: bootstrap.sh [--check]" >&2
  exit 2
  ;;
esac

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "bootstrap: not a git repository" >&2
  exit 2
fi

git log --reverse --date=short --format='%h%x09%ad%x09%s' | awk -F'\t' -v check="$CHECK" '
function emit(esha, edate) {
  printf "round %d  %s..%s  %s..%s  %d commits (%d naming a task, %d verify, %d rejected)%s\n",
    n, start, esha, sdate, edate, c, t, v, r, note
}
{
  s = $3; last_sha = $1; last_date = $2
  if (!open && s ~ /^chore\(dogfood\)/) {
    n++; open = 1; start = $1; sdate = $2; c = 0; t = 0; v = 0; r = 0
  }
  if (s ~ /REJECTED/) rt++
  if (open) {
    c++
    if (s ~ /^verify:/) v++; else if (s ~ /T-[0-9]+/) t++
    if (s ~ /REJECTED/) r++
    # the strip commit is the last one of the round: it removes the installed instance
    if (s ~ /strip.*dogfood instance/) { emit($1, $2); open = 0 }
  }
}
END {
  if (open) { note = " (in flight)"; emit(last_sha, last_date) }
  if (check == "") exit 0
  if (n == 0) {
    print "bootstrap: no dogfood round in this history — the claim has nothing behind it" > "/dev/stderr"
    exit 1
  }
  # a verifier that never refused is a verifier that is not running, so a history with no
  # rejection is a failed claim rather than a clean one
  if (rt == 0) {
    print "bootstrap: no REJECTED commit anywhere — nothing shows the verifier ever refused" > "/dev/stderr"
    exit 1
  }
  printf "%d rounds; REJECTED commits: %d — the record supports the claim\n", n, rt
}
'
