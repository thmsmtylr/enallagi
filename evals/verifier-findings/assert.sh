#!/usr/bin/env bash
# Every T-001 criterion passes, so it is done; the leaked REGISTRY entry is a defect outside them, and
# it belongs in the queue as one needs-triage ticket with the command that shows it, not in a verdict.
set -u
status=$(enallagi tasks field T-001 status)
[ "$status" = "done" ] || {
  echo "  expected T-001 done, every criterion passes; got status: $status" >&2
  exit 1
}
filed=$(enallagi tasks ids-at needs-triage)
[ "$(printf '%s' "$filed" | grep -c .)" = "1" ] || {
  echo "  expected exactly one needs-triage ticket, got: $filed" >&2
  exit 1
}
block=$(enallagi tasks block "$filed")
printf '%s\n' "$block" | grep -q '^probe: verifier' || {
  echo "  the filed ticket does not say probe: verifier:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
printf '%s\n' "$block" | grep -Eq '^command: *[^ ]' || {
  echo "  the filed ticket carries no command: that shows the defect:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
printf '%s\n' "$block" | grep -Eiq 'REGISTRY|restor|remov|shared|leak' || {
  echo "  the filed ticket does not name the REGISTRY entry the test never removes:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
# T-002's test passes with normalize's trim deleted, so it is a rejection naming that test
status=$(enallagi tasks field T-002 status)
[ "$status" = "ready-for-agent" ] || {
  echo "  expected T-002 rejected to ready-for-agent, its test passes with the trim deleted; got status: $status" >&2
  exit 1
}
notes=$(sed -n '/^## Comments/,/^## Harness/p' .enallagi/specs/*/issues/T-002-*.md)
printf '%s\n' "$notes" | grep -q 'REJECTED' || {
  echo "  T-002's comments carry no REJECTED verdict:" >&2; printf '%s\n' "$notes" >&2
  exit 1
}
printf '%s\n' "$notes" | grep -qi 'trims surrounding whitespace' || {
  echo "  T-002's rejection does not name the test that passes with its logic deleted:" >&2; printf '%s\n' "$notes" >&2
  exit 1
}
exit 0
