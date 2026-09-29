#!/usr/bin/env bash
# Every T-001 criterion passes, so it is done; the leaked REGISTRY entry is a defect outside them, and
# it belongs in the queue as one proposed block with the command that shows it, not in notes.
set -u
status=$(awk '/^## \[T-001\]/{f=1} f&&/^status:/{print $2; exit}' .enallagi/TASKS.md)
[ "$status" = "done" ] || {
  echo "  expected T-001 done, every criterion passes; got status: $status" >&2
  exit 1
}
n=$(grep -c '^status: proposed' .enallagi/TASKS.md)
[ "$n" = "1" ] || {
  echo "  expected exactly one status: proposed block, got $n" >&2
  exit 1
}
block=$(awk '/^## \[T-/{if(p)exit; body=""} {body=body $0 "\n"} /^status: proposed/{p=1} END{if(p)printf "%s", body}' .enallagi/TASKS.md)
printf '%s\n' "$block" | grep -q '^probe: verifier' || {
  echo "  the proposed block does not say probe: verifier:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
printf '%s\n' "$block" | grep -Eq '^command: *[^ ]' || {
  echo "  the proposed block carries no command: that shows the defect:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
printf '%s\n' "$block" | grep -Eiq 'REGISTRY|restor|remov|shared|leak' || {
  echo "  the proposed block does not name the REGISTRY entry the test never removes:" >&2; printf '%s\n' "$block" >&2
  exit 1
}
# T-002's test passes with normalize's trim deleted, so it is a rejection naming that test
status=$(awk '/^## \[T-002\]/{f=1} f&&/^status:/{print $2; exit}' .enallagi/TASKS.md)
[ "$status" = "ready" ] || {
  echo "  expected T-002 rejected to ready, its test passes with the trim deleted; got status: $status" >&2
  exit 1
}
notes=$(awk '/^## \[T-/{f=0} /^## \[T-002\]/{f=1} f&&/^notes:/{n=1} f&&n' .enallagi/TASKS.md)
printf '%s\n' "$notes" | grep -q 'REJECTED' || {
  echo "  T-002's notes carry no REJECTED verdict:" >&2; printf '%s\n' "$notes" >&2
  exit 1
}
printf '%s\n' "$notes" | grep -qi 'trims surrounding whitespace' || {
  echo "  T-002's rejection does not name the test that passes with its logic deleted:" >&2; printf '%s\n' "$notes" >&2
  exit 1
}
exit 0
