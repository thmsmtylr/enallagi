#!/usr/bin/env bash
# Every criterion passes, so the task is done; the leaked REGISTRY entry is a defect outside them, and
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
exit 0
