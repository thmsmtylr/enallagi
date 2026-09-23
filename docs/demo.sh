#!/usr/bin/env bash
# Sixty seconds: one task, ready -> done, in a repository this script creates and deletes.
#
#   docs/demo.sh    no arguments, no credential, no network
#
# The coding agent is replaced by `src/lane.sh`, a fixture that does what the role prompt it is
# handed asks for — the same shape driver.sh uses, and the reason this runs on a stranger's laptop.
# Everything else is the real package: the real `enallagi init`, the real launcher, the real gates.
# What the reader is meant to notice is the last section: the launcher re-runs the check and the
# scope diff BEHIND the verifier's verdict, so `done` is a fact about the tree and not a claim an
# agent made about itself.
#
# ENALLAGI_BIN names the binary. It defaults to this checkout's release build, which is built here
# when it is absent.
set -u
PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENALLAGI_BIN="${ENALLAGI_BIN:-$PKG/target/release/enallagi}"
if [ ! -x "$ENALLAGI_BIN" ]; then
  (cd "$PKG" && cargo build --release -q) || {
    echo "demo: no $ENALLAGI_BIN and cargo build --release failed" >&2
    exit 3
  }
fi
export ENALLAGI_BIN

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
enallagi demo — one task goes ready -> done in a repository this script creates and then deletes.
Nothing is installed on your machine and no credential is read. Takes about fifteen seconds.
INTRO

# The stand-in for the coding agent. It is handed the same role prompt a real agent would be, and
# it is deliberately dumb: it does the implementer's protocol, then the verifier's, and nothing
# else. No API key is read anywhere in this script.
cat >src/lane.sh <<'LANE'
#!/usr/bin/env bash
case "$1" in
  *"roles/implementer.md"*)
    echo 'export const y = 2' > src/allowed.ts
    "$ENALLAGI_BIN" tasks set-status T-001 review 'the demo lane implemented it' >/dev/null
    printf '\n## demo — T-001 — landed\nfriction: none\n' >> .enallagi/PROGRESS.md
    git add -A && git commit -qm 'feat: T-001 the work' >/dev/null
    ;;
  *"roles/verifier.md"*)
    "$ENALLAGI_BIN" tasks set-status T-001 done 'the demo lane verified it' >/dev/null
    ;;
esac
echo '{"total_cost_usd": 0.01}'
LANE
chmod +x src/lane.sh

# The skills the role prompts name are declared with `path:` sources this script writes, so the real
# resolve, vendor and lock path runs with nothing to fetch. Dropping them instead leaves the prompts'
# `{{skill:id}}` tokens unmatched and the run refuses the stage.
for id in tdd ponytail debugging review-received verify-before-done review-requested brainstorming caveman-commit; do
  mkdir -p "vendor/$id"
  printf '# %s\n' "$id" >"vendor/$id/SKILL.md"
done

# a real repo puts its test command in check.command
cat >enallagi.toml <<'TOML'
[agent]
preset = "custom"
command = ["./src/lane.sh", "{prompt}", "{turns}"]

[agent.usage]
cost = "total_cost_usd"

[check]
command = "true"
TOML
for id in tdd ponytail debugging review-received verify-before-done review-requested brainstorming caveman-commit; do
  printf '\n[[skill]]\nid = "%s"\nsource = "path:vendor/%s"\npath = ""\ngate = "none"\nwhy = "the demo fixture"\n' \
    "$id" "$id" >>enallagi.toml
done

step "install into a throwaway repo  ($D)"
INSTALL=$("$ENALLAGI_BIN" init 2>&1) || {
  printf '%s\n' "$INSTALL" >&2
  echo "demo: install failed" >&2
  exit 3
}
# the count, not the 30-line file list: the reader learns nothing from the list
printf 'installed %s files\n' "$(printf '%s\n' "$INSTALL" | grep -c '^  wrote: ')"

# init seeds no task. The demo queues one with a scope: line, so the scope gate below has
# something to judge the lane's diff against.
cat >>.enallagi/TASKS.md <<'BLOCK'

## [T-001] the demo task
scope: src/allowed.ts
blockedBy: none
status: ready
rows: none — harness
criteria:
  - the demo lane writes src/allowed.ts
notes:
BLOCK
git add -A && git commit -qm 'chore: T-001 setup' >/dev/null

# `CI` forces --frozen, which refuses a stage whose skills are not already vendored and locked, so
# the demo vendors them first. The sources are local, so this reaches no network.
step "vendor the declared skills and pin them in harness.lock"
"$ENALLAGI_BIN" skills sync | sed 's/^/  /'
git add -A && git commit -qm 'chore: vendor the declared skills' >/dev/null

step "the queue"
"$ENALLAGI_BIN" tasks list

step "one iteration: an implementer process, then a separate verifier process"
"$ENALLAGI_BIN" run --iterations 1 --no-tui 2>&1 | grep -E 'stage\.start|kind=gate| gate |run\.end'

step "what persisted, read back off the tree and not off anything an agent said"
printf 'T-001  status: %s\n' "$("$ENALLAGI_BIN" tasks list | awk '/^T-001/{print $NF}')"
git log --oneline --format='%s' | sed 's/^/commit  /' | head -3
printf 'PROGRESS.md ends: %s\n' "$(grep '^## ' .enallagi/PROGRESS.md | tail -1)"
