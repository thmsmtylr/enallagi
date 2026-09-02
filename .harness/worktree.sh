#!/usr/bin/env bash
# One lane, one worktree. `loop.sh` gets a checkout of its own, so the parent working tree and HEAD
# are untouched while it runs, and a lane that goes wrong is one `git worktree remove` away.
#
# This is what the harness does NOT give you by default: fresh sessions isolate CONTEXT — each stage
# is a new process with none of the last one's conversation — and every one of them still writes to
# the same checkout. That is why `one checkout is one writer` is a rail rather than a mechanism.
# Run this instead of `loop.sh` when you want the filesystem isolated too.
#
# Usage:  .harness/worktree.sh [iterations]
# Stop:   touch STOP  in the WORKTREE (the loop checks it there, not here)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1

PARENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
[ "$PARENT_BRANCH" = "HEAD" ] && { echo "worktree: the parent checkout is detached; check out a branch first" >&2; exit 1; }
LANE="lane/$(date +%Y%m%d-%H%M%S)-$$"
DIR="$(cd .. && pwd)/$(basename "$PWD")-$(basename "$LANE")"

git worktree add -b "$LANE" "$DIR" >/dev/null 2>&1 \
  || { echo "worktree: could not create $DIR" >&2; exit 1; }
echo "worktree: $LANE in $DIR, from $PARENT_BRANCH at $(git rev-parse --short HEAD)"

# The loop runs entirely in there. Its gates, its commits and its STOP file are all the lane's.
( cd "$DIR" && ./.harness/loop.sh "${1:-3}" )
LOOP_RC=$?

# Uncommitted work in the lane is the lane's to finish, not ours to throw away: a terminated
# iteration leaves exactly this, and `implementer.md` says finish it, never restart it.
if [ -n "$(git -C "$DIR" status --porcelain)" ]; then
  echo "worktree: $LANE has uncommitted work. Left in place — finish it there, never restart it:"
  git -C "$DIR" status --short | sed 's/^/  /'
  exit 1
fi

# Fast-forward or nothing. A merge commit here would mean the parent moved under the lane, which is
# the second-writer case the rails are about, and it is a human's call — never this script's.
if git merge --ff-only "$LANE" >/dev/null 2>&1; then
  echo "worktree: $PARENT_BRANCH fast-forwarded to $LANE at $(git rev-parse --short HEAD)"
  git worktree remove "$DIR" && git branch -d "$LANE" >/dev/null 2>&1
  echo "worktree: removed $DIR"
  exit "$LOOP_RC"
fi

cat >&2 <<WHAT
worktree: $PARENT_BRANCH has moved and $LANE cannot fast-forward into it. Nothing was merged and
nothing was removed. A merge here is a decision, not a step:
  cd $DIR                       # the lane's work, intact
  git -C $PWD merge $LANE       # if you want the merge commit
  git -C $PWD worktree remove $DIR && git -C $PWD branch -D $LANE   # if you do not want the work
WHAT
exit 1
