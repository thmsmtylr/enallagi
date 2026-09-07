#!/usr/bin/env bash
# A done task's `notes:` are its audit trail (`citable`) AND the only channel between two
# fresh sessions — so they are kept, not deleted. But 85% of TASKS.md was four finished
# tasks, and every process an iteration spawns re-reads the whole file to find its own
# block. The verdict moves to DECISIONS.md; the block keeps exactly what the two
# resolvers read: the header, `blockedBy:` and `status:`.
#
# It also rolls PROGRESS.md over. The loop reads only `tail -200` of that file, so past a few
# thousand lines the rest is an archive wearing a state file's name -- same move,
# same reason: keep the file a fresh process re-reads small, keep the record whole.
#
# Usage:  __HARNESS_DIR__/archive-done.sh [--dry-run]
# Called at the top of each loop.sh iteration, between agents, never during one.
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
DRY=""
[ "${1:-}" = "--dry-run" ] && DRY=1

# TASKS.md is the bus. Rewriting it under a live agent is LEARNINGS 2026-08-25, one checkout is one
# writer. Liveness is `loop.pid`: loop.sh writes its pid on start and removes it on exit, and a
# process is that loop's own lane exactly when the pid is among its ancestors. The guard used to
# scan `ps` for a loop.sh anywhere on the machine, which answered for every checkout at once and
# read a different repository's loop as this one's. one-writer.sh calls this with --dry-run rather
# than carrying a copy of the question.
PIDFILE="__HARNESS_DIR__/loop.pid"
loop_live() { # a loop is running here, and this process is not under it
  local pid p
  [ -s "$PIDFILE" ] || return 1
  pid=$(tr -d ' \n' <"$PIDFILE")
  kill -0 "$pid" 2>/dev/null || return 1
  p=$$
  while [ "${p:-0}" -gt 1 ]; do
    [ "$p" = "$pid" ] && return 1
    p=$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' ')
  done
  return 0
}
if loop_live; then
  echo "archive: an agent is running — TASKS.md is its bus, not touching it"
  exit 1
fi

# An uncommitted verdict folded into an archive commit loses its author and its message.
if [ -z "$DRY" ] && ! git diff --quiet -- TASKS.md; then
  echo "archive: TASKS.md has uncommitted changes — commit the verdict first"
  exit 1
fi

# no bytecode: `import tasks` wrote .harness/__pycache__/ and the next lane's `git add -A` shipped
# it, which the scope gate rejected -- on Linux only, a global gitignore hid it on the author's mac
PYTHONDONTWRITEBYTECODE=1 python3 - "$DRY" <<'PY'
import subprocess, sys
sys.path.insert(0, "__HARNESS_DIR__")
import tasks  # the one parser: a splitter of its own read a fenced heading as a block and a
              # column-0 `# ` line in notes as the end of one, and neither matched what the launcher saw

dry = bool(sys.argv[1])
sha = subprocess.check_output(["git", "rev-parse", "--short", "HEAD"]).decode().strip()
src = open("TASKS.md").read()
lines = src.split("\n")
blocks = tasks.parse(src)  # refuses an ambiguous queue, which is the right answer here too

archived, out, cursor = [], [], 0
for b in blocks:
    start = b["line"] - 1
    stop = start + len(b["body"]) + 1
    out.extend(lines[cursor:start])  # whatever sits between blocks: the header, a divider
    body, cursor = lines[start:stop], stop
    if tasks.field(b, "status") != "done" or tasks.field(b, "archived") is not None:
        out.extend(body)
        continue
    # Every field the two resolvers read is kept, so their view of the queue is
    # byte-identical before and after. Only the verdict prose moves.
    # the FIRST of each, which is the one field() reads; a quoted block in the notes has its own
    keep, seen = [], set()
    for l in body[1:]:
        key = l.split(":", 1)[0]
        if key in ("blockedBy", "scope", "attended") and key not in seen:
            keep.append(l)
            seen.add(key)
    out.extend([body[0]] + keep + ["status: done",
                f"archived: DECISIONS.md — full block at `git show {sha}:TASKS.md`", ""])
    archived.append((b["id"], "\n".join(body).rstrip() + "\n"))
out.extend(lines[cursor:])

if not archived:
    print("archive: nothing to archive")
    raise SystemExit(0)

print("archive: " + " ".join(t for t, _ in archived) + " -> DECISIONS.md")
if dry:
    raise SystemExit(0)

try:
    dec = open("DECISIONS.md").read()
except FileNotFoundError:
    dec = ("# DECISIONS\n\nCompleted task blocks, verbatim, moved out of TASKS.md once `done`.\n"
           "The queue stays small; the audit trail stays whole. Each block is the implementer's and\n"
           "the verifier's own words, never summarised on the way in.\n")
open("DECISIONS.md", "w").write(dec.rstrip() + "\n\n" + "\n\n".join(b for _, b in archived))
open("TASKS.md", "w").write("\n".join(out))
PY
rc=$?
[ "$rc" -ne 0 ] && exit "$rc"

# The same move for the loop's own record. PROGRESS_MAX is when it triggers, PROGRESS_KEEP is
# what stays -- and the split lands on an entry heading, never inside an entry.
PROGRESS_MAX="${PROGRESS_MAX:-2000}" PROGRESS_KEEP="${PROGRESS_KEEP:-200}" DRY="$DRY" python3 - <<'ROLLOVER'
import os

MAX, KEEP, dry = int(os.environ['PROGRESS_MAX']), int(os.environ['PROGRESS_KEEP']), bool(os.environ['DRY'])
try:
    lines = open('PROGRESS.md').read().split('\n')
except FileNotFoundError:
    raise SystemExit(0)
if len(lines) <= MAX:
    raise SystemExit(0)

# The header is everything up to the first horizontal rule: the entry format lives there and every
# iteration has to read it, so it never moves.
rule = next((i for i, l in enumerate(lines) if l.strip() == '---'), 0) + 1
head, body = lines[:rule], lines[rule:]
start = max(0, len(body) - KEEP)
split = next((i for i, l in enumerate(body) if l.startswith('## ') and i >= start), start)
moved, kept = body[:split], body[split:]
if not moved:
    raise SystemExit(0)

print('archive: PROGRESS.md is %d lines -> %d moved to PROGRESS.archive.md' % (len(lines), len(moved)))
if dry:
    raise SystemExit(0)

note = '<!-- Entries before this point are in PROGRESS.archive.md. Nothing reads it; it is the record. -->'
archive = open('PROGRESS.archive.md').read().rstrip() + '\n\n' if os.path.exists('PROGRESS.archive.md') else (
    '# PROGRESS (archive)\n\nEntries rolled out of PROGRESS.md by `archive-done.sh`, oldest first.\n'
    'The loop does not read this file. It exists so the record stays whole.\n\n')
open('PROGRESS.archive.md', 'w').write(archive + '\n'.join(moved).strip() + '\n')
open('PROGRESS.md', 'w').write('\n'.join(head + ['', note, ''] + kept))
ROLLOVER

[ -n "$DRY" ] && exit 0

if ! git diff --quiet -- TASKS.md PROGRESS.md; then
  git add TASKS.md DECISIONS.md PROGRESS.md 2>/dev/null
  [ -f PROGRESS.archive.md ] && git add PROGRESS.archive.md
  git commit -q -m "chore(archive): finished blocks to DECISIONS.md, old entries to PROGRESS.archive.md"
fi
