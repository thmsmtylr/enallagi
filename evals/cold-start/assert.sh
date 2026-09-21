#!/usr/bin/env bash
# Zero findings against a healthy tree is the bar. A failure names every probe that fired and its
# count, so the total is the number the blocks behind this eval move.
set -u
cd fixture || exit 1

out=$(enallagi probe 2>&1)
rc=$?

# a probe that died and a probe that found nothing both print no FINDING line, so count nothing
# until the run is established: exit status, then a PROBE line, then no probe left unrun
[ "$rc" -eq 0 ] || {
  echo "  enallagi probe exited $rc, so nothing was counted:" >&2
  printf '%s\n' "$out" >&2
  exit 1
}
printf '%s\n' "$out" | grep -q '^PROBE ' || {
  echo "  enallagi probe printed no PROBE line, so no probe ran:" >&2
  printf '%s\n' "$out" >&2
  exit 1
}
errored=$(printf '%s\n' "$out" | awk '$1 == "PROBE" && $3 == "ERROR" { printf "%s ", $2 }')
[ -z "$errored" ] || {
  echo "  these probes could not run, so their findings were never counted: $errored" >&2
  printf '%s\n' "$out" >&2
  exit 1
}

printf '%s\n' "$out" | awk '$1 == "PROBE" && $3 ~ /^[0-9]+$/ && $3 > 0 { print "cold-start " $2 " " $3 }'
total=$(printf '%s\n' "$out" | grep -c '^FINDING ')
echo "cold-start total $total"
[ "$total" -eq 0 ]
