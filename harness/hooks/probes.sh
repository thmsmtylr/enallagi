#!/usr/bin/env bash
# Thirteen probes over the tree. Reports; never gates, and is wired into no hook and no build task.
# Tokens like __CHECK__ are substituted by install.sh from harness.json. Edit harness.json, re-install.
# Exit 0 when every probe ran, non-zero only when one could not.
set -u
ROOT="${1:-${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}}"
# `cd ""` returns 0, so an empty root has to be caught before the cd, not by it
[ -n "$ROOT" ] && cd "$ROOT" || { echo "probes: no project root" >&2; exit 2; }

# the force variant, because a cached green is a green nobody ran (LEARNINGS.md, 2026-08-27)
if [ -n "${TURBO_HASH:-}" ]; then
  CHECK_LOG="nested under turbo, the check would recurse"; CHECK_STATUS=nested
else
  CHECK_LOG=$(__CHECK_FORCE__ 2>&1); CHECK_STATUS=$?
fi

# The one probe that does not read text: it drives the built artifact through the surface a user
# touches and prints what fell short. The other eleven read the repo, so the floor and the direction
# signal are the same instrument and capability shortfall is invisible to them.
# Off unless harness.json names a driverCommand AND HARNESS_DRIVER is set, because it costs
# wall-clock on every scout round and has to earn it.
#
# Contract: the command exits 0 when it REACHED the artifact, whatever it found there, and prints
# one line per shortfall beginning `FINDING `. A non-zero exit means it could not reach the artifact
# at all -- that is `PROBE driver ERROR`, and nothing is proposed from a probe that did not run.
# It gets a throwaway working directory and a stripped environment: if the thing you drive is itself
# an agent, that is what stops it inheriting this loop's context, settings and tools. Watch the
# persistent effect, not the answer -- diff the store, the file, the row it was supposed to change.
# set before the assignment on purpose: the driver runs from a throwaway directory, so a relative
# path in driverCommand cannot work, and `$HARNESS_ROOT/scripts/drive.sh` resolves right here.
HARNESS_ROOT="$ROOT"
DRIVER="__DRIVER_COMMAND__"
DRIVER_STATUS=off
DRIVER_LOG=""
if [ -n "$DRIVER" ] && [ -n "${HARNESS_DRIVER:-}" ]; then
  DRIVER_DIR=$(mktemp -d)
  DRIVER_LOG=$(cd "$DRIVER_DIR" && env -i PATH="$PATH" HOME="$HOME" HARNESS_ROOT="$ROOT" sh -c "$DRIVER" 2>&1)
  DRIVER_STATUS=$?
  rm -rf "$DRIVER_DIR"
fi
export DRIVER DRIVER_STATUS DRIVER_LOG

CHECK_LOG="$CHECK_LOG" CHECK_STATUS="$CHECK_STATUS" python3 - <<'PY'
import glob, json, os, re, subprocess, sys

SRC = '__SOURCE_ROOT__'
SPEC = '__SPEC__'
ROWS_HEADING = '__ROWS_HEADING__'
ROWS_END_HEADING = '__ROWS_END_HEADING__'
ROW_COUNT_FILE = '__ROW_COUNT_FILE__'
TEST_DECL = __TEST_DECL_PATTERNS__
SOURCE_EXT = tuple(__SOURCE_EXT__)
errors = 0


def read(path):
    with open(path, encoding='utf-8', errors='replace') as handle:
        return handle.read()


def lines_of(path):
    return read(path).split('\n')


def git(*args):
    out = subprocess.run(['git'] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if out.returncode != 0:
        raise RuntimeError('git ' + ' '.join(args) + ': ' + out.stderr.decode().strip())
    return out.stdout.decode('utf-8', 'replace')


TRACKED = []


def tracked():
    if not TRACKED:
        TRACKED.extend(p for p in git('ls-files', '-z').split('\0') if p)
    return TRACKED


def declares(path, name):
    # how a test declares its name is language-specific: TEST_DECL comes from harness.json
    wanted = [pattern.replace('{name}', name) for pattern in TEST_DECL]
    for line in lines_of(path):
        code = line.lstrip()
        if any(code.startswith(w) for w in wanted):
            return True
    return False


def probe(name, finder):
    global errors
    try:
        found = finder()
    except Exception as problem:
        print('PROBE %s ERROR %s: %s' % (name, type(problem).__name__, str(problem).replace('\n', ' ')))
        errors += 1
        return
    print('PROBE %s %d' % (name, len(found)))
    for path, line, message in found:
        print('FINDING %s %s:%s %s' % (name, path, line, message.replace('\n', ' ')))


# the same slice and cell pattern the floor's own row parser uses; a second parser must read what it reads
ROW = re.compile(r'^\|[^|]+\|\s*`([\w./-]+__TEST_FILE_SUFFIX_RE__)::([^`]+)`\s*\|', re.M)
SEPARATOR = re.compile(r'^\|[\s:|-]+\|\s*$')


def spec_rows():
    text = read(SPEC)
    start = text.find('\n' + ROWS_HEADING)
    if start < 0:
        raise RuntimeError('%s: no "%s" heading' % (SPEC, ROWS_HEADING))
    after = text.find('\n' + ROWS_END_HEADING, start)
    section = text[start:after if after >= 0 else len(text)]
    base = text[:start].count('\n') + 1
    rows = [(m.group(1), m.group(2), base + section[:m.start()].count('\n')) for m in ROW.finditer(section)]
    shaped = [ln for ln in section.split('\n')
              if ln.startswith('|') and len(ln.split('|')) > 2 and not SEPARATOR.match(ln)
              and ln.split('|')[-2].strip().lower() != 'test']
    if len(shaped) != len(rows):
        raise RuntimeError('the exit-criteria section has %d row-shaped lines but %d parsed as file::test — a table went unread'
                           % (len(shaped), len(rows)))
    # the second, independent count: a parser reports the size of what it read or it shrinks silently
    declared = re.search(r'SPEC_ROW_COUNT\s*=\s*(\d+)', read(ROW_COUNT_FILE)) if ROW_COUNT_FILE and os.path.exists(ROW_COUNT_FILE) else None
    if declared and int(declared.group(1)) != len(rows):
        raise RuntimeError('%s parsed to %d rows, %s declares %s' % (SPEC, len(rows), ROW_COUNT_FILE, declared.group(1)))
    return rows


def untested_rows():
    # the row identity travels with the message: queue_uncovered asks which criterion, not just where
    out = []
    for name, test, line in spec_rows():
        path = name if '/' in name else SRC + '/' + name
        if not os.path.exists(path):
            out.append((name, test, line, 'no file %s for criterion %s::%s' % (path, name, test)))
        elif not declares(path, test):
            out.append((name, test, line, '%s declares no test named %s' % (path, test)))
    return out


def spec_untested():
    return [(SPEC, line, message) for _, _, line, message in untested_rows()]


RAILS_FILE = '__HARNESS_DIR__/RAILS.md'
RAILS_HEADER = '| Rail | What it means | Enforced by |'
IS_PATH = re.compile(r'\.(sh|ts|tsx|js|json|toml|md|lock)$')
# the check runs scripts named in one of these; one hop of indirection is followed below
HARNESS_FILES = [f for f in __HARNESS_FILES__ if os.path.exists(f)] + sorted(g for p in __HARNESS_GLOBS__ for g in glob.glob(p))


# enforcement is: the check runs it, the tool's settings wire it, or a hash covers it
WIRED = (['test-hashes.json', '__HARNESS_DIR__/loop.sh', '__HARNESS_DIR__/tasks.py']
         + sorted(glob.glob('__HARNESS_DIR__/lib/*.sh'))
         + sorted(glob.glob('.*/settings.json')) + sorted(glob.glob('.*/settings.local.json'))
         + sorted(glob.glob('__HARNESS_DIR__/roles/*.md')))


def harness_text():
    # a hook the settings wire and a file a hash covers are both enforcement; only the check
    # was consulted here, which read every hook the rails name as unenforced
    text = '\n'.join(read(f) for f in HARNESS_FILES + WIRED if os.path.exists(f))
    # one hop: check runs a script that runs a script, and both are enforcement
    for hop in sorted(set(re.findall(r'[\w./-]+\.(?:sh|ts)', text))):
        path = re.sub(r'^(\.\./)+', '', hop).lstrip('/')
        if os.path.exists(path) and os.path.isfile(path):
            text += '\n' + read(path)
    return text


def rail_rows():
    rows = []
    inside = False
    if not os.path.exists(RAILS_FILE):
        raise RuntimeError('%s does not exist -- the rails are unwritten, so nothing here is enforced' % RAILS_FILE)
    for index, line in enumerate(lines_of(RAILS_FILE)):
        if line.strip() == RAILS_HEADER:
            inside = True
            continue
        if inside and not line.startswith('|'):
            inside = False
        if not inside or SEPARATOR.match(line):
            continue
        cells = line.split('|')[1:-1]
        if len(cells) >= 3:
            rows.append((index + 1, cells))
    return rows


def resolve(token):
    if os.path.exists(token):
        return token
    if '/' in token:
        return None
    # CLAUDE.md names most enforcement by basename; a tracked file of that name is the enforcement
    named = [p for p in tracked() if os.path.basename(p) == token]
    return named[0] if named else None


def rail_unenforced():
    found = []
    harness = harness_text()
    for line, cells in rail_rows():
        for token in re.findall(r'`([^`]+)`', cells[2]):
            if '::' in token:
                name, test = token.split('::', 1)
                path = resolve(name)
                if path is None:
                    found.append((RAILS_FILE, line, 'enforced by %s, and %s does not exist' % (token, name)))
                elif not declares(path, test):
                    found.append((RAILS_FILE, line, 'enforced by %s, and %s declares no such test' % (token, path)))
                continue
            if '/' not in token and not token.startswith('.') and not IS_PATH.search(token):
                continue
            path = resolve(token.rstrip('/'))
            if path is None:
                found.append((RAILS_FILE, line, 'enforced by %s, which does not exist' % token))
                continue
            if path.endswith('.test.ts') or not (path.endswith('.sh') or path.endswith('.ts')):
                continue
            if os.path.basename(path) not in harness:
                found.append((RAILS_FILE, line, '%s exists and the check does not run it' % token))
    return found


HASHES = 'test-hashes.json'


def hash_uncovered():
    """A rail that names files and names test-hashes.json as its enforcement is true only if
    every file it names has a key there. `rail-unenforced` goes quiet as soon as the file exists,
    whatever is in it, which made `harness-immutable` aspirational."""
    keys = json.load(open(HASHES)) if os.path.exists(HASHES) else None
    found = []
    for line, cells in rail_rows():
        if HASHES not in cells[2]:
            continue
        rail = cells[0].strip()
        for token in re.findall(r'`([^`]+)`', cells[1]):
            token = token.strip().rstrip('/')
            if token == HASHES or ('/' not in token and not IS_PATH.search(token)):
                continue
            if not os.path.exists(token):
                continue
            if keys is None:
                found.append((RAILS_FILE, line, '%s names %s and %s does not exist, so the rail covers nothing'
                              % (rail, token, HASHES)))
            elif not any(key == token or key.startswith(token + '#') for key in keys):
                found.append((RAILS_FILE, line, '%s names %s and %s has no key for it' % (rail, token, HASHES)))
    return found


OPEN_STATUS = ('proposed', 'ready', 'blocked', 'review')
ROW_REF = re.compile(r'([\w./-]+__TEST_FILE_SUFFIX_RE__)::([^`,\n]+)')


def queue_uncovered():
    """The queue against the spec, both directions -- Spec Kit's `analyze` step. `spec-untested`
    asks whether a criterion has a test and `queue-hygiene` asks whether the queue is internally
    consistent; neither asks whether the two agree  -- Spec Kit's `analyze` step."""
    found = []
    defined = set((name, test) for name, test, _ in spec_rows())
    claimed = set()
    for block in task_blocks():
        _, status = field(block, 'status')
        rows_at, rows = field(block, 'rows')
        if rows_at is None or status not in OPEN_STATUS:
            continue
        for name, test in ROW_REF.findall(rows):
            ref = (name, test.strip().rstrip('`').strip())
            claimed.add(ref)
            if ref not in defined:
                found.append(('TASKS.md', rows_at, '%s claims row %s::%s and the exit criteria define no such row'
                              % (block['id'], ref[0], ref[1])))
    for name, test, line, message in untested_rows():
        if (name, test) not in claimed:
            found.append((SPEC, line, '%s::%s is untested and no open task names it (%s)' % (name, test, message)))
    return found


def normal(text):
    return re.sub(r'\s+', ' ', re.sub(r'[^a-z0-9 ]', ' ', text.lower())).strip()


def friction_repeat():
    """The round-end question, enforced. First occurrence is evidence and stays in PROGRESS.md;
    the SECOND becomes a rule in LEARNINGS.md, or the loop is paying for it every round.
    ponytail: exact normalised match, so a reworded repeat escapes it -- token overlap if that
    turns out to be the common case."""
    seen = {}
    for index, line in enumerate(lines_of('PROGRESS.md')):
        if not line.startswith('friction:'):
            continue
        text = line.split(':', 1)[1].strip()
        key = normal(text)
        if not key or key in ('none', 'na', 'nothing'):
            continue
        seen.setdefault(key, []).append((index + 1, text))
    learned = normal(read('LEARNINGS.md')) if os.path.exists('LEARNINGS.md') else ''
    return [('PROGRESS.md', hits[-1][0],
             'the same friction is recorded %d times and LEARNINGS.md carries no rule for it: %s'
             % (len(hits), hits[-1][1][:90]))
            for key, hits in sorted(seen.items()) if len(hits) > 1 and key not in learned]


def driver():
    status, log = os.environ['DRIVER_STATUS'], os.environ['DRIVER_LOG']
    if status != '0':
        raise RuntimeError('the driver exited %s without reaching the artifact: %s'
                           % (status, log.strip().replace('\n', ' ')[:200]))
    return [(os.environ['DRIVER'], 0, line[len('FINDING '):].strip())
            for line in log.split('\n') if line.startswith('FINDING ')]


CITED = re.compile(r'^(bun|bunx|git|npm|turbo|node|ps|sed|grep|touch|rm|chmod)\b')


def cites_something(text):
    for token in re.findall(r'`([^`]+)`', text):
        if '/' in token or re.search(r'\.\w', token) or CITED.match(token):
            return True
    return False


def learning_entries():
    entries, open_at = [], None
    for index, line in enumerate(lines_of('LEARNINGS.md')):
        if line.startswith('- '):
            entries.append([index + 1, line])
            open_at = len(entries) - 1
        elif open_at is not None and line.startswith('  ') and line.strip():
            entries[open_at][1] += ' ' + line.strip()
        elif not line.strip():
            open_at = None
    return entries


def learning_unenforced():
    return [('LEARNINGS.md', at, 'entry names no file, command or hook: %s' % text.strip()[:90])
            for at, text in learning_entries() if not cites_something(text)]


LEARNINGS_CAP = __LEARNINGS_CAP__
DATED = re.compile(r'^- \[\d{4}-\d{2}-\d{2}\]')


def learning_ungated():
    """A rule written from a repeated friction is a write to the agent's standing context, and an
    unvalidated write is the failure mode the field has measured: reflective memory made two
    ALFWorld environments strictly worse than no memory at all, with 0 of 121 reflections naming the
    correct target (arXiv:2605.29463), and accumulation without a gate regressed below the no-skills
    baseline (arXiv:2605.29668). So a dated rule names the eval that holds it, and the library is
    capacity-bounded the way GRASP's is. `[seed]` entries predate the gate and are exempt."""
    found = []
    entries = learning_entries()
    for at, text in entries:
        if not DATED.match(text.strip()):
            continue
        named = re.findall(r'evals/([\w.-]+)', text)
        if not named:
            found.append(('LEARNINGS.md', at, 'dated rule names no eval, so nothing decided it was '
                                              'worth its place: %s' % text.strip()[:80]))
            continue
        for name in named:
            if not os.path.isdir('evals/' + name):
                found.append(('LEARNINGS.md', at, 'names evals/%s, which does not exist' % name))
    if len(entries) > LEARNINGS_CAP:
        found.append(('LEARNINGS.md', 0, 'the rule library holds %d entries against a cap of %d. Every '
                      'entry is read at the start of every task; adding one means removing one'
                      % (len(entries), LEARNINGS_CAP)))
    return found


def ponytail_ceiling():
    found = []
    marker = 'ponytail' + ':'
    for path in tracked():
        if not path.endswith(SOURCE_EXT) or not os.path.exists(path):
            continue
        for index, line in enumerate(lines_of(path)):
            if marker in line:
                found.append((path, index + 1, line.strip()[:100]))
    return found


HEADING = re.compile(r'^## \[(T-\d+)\]')


def task_blocks():
    blocks = []
    current = None
    for index, line in enumerate(lines_of('TASKS.md')):
        match = HEADING.match(line)
        if match:
            current = {'id': match.group(1), 'line': index + 1, 'body': []}
            blocks.append(current)
            continue
        if line.startswith('## '):
            current = None
            continue
        if current is not None:
            current['body'].append((index + 1, line))
    return blocks


def field(block, key):
    for line, text in block['body']:
        if text.startswith(key + ':'):
            return line, text.split(':', 1)[1].strip()
    return None, None


def rejection_stale():
    found = []
    for block in task_blocks():
        line, status = field(block, 'status')
        if status is None:
            continue
        notes_at, _ = field(block, 'notes')
        notes = '\n'.join(t for ln, t in block['body'] if notes_at is not None and ln >= notes_at)
        if 'REJECTED' in notes and status != 'ready':
            found.append(('TASKS.md', line, '%s notes carry REJECTED while status is %s' % (block['id'], status)))
        if status == 'needs-spec':
            found.append(('TASKS.md', line, '%s is parked at needs-spec' % block['id']))
    return found


def queue_hygiene():
    found = []
    blocks = task_blocks()
    ids = set(b['id'] for b in blocks)
    seen = set()
    for block in blocks:
        if block['id'] in seen:
            found.append(('TASKS.md', block['line'], 'a second block is numbered %s' % block['id']))
        seen.add(block['id'])
        status_at, status = field(block, 'status')
        if status is None:
            found.append(('TASKS.md', block['line'], '%s has no status line' % block['id']))
        blocked_at, blocked = field(block, 'blockedBy')
        for other in re.findall(r'T-\d+', blocked or ''):
            if other not in ids:
                found.append(('TASKS.md', blocked_at, '%s is blocked by %s, which no block defines' % (block['id'], other)))
        # only on a done block: an unfinished task's scope names the files it will create
        scope_at, scope = field(block, 'scope')
        for pattern in (scope or '').split(',') if status == 'done' else []:
            pattern = pattern.strip().strip('`')
            if pattern and not glob.glob(pattern, recursive=True):
                found.append(('TASKS.md', scope_at, '%s is done and its scope %s matches no file' % (block['id'], pattern)))
    return found


def check_red():
    status, log = os.environ['CHECK_STATUS'], os.environ['CHECK_LOG']
    if status == 'nested':
        raise RuntimeError(log)
    if status == '127':
        raise RuntimeError('the check could not be run: ' + log.strip()[:120])
    if status == '0':
        return []
    first = re.search(r'^\s*(\S+#\S+):\s+ERROR', log, re.M) or re.search(r'^\s*Failed:\s*(\S+)', log, re.M)
    named = re.search(r'^.*\(fail\) (.+?)( \[[\d.]+m?s\])?$', log, re.M)
    return [(HARNESS_FILES[0] if HARNESS_FILES else 'check', 0, 'the check exited %s, first failing task %s%s'
             % (status, first.group(1) if first else 'unnamed',
                ', first failure ' + named.group(1) if named else ''))]


DOCS = set(__DOCS__)
HARNESS = set(__HARNESS_ALLOW__)
MACHINERY = set(__MACHINERY__)
ALLOWED_PREFIXES = __ALLOWED_PREFIXES__


def allowed(path):
    return any(path.startswith(prefix) for prefix in ALLOWED_PREFIXES) or path in DOCS or path in HARNESS


def machinery(path):
    for part in path.strip('/').split('/'):
        if part in MACHINERY or part.startswith('.env') or part.endswith('.tsbuildinfo'):
            return True
    return False


def litter():
    found = []
    for path in tracked():
        if not allowed(path):
            found.append((path, 0, 'tracked and neither product nor a document that governs it'))
    records = [r for r in git('status', '--porcelain', '-uall', '--ignored', '-z').split('\0') if r]
    skip = False
    for record in records:
        if skip:
            skip = False
            continue
        code, path = record[:2], record[3:]
        if code[0] in 'RC':
            skip = True
        if code not in ('??', '!!'):
            continue
        if not allowed(path) and not machinery(path):
            found.append((path, 0, 'untracked or ignored, on no allowlist and no known machinery'))
    return sorted(found)


probe('spec-untested', spec_untested)
probe('queue-uncovered', queue_uncovered)
probe('rail-unenforced', rail_unenforced)
probe('hash-uncovered', hash_uncovered)
probe('learning-unenforced', learning_unenforced)
probe('learning-ungated', learning_ungated)
probe('ponytail-ceiling', ponytail_ceiling)
probe('rejection-stale', rejection_stale)
probe('queue-hygiene', queue_hygiene)
probe('friction-repeat', friction_repeat)
probe('check-red', check_red)
probe('litter', litter)
if os.environ['DRIVER_STATUS'] == 'off':
    # not a count of zero: a probe that did not run has found nothing, which is no evidence about
    # the tree (LEARNINGS.md, zero-as-pass). The scout reports this line and proposes from it.
    print('PROBE driver OFF -- no driverCommand in harness.json, or HARNESS_DRIVER is unset. '
          'Nothing here exercised the built artifact.')
else:
    probe('driver', driver)
sys.exit(1 if errors else 0)
PY
