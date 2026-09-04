#!/usr/bin/env bash
# Install the harness into a repo. Idempotent: re-run to upgrade, or after editing harness.json.
#
#   ./install.sh /path/to/repo [--dry-run] [--adapter <name>]
#
# Layout it writes, and none of it is tool-specific:
#   AGENTS.md              the core context file, read by 20+ agents
#   CLAUDE.md, GEMINI.md,  one-line pointers at it, for tools that read their own file
#   .github/copilot-instructions.md
#   .harness/              loop.sh, worktree.sh, archive-done.sh, watch.sh, driver.example.sh, RAILS.md
#   .harness/lib/          the launcher's modules: queue.sh, agent.sh, gates.sh
#   .harness/tasks.py      the one parser for TASKS.md
#   .harness/hooks/        probes.sh, check-gate.sh, verify-done.sh, immutable.sh
#   .harness/roles/        the five role prompts the launcher feeds to a fresh agent process
#   <skillsDir>/           this project's own Agent Skill, in the agentskills.io format
#   evals/                 the write-path gate for a new LEARNINGS.md rule
#   documents              TASKS.md, PROGRESS.md, LEARNINGS.md, DECISIONS.md, .check-baseline
#
# Adapters (optional): --adapter claude also writes .claude/agents/ and .claude/settings.json;
# --adapter bun-turbo writes the five-stage check.ts floor. See adapters/*/README.md.
set -u

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET=""; DRY=""; ADAPTERS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY=1 ;;
    --adapter) shift; ADAPTERS+=("${1:-}") ;;
    -*) echo "install: unknown flag $1" >&2; exit 2 ;;
    *)  TARGET="$1" ;;
  esac
  shift
done

[ -n "$TARGET" ] || { sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
TARGET="$(cd "$TARGET" 2>/dev/null && pwd)" || { echo "install: $TARGET is not a directory" >&2; exit 2; }
git -C "$TARGET" rev-parse --git-dir >/dev/null 2>&1 \
  || { echo "install: $TARGET is not a git repository. The harness records its own history there; git init first." >&2; exit 2; }

say() { echo "  $*"; }
run() { [ -n "$DRY" ] && { say "would: $*"; return 0; }; "$@"; }

CONFIG="$TARGET/harness.json"
if [ ! -f "$CONFIG" ]; then
  echo "no harness.json in $TARGET — seeding defaults. Edit it, then re-run this script."
  run cp "$SRC/harness.default.json" "$CONFIG"
  [ -n "$DRY" ] && CONFIG="$SRC/harness.default.json"
fi
export SRC TARGET CONFIG

read_key() { python3 -c "
import json,os,sys
try: v=json.load(open(os.environ['CONFIG'])).get('$1')
except Exception: v=None
if v is None:
    v=json.load(open(os.path.join(os.environ['SRC'],'harness.default.json'))).get('$1')
print(v if not isinstance(v,list) else ' '.join(v))"; }

SPEC_FILE=$(read_key spec);        HARNESS_DIR=$(read_key harnessDir)
SKILLS_DIR=$(read_key skillsDir);  CONTEXT_FILE=$(read_key contextFile)
POINTERS=$(read_key pointerFiles)

# Merging defaults under your answers means a new key in a later version does not break an old
# install; a missing key leaves a token standing, and the assert at the end catches it.
SUBST=$(python3 - <<'PY'
import json, os, re, shlex, sys
AGENT_ROLES = ['default', 'scout', 'adjudicator', 'implementer', 'verifier']
defaults = json.load(open(os.path.join(os.environ['SRC'], 'harness.default.json')))
answers = json.load(open(os.environ['CONFIG']))
merged = {**defaults, **answers}
SHELL_SPLICED = {'check', 'checkForce', 'failNameSed', 'spec', 'harnessDir', 'skillsDir',
                 'contextFile', 'rateLimitPattern', 'driverCommand'}
out = {}
for key, value in merged.items():
    if key.startswith('_'):
        continue
    if key in SHELL_SPLICED and ('"' in str(value) or '\n' in str(value)):
        sys.exit('harness.json: %r is spliced into a shell script, so it cannot contain a double '
                 'quote or a newline. Got: %r' % (key, value))
    token = '__' + re.sub(r'(?<!^)(?=[A-Z])', '_', key).upper() + '__'
    if key == 'agentCommand':
        # Either a word list for every role, or an object keyed by role with a 'default'.
        # Renders one __AGENT_COMMAND_<ROLE>__ token per role; loop.sh picks by stage.
        roles = value if isinstance(value, dict) else {'default': value}
        if 'default' not in roles:
            sys.exit('harness.json: agentCommand as an object needs a "default" key. Got: %r' % sorted(roles))
        unknown = set(roles) - set(AGENT_ROLES)
        if unknown:
            sys.exit('harness.json: agentCommand names roles that do not exist: %s. Known: %s'
                     % (sorted(unknown), AGENT_ROLES))
        for role in AGENT_ROLES:
            # a token per role whether or not harness.json names it, so loop.sh can reference all
            # of them and install.sh's leftover-token check still means something
            words = roles.get(role, roles['default'])
            # a shell word list, quoted once here so a prompt with spaces cannot split
            out['__AGENT_COMMAND_%s__' % role.upper()] = ' '.join(shlex.quote(str(w)) for w in words)
        out[token] = out['__AGENT_COMMAND_DEFAULT__']
        # the binaries, for the pgrep guards: one that recognises only one vendor's process
        # never fires for anyone else
        binaries = sorted(set(os.path.basename(str(w[0])) for w in roles.values() if w))
        out['__AGENT_BINARY__'] = '|'.join(binaries) if binaries else 'agent'
    else:
        out[token] = json.dumps(value) if isinstance(value, (list, dict)) else str(value)
json.dump(out, sys.stdout)
PY
) || exit 2

echo "installing the harness into $TARGET${DRY:+  (dry run)}"
run mkdir -p "$TARGET/$HARNESS_DIR/hooks" "$TARGET/$HARNESS_DIR/lib" "$TARGET/$HARNESS_DIR/roles" "$TARGET/$SKILLS_DIR"

place() { # $1 = source file, $2 = destination
  if [ -n "$DRY" ]; then say "would write: ${2#"$TARGET"/}"; return 0; fi
  mkdir -p "$(dirname "$2")"
  SUBST="$SUBST" python3 - "$1" "$2" <<'PY'
import json, os, sys
subs = json.loads(os.environ['SUBST'])
text = open(sys.argv[1], encoding='utf-8').read()
for token, value in subs.items():
    text = text.replace(token, value)
open(sys.argv[2], 'w', encoding='utf-8').write(text)
PY
  say "wrote: ${2#"$TARGET"/}"
}
seed() { [ -s "$2" ] && { say "kept: ${2#"$TARGET"/} (already has content)"; return 0; }; place "$1" "$2"; }

for f in "$SRC"/harness/*.sh;       do place "$f" "$TARGET/$HARNESS_DIR/$(basename "$f")"; done
for f in "$SRC"/harness/hooks/*.sh; do place "$f" "$TARGET/$HARNESS_DIR/hooks/$(basename "$f")"; done
for f in "$SRC"/harness/lib/*.sh;   do place "$f" "$TARGET/$HARNESS_DIR/lib/$(basename "$f")"; done
place "$SRC/harness/tasks.py" "$TARGET/$HARNESS_DIR/tasks.py"
for f in "$SRC"/roles/*.md;         do place "$f" "$TARGET/$HARNESS_DIR/roles/$(basename "$f")"; done
place "$SRC/templates/RAILS.md" "$TARGET/$HARNESS_DIR/RAILS.md"
run chmod +x "$TARGET/$HARNESS_DIR"/*.sh "$TARGET/$HARNESS_DIR/hooks"/*.sh "$TARGET/$HARNESS_DIR/tasks.py"
# The run log is machinery, not content: a lane that stages everything would otherwise commit it,
# and the scope gate would reject that lane for a file it did not write. Scoped to the harness
# directory, so the repository's own .gitignore is never touched.
# Every path in here is written BY the harness, never by a lane, and gate_verdict reads
# `git status --porcelain` to decide whether a lane left its work off the branch -- so an
# un-ignored file the harness wrote itself reads as an uncommitted implementation and forces a
# verified task back to ready. It did, on 2026-09-04, to T-004 on its fourth pass.
[ -n "$DRY" ] || [ -e "$TARGET/$HARNESS_DIR/.gitignore" ] || printf 'run.log\n*.log\nlogs/\n' > "$TARGET/$HARNESS_DIR/.gitignore"

# Skills install per tool, not per repository (superpowers: "Installation differs by harness").
# What ships here is this project's OWN skill, in the format ~48 clients read.
while IFS= read -r -d '' f; do
  place "$f" "$TARGET/$SKILLS_DIR/${f#"$SRC"/skills/}"
done < <(find "$SRC/skills" -type f -print0)

# The write-path gate lives where the rules are written. The runner is replaced on upgrade; the
# evals themselves are the repository's own and are never overwritten.
place "$SRC/evals/run.sh" "$TARGET/evals/run.sh"
run chmod +x "$TARGET/evals/run.sh"
seed "$SRC/evals/README.md" "$TARGET/evals/README.md"

# --- documents, seeded once -------------------------------------------------
# A document with content in it is the project's own record and is never overwritten.
for f in "$SRC"/templates/*; do
  base="$(basename "$f")"
  case "$base" in
    RAILS.md|SPEC.section.md|AGENTS.md|pointer.md) continue ;;
    dot.*) base=".${base#dot.}" ;;
  esac
  seed "$f" "$TARGET/$base"
done
seed "$SRC/templates/AGENTS.md" "$TARGET/$CONTEXT_FILE"
if [ -s "$TARGET/$SPEC_FILE" ]; then
  say "kept: $SPEC_FILE — merge $SRC/templates/SPEC.section.md into it by hand"
else
  seed "$SRC/templates/SPEC.section.md" "$TARGET/$SPEC_FILE"
fi

# One-line pointers, for tools that read their own file rather than the open format. Claude Code
# does not read AGENTS.md natively (anthropics/claude-code#34235); the @ import is its documented
# workaround, and CLAUDE.md -> AGENTS.md is the most common pair observed in the wild, 311 times
# across 2,926 repositories (arXiv:2602.14690).
for p in $POINTERS; do
  [ "$p" = "$CONTEXT_FILE" ] && continue
  if [ -s "$TARGET/$p" ]; then
    if grep -q "$CONTEXT_FILE" "$TARGET/$p" 2>/dev/null; then
      say "kept: $p (already points at $CONTEXT_FILE)"
    else
      say "kept: $p — add a line pointing at $CONTEXT_FILE, or delete it"
    fi
  else
    place "$SRC/templates/pointer.md" "$TARGET/$p"
  fi
done

for a in ${ADAPTERS+"${ADAPTERS[@]}"}; do
  [ -d "$SRC/adapters/$a" ] || { echo "install: no adapter named $a" >&2; exit 2; }
  echo "adapter: $a"
  case "$a" in
    claude)
      for f in "$SRC"/roles/*.md; do place "$f" "$TARGET/.claude/agents/$(basename "$f")"; done
      seed "$SRC/adapters/claude/settings.json" "$TARGET/.claude/settings.json" ;;
    *)
      for f in "$SRC/adapters/$a"/*.sh; do [ -e "$f" ] && place "$f" "$TARGET/$HARNESS_DIR/hooks/$(basename "$f")"; done
      for f in "$SRC/adapters/$a"/*.ts; do [ -e "$f" ] && place "$f" "$TARGET/$(basename "$f")"; done
      run chmod +x "$TARGET/$HARNESS_DIR/hooks"/*.sh ;;
  esac
done

# --- assert what was executed, never merely that nothing failed -------------
if [ -z "$DRY" ]; then
  LEFT=$(grep -rlE '__[A-Z][A-Z_]+__' "$TARGET/$HARNESS_DIR" "$TARGET/$SKILLS_DIR" "$TARGET/$CONTEXT_FILE" 2>/dev/null || true)
  if [ -n "$LEFT" ]; then
    echo; echo "install FAILED: a token survived substitution, so harness.json is missing a key:" >&2
    for f in $LEFT; do echo "  ${f#"$TARGET"/}: $(grep -ohE '__[A-Z][A-Z_]+__' "$f" | sort -u | tr '\n' ' ')" >&2; done
    exit 2
  fi
  for f in "$TARGET/$HARNESS_DIR"/*.sh "$TARGET/$HARNESS_DIR/hooks"/*.sh "$TARGET/$HARNESS_DIR/lib"/*.sh; do
    bash -n "$f" || { echo "install FAILED: $f is not valid bash after substitution" >&2; exit 2; }
  done
  python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" "$TARGET/$HARNESS_DIR/tasks.py" \
    || { echo "install FAILED: tasks.py is not valid python after substitution" >&2; exit 2; }
  COUNT=$(find "$TARGET/$HARNESS_DIR" "$TARGET/$SKILLS_DIR" -type f | wc -l | tr -d ' ')
  echo; echo "installed $COUNT files, every token substituted, every script parses."
fi

cat <<NEXT

Next, in $TARGET:
  1. Edit harness.json — 'check', 'spec' and 'agentCommand' are the three that matter.
  2. Re-run this script. Substitution is idempotent.
  3. Write your exit criteria into $SPEC_FILE under the heading harness.json names.
  4. git add $HARNESS_DIR $SKILLS_DIR harness.json $CONTEXT_FILE   # the harness is tracked, on purpose
  5. $HARNESS_DIR/hooks/probes.sh     # what the tree says about itself
  6. $HARNESS_DIR/loop.sh 1           # one iteration, attended, watch it work
NEXT
