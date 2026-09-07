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
#   .harness/hooks/        probes.sh, check-gate.sh, verify-done.sh, immutable.sh, one-writer.sh
#   .harness/roles/        the five role prompts the launcher feeds to a fresh agent process
#   <skillsDir>/           this project's own Agent Skill, in the agentskills.io format
#   evals/                 the write-path gate for a new LEARNINGS.md rule
#   documents              TASKS.md, PROGRESS.md, LEARNINGS.md, DECISIONS.md, .check-baseline
#
# Adapters (optional): --adapter claude also writes .claude/agents/, .claude/settings.json and
# the UserPromptSubmit skill hook it registers;
# --adapter bun-turbo writes the five-stage check.ts floor. See adapters/*/README.md.
set -u

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET=""
DRY=""
ADAPTERS=()
while [ $# -gt 0 ]; do
  case "$1" in
  --dry-run) DRY=1 ;;
  --adapter)
    shift
    ADAPTERS+=("${1:-}")
    ;;
  -*)
    echo "install: unknown flag $1" >&2
    exit 2
    ;;
  *) TARGET="$1" ;;
  esac
  shift
done

[ -n "$TARGET" ] || {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}
TARGET="$(cd "$TARGET" 2>/dev/null && pwd)" || {
  echo "install: $TARGET is not a directory" >&2
  exit 2
}
git -C "$TARGET" rev-parse --git-dir >/dev/null 2>&1 ||
  {
    echo "install: $TARGET is not a git repository. The harness records its own history there; git init first." >&2
    exit 2
  }

say() { echo "  $*"; }
run() {
  [ -n "$DRY" ] && {
    say "would: $*"
    return 0
  }
  "$@"
}

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

SPEC_FILE=$(read_key spec)
HARNESS_DIR=$(read_key harnessDir)
SKILLS_DIR=$(read_key skillsDir)
CONTEXT_FILE=$(read_key contextFile)
POINTERS=$(read_key pointerFiles)

# Merging defaults under your answers means a new key in a later version does not break an old
# install; a missing key leaves a token standing, and the assert at the end catches it.
SUBST=$(
  python3 - <<'PY'
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
    if key in ('check', 'checkForce') and not str(value).strip():
        sys.exit('harness.json: %r is empty. The gate would run nothing and call it green.' % key)
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
        # __KEY_JSON__ is the same value as a JSON literal, which is also a Python literal: the
        # Python heredocs read that form, so an apostrophe in `check` is a character, not a
        # SyntaxError that takes all sixteen probes down with it
        out[token[:-2] + '_JSON__'] = json.dumps(value)
json.dump(out, sys.stdout)
PY
) || exit 2

echo "installing the harness into $TARGET${DRY:+  (dry run)}"
run mkdir -p "$TARGET/$HARNESS_DIR/hooks" "$TARGET/$HARNESS_DIR/lib" "$TARGET/$HARNESS_DIR/roles" "$TARGET/$SKILLS_DIR"

TRACK="" # every top-level path this run wrote or seeded, for the `git add` at the end
track() {
  local rel="${1#"$TARGET"/}"
  rel="${rel%%/*}"
  case " $TRACK " in *" $rel "*) ;; *) TRACK="$TRACK $rel" ;; esac
}
place() { # $1 = source file, $2 = destination
  track "$2"
  if [ -n "$DRY" ]; then
    say "would write: ${2#"$TARGET"/}"
    return 0
  fi
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
seed() {
  track "$2"
  [ -s "$2" ] && {
    say "kept: ${2#"$TARGET"/} (already has content)"
    return 0
  }
  place "$1" "$2"
}

for f in "$SRC"/harness/*.sh; do place "$f" "$TARGET/$HARNESS_DIR/$(basename "$f")"; done
for f in "$SRC"/harness/hooks/*.sh; do place "$f" "$TARGET/$HARNESS_DIR/hooks/$(basename "$f")"; done
for f in "$SRC"/harness/lib/*.sh; do place "$f" "$TARGET/$HARNESS_DIR/lib/$(basename "$f")"; done
place "$SRC/harness/tasks.py" "$TARGET/$HARNESS_DIR/tasks.py"
for f in "$SRC"/roles/*.md; do place "$f" "$TARGET/$HARNESS_DIR/roles/$(basename "$f")"; done
place "$SRC/templates/RAILS.md" "$TARGET/$HARNESS_DIR/RAILS.md"
run chmod +x "$TARGET/$HARNESS_DIR"/*.sh "$TARGET/$HARNESS_DIR/hooks"/*.sh "$TARGET/$HARNESS_DIR/tasks.py"
# The run log is machinery, not content: a lane that stages everything would otherwise commit it,
# and the scope gate would reject that lane for a file it did not write. Scoped to the harness
# directory, so the repository's own .gitignore is never touched.
# Every path in here is written BY the harness, never by a lane, and gate_verdict reads
# `git status --porcelain` to decide whether a lane left its work off the branch -- so an
# un-ignored file the harness wrote itself reads as an uncommitted implementation and forces a
# verified task back to ready. It did, on 2026-09-04, to T-004 on its fourth pass.
# `worktrees/` is APPENDED rather than seeded: the seed only fires when the file is absent, so an
# install that predates worktree.sh moving its checkout in here already has the file and would
# never get the cover. Both happen in this one run, so the ignore is in place before the new
# worktree.sh can create the directory.
[ -n "$DRY" ] || {
  [ -e "$TARGET/$HARNESS_DIR/.gitignore" ] || printf 'run.log\n*.log\nlogs/\n' >"$TARGET/$HARNESS_DIR/.gitignore"
  grep -qx 'worktrees/' "$TARGET/$HARNESS_DIR/.gitignore" || printf 'worktrees/\n' >>"$TARGET/$HARNESS_DIR/.gitignore"
  grep -qx 'loop.pid' "$TARGET/$HARNESS_DIR/.gitignore" || printf 'loop.pid\n' >>"$TARGET/$HARNESS_DIR/.gitignore"
  grep -qx '__pycache__/' "$TARGET/$HARNESS_DIR/.gitignore" || printf '__pycache__/\n' >>"$TARGET/$HARNESS_DIR/.gitignore"
}

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
  RAILS.md | SPEC.section.md | AGENTS.md | pointer.md) continue ;;
  dot.*) base=".${base#dot.}" ;;
  esac
  seed "$f" "$TARGET/$base"
done
seed "$SRC/templates/AGENTS.md" "$TARGET/$CONTEXT_FILE"
# The context file is seeded on the FIRST run, when harness.json is still the defaults, so it
# names the default check. On a re-run with a real `check`, the lines the template wrote from the
# default are rewritten -- only those: a backticked default is the template's text, never yours.
[ -n "$DRY" ] || [ ! -s "$TARGET/$CONTEXT_FILE" ] || python3 - "$TARGET/$CONTEXT_FILE" <<'RESYNC'
import json, os, sys
src, cfg = os.environ['SRC'], os.environ['CONFIG']
d = json.load(open(os.path.join(src, 'harness.default.json')))
c = {**d, **json.load(open(cfg))}
path = sys.argv[1]
text = before = open(path, encoding='utf-8').read()
for key in ('check', 'checkForce'):
    if c[key] != d[key]:
        text = text.replace('`%s`' % d[key], '`%s`' % c[key])
if text != before:
    open(path, 'w', encoding='utf-8').write(text)
    print('  updated: %s now names `%s`' % (os.path.basename(path), c['check']))
RESYNC
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
  [ -d "$SRC/adapters/$a" ] || {
    echo "install: no adapter named $a" >&2
    exit 2
  }
  echo "adapter: $a"
  case "$a" in
  claude)
    for f in "$SRC"/roles/*.md; do place "$f" "$TARGET/.claude/agents/$(basename "$f")"; done
    # The hook scripts go where the portable ones already live, so settings.json can point at
    # __HARNESS_DIR__/hooks/ for all of them and nothing tool-specific ends up outside .claude/.
    for f in "$SRC"/adapters/claude/*.sh; do place "$f" "$TARGET/$HARNESS_DIR/hooks/$(basename "$f")"; done
    run chmod +x "$TARGET/$HARNESS_DIR/hooks"/*.sh
    if [ -n "$DRY" ] || [ ! -s "$TARGET/.claude/settings.json" ]; then
      place "$SRC/adapters/claude/settings.json" "$TARGET/.claude/settings.json"
    else
      # every repo already on Claude Code has one, and `seed` kept it and wired no hook at all.
      # Hooks are added by command; a deny rule by text; anything else in the file is left alone.
      SUBST="$SUBST" python3 - "$SRC/adapters/claude/settings.json" "$TARGET/.claude/settings.json" <<'MERGE' || exit 2
import json, os, sys
subs = json.loads(os.environ['SUBST'])
ours = open(sys.argv[1], encoding='utf-8').read()
for token, value in subs.items():
    ours = ours.replace(token, value)
ours = json.loads(ours)
try:
    theirs = json.load(open(sys.argv[2], encoding='utf-8'))
except ValueError as why:
    sys.exit('install: %s is not valid JSON (%s); fix it or move it aside, nothing here will guess' % (sys.argv[2], why))
added = 0
for event, groups in ours.get('hooks', {}).items():
    have = theirs.setdefault('hooks', {}).setdefault(event, [])
    known = {h.get('command') for g in have for h in g.get('hooks', [])}
    for g in groups:
        new = [h for h in g['hooks'] if h.get('command') not in known]
        if new:
            have.append({**g, 'hooks': new})
            added += len(new)
deny = theirs.setdefault('permissions', {}).setdefault('deny', [])
for rule in ours.get('permissions', {}).get('deny', []):
    if rule not in deny:
        deny.append(rule)
json.dump(theirs, open(sys.argv[2], 'w', encoding='utf-8'), indent=2)
open(sys.argv[2], 'a').write('\n')
print('  merged: .claude/settings.json (%d hook(s) added, the rest kept)' % added)
MERGE
    fi
    ;;
  *)
    for f in "$SRC/adapters/$a"/*.sh; do [ -e "$f" ] && place "$f" "$TARGET/$HARNESS_DIR/hooks/$(basename "$f")"; done
    for f in "$SRC/adapters/$a"/*.ts; do [ -e "$f" ] && place "$f" "$TARGET/$(basename "$f")"; done
    run chmod +x "$TARGET/$HARNESS_DIR/hooks"/*.sh
    ;;
  esac
done

# --- assert what was executed, never merely that nothing failed -------------
if [ -z "$DRY" ]; then
  # shellcheck disable=SC2086 # $POINTERS is a space-separated list on purpose
  LEFT=$(cd "$TARGET" && { grep -rlE '__[A-Z][A-Z_]+__' "$HARNESS_DIR" "$SKILLS_DIR" "$CONTEXT_FILE" $POINTERS 2>/dev/null || true; })
  if [ -n "$LEFT" ]; then
    echo
    echo "install FAILED: a token survived substitution, so harness.json is missing a key:" >&2
    for f in $LEFT; do echo "  $f: $(grep -ohE '__[A-Z][A-Z_]+__' "$TARGET/$f" | sort -u | tr '\n' ' ')" >&2; done
    exit 2
  fi
  for f in "$TARGET/$HARNESS_DIR"/*.sh "$TARGET/$HARNESS_DIR/hooks"/*.sh "$TARGET/$HARNESS_DIR/lib"/*.sh; do
    bash -n "$f" || {
      echo "install FAILED: $f is not valid bash after substitution" >&2
      exit 2
    }
  done
  python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" "$TARGET/$HARNESS_DIR/tasks.py" ||
    {
      echo "install FAILED: tasks.py is not valid python after substitution" >&2
      exit 2
    }
  COUNT=$(find "$TARGET/$HARNESS_DIR" "$TARGET/$SKILLS_DIR" -type f | wc -l | tr -d ' ')
  echo
  echo "installed $COUNT files, every token substituted, every script parses."
fi

cat <<NEXT

Next, in $TARGET:
  1. Edit harness.json — 'check', 'spec' and 'agentCommand' are the three that matter.
  2. Re-run this script. Substitution is idempotent.
  3. Write your exit criteria into $SPEC_FILE under the heading harness.json names.
  4. git add harness.json$TRACK
     # everything this run wrote. gate_verdict counts an untracked path as work off the branch,
     # so a document left untracked here fails the first verdict.
  5. $HARNESS_DIR/hooks/probes.sh     # what the tree says about itself
  6. $HARNESS_DIR/loop.sh 1           # one iteration, attended, watch it work
NEXT
