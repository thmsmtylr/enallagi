#!/usr/bin/env bash
# Sixty seconds: one task, ready -> done, in a repository this script creates and deletes.
#
#   docs/demo.sh    no arguments, no credential, no network
#
# `{ time ./docs/demo.sh; }` on the author's laptop, 2026-09-06: 13.507s and 13.485s on two
# consecutive runs, of which the loop itself reports 10s. That is where "about fifteen
# seconds" below comes from.
#
# The coding agent is replaced by `src/lane.sh`, a fixture that does what the role prompt it is
# handed asks for — the same shape driver.sh uses (driver.sh:33) and the reason this runs on a
# stranger's laptop. Everything else is the real package: the real install.sh, the real launcher,
# the real gates. What the reader is meant to notice is the last section: the launcher re-runs the
# check and the scope diff BEHIND the verifier's verdict, so `done` is a fact about the tree and
# not a claim an agent made about itself.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

D=$(mktemp -d) || exit 3
# the machine is left as it was found, on every exit path
trap 'rm -rf "$D"' EXIT INT TERM

step() { printf '\n== %s\n' "$1"; }

cd "$D" || exit 3
git init -q
git config user.email demo@local
git config user.name demo
git config commit.gpgsign false # a signing key on this machine is not the demo's business
mkdir -p src && echo 'export const x = 1' >src/schema.ts
git add -A && git commit -qm 'chore: init' >/dev/null

cat <<'INTRO'
harness demo — one task goes ready -> done in a repository this script creates and then deletes.
Nothing is installed on your machine and no credential is read. Takes about fifteen seconds.
INTRO

step "install into a throwaway repo  ($D)"
# install.sh's own summary line, not its 33-line file list: the reader learns nothing from the list
INSTALL=$("$PKG/install.sh" "$D" 2>&1) || {
  printf '%s\n' "$INSTALL" >&2
  echo "demo: install failed" >&2
  exit 3
}
printf '%s\n' "$INSTALL" | grep -E '^installed [0-9]+ files'

# The stand-in for the coding agent. It is handed the same role prompt a real agent would be, and
# it is deliberately dumb: it does the implementer's protocol, then the verifier's, and nothing
# else. No API key is read anywhere in this script.
cat >src/lane.sh <<'LANE'
#!/usr/bin/env bash
case "$1" in
  *"roles/implementer.md"*)
    echo 'export const y = 2' > src/allowed.ts
    sed -i.bak 's/^status: ready/status: review/' TASKS.md && rm -f TASKS.md.bak
    printf '\n## demo — T-001 — landed\nfriction: none\n' >> PROGRESS.md
    git add -A && git commit -qm 'feat: T-001 the work' >/dev/null
    ;;
  *"roles/verifier.md"*)
    sed -i.bak 's/^status: review/status: done/' TASKS.md && rm -f TASKS.md.bak
    ;;
esac
LANE
chmod +x src/lane.sh

python3 - <<'CFG'
import json
c = json.load(open('harness.json'))
c['agentCommand'] = ['./src/lane.sh', '{prompt}', '{turns}']
c['check'] = c['checkForce'] = 'true'   # a real repo puts its test command here
json.dump(c, open('harness.json', 'w'), indent=2)
CFG
"$PKG/install.sh" "$D" >/dev/null 2>&1 || exit 3

# The seeded T-001 ships with an empty scope: line. Give it one, so the scope gate below has
# something to judge the lane's diff against.
python3 - <<'TASK'
s = open('TASKS.md').read().replace('''## [T-001] <the first task>
scope:
blockedBy: none''', '''## [T-001] <the first task>
scope: src/allowed.ts
blockedBy: none''', 1)
open('TASKS.md', 'w').write(s)
TASK
git add -A && git commit -qm 'chore: T-001 setup' >/dev/null

step "the queue"
.harness/tasks.py list

step "one iteration: an implementer process, then a separate verifier process"
.harness/loop.sh 1 2>&1 | grep -v '^archive: '

step "what persisted, read back off the tree and not off anything an agent said"
printf 'T-001  status: %s\n' "$(.harness/tasks.py list | awk '/^T-001/{print $NF}')"
git log --oneline --format='%s' | sed 's/^/commit  /' | head -3
printf 'PROGRESS.md ends: %s\n' "$(grep '^## ' PROGRESS.md | tail -1)"
