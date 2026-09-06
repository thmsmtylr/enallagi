#!/usr/bin/env bash
# PreToolUse: refuse an Edit/Write from a session that is not the lane the loop is running.
# Same ceiling as immutable.sh: a `sed -i` or a heredoc from Bash never reaches a PreToolUse
# matcher, so this detects and forces the bypass to be deliberate; the authority is the verifier
# reading the diff. Every other gate in this package binds a task in flight -- gate_verdict,
# gate_scope, the role separation -- and the operator sits outside all of them.
set -u
ROOT="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -n "$ROOT" ] || exit 0
ARCHIVE="$ROOT/__HARNESS_DIR__/archive-done.sh"
[ -x "$ARCHIVE" ] || exit 0
# "Is a lane live in THIS checkout, and is it someone other than me?" is archive-done.sh:31's
# process-tree walk, called rather than copied. It excludes the caller's own chain, so the lane
# itself is never its own answer, and --dry-run writes nothing and has no other non-zero path.
"$ARCHIVE" --dry-run 2>&1 | grep -q 'an agent is running' || exit 0
# Read from the queue, because a lane's argv does not carry its task id: the block sits at review
# while the verifier runs and is still ready while the implementer does. Only on the refusal path,
# so the allow path costs one walk.
cd "$ROOT" || exit 0
TASK=$(python3 __HARNESS_DIR__/tasks.py ids-at review TASKS.md 2>/dev/null | head -1)
[ -n "$TASK" ] || TASK=$(python3 __HARNESS_DIR__/tasks.py ready-unattended TASKS.md 2>/dev/null)
echo "one-writer: a lane is live in this checkout and this session is not it.${TASK:+ It is on $TASK.} One checkout is one writer (LEARNINGS.md): two sessions editing the same file ship two versions of it. This edit is refused. \`touch STOP\` halts the loop before its next stage; when it is idle every write is allowed again." >&2
exit 2
