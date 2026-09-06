#!/usr/bin/env python3
"""The queue, parsed once.

TASKS.md used to be read by seven separate awk and sed programs inside loop.sh, each
re-implementing "find the block, read the field". Two defects came from that: BSD sed reads
`[ \t]` as space-backslash-t, and the awk could not see code fences, so the block-format example
in the template was a task a lane could take.

Every question the launcher asks about the queue is answered here.

    tasks.py list [file]                    id, title, status -- one line per block, file order
    tasks.py ready-unattended [file]        first ready, unattended, unblocked id
    tasks.py ids-at <status> [file]         every id at one status
    tasks.py block <id> [file]              one block, verbatim
    tasks.py field <id> <key> [file]        one field of one block
    tasks.py set-status <id> <status> [reason] [file]
    tasks.py unblock [file]                 blocked -> ready where every blocker is done
    tasks.py rejections [file]              the kill lines from DECISIONS.md
    tasks.py --selftest
"""
import io
import os
import re
import sys

HEADING = re.compile(r'^## \[(T-\d+)\]\s*(.*)')
FENCE = re.compile(r'^\s*(```|~~~)')


def read(path):
    with io.open(path, encoding='utf-8', errors='replace') as handle:
        return handle.read()


def parse(text):
    """Blocks in file order. A heading inside a fenced code block is documentation, not a task."""
    blocks, current, fenced = [], None, False
    for index, line in enumerate(text.split('\n')):
        if FENCE.match(line):
            fenced = not fenced
        if not fenced:
            found = HEADING.match(line)
            if found:
                current = {'id': found.group(1), 'title': found.group(2).strip(),
                           'line': index + 1, 'body': []}
                blocks.append(current)
                continue
            if line.startswith('## ') or line.startswith('# '):
                current = None
                continue
        if current is not None:
            current['body'].append((index + 1, line))
    return blocks


def field(block, key):
    """The first `key: value` line of a block. Values are stripped; a missing field is None."""
    for _, line in block['body']:
        if line.startswith(key + ':'):
            return line.split(':', 1)[1].strip()
    return None


def blockers(block):
    raw = (field(block, 'blockedBy') or '').replace(' ', '')
    if raw in ('', 'none'):
        return []
    return [b for b in raw.split(',') if b]


def ready_unattended(blocks):
    """First task a lane may take: ready, not attended, every blocker done."""
    status = dict((b['id'], field(b, 'status')) for b in blocks)
    for block in blocks:
        if field(block, 'status') != 'ready' or field(block, 'attended') == 'true':
            continue
        if all(status.get(b) == 'done' for b in blockers(block)):
            return block['id']
    return None


def unblock(text):
    """Rewrite `status: blocked` to ready wherever every blocker is done."""
    blocks = parse(text)
    status = dict((b['id'], field(b, 'status')) for b in blocks)
    release = set(b['id'] for b in blocks
                  if field(b, 'status') == 'blocked' and blockers(b)
                  and all(status.get(x) == 'done' for x in blockers(b)))
    if not release:
        return text
    lines = text.split('\n')
    for block in blocks:
        if block['id'] not in release:
            continue
        for at, line in block['body']:
            if line.startswith('status:'):
                lines[at - 1] = 'status: ready'
                break
    return '\n'.join(lines)


def set_status(text, task, status, reason=''):
    """Rewrite one block's status in place, recording why next to it."""
    for block in parse(text):
        if block['id'] != task:
            continue
        for at, line in block['body']:
            if not line.startswith('status:'):
                continue
            lines = text.split('\n')
            lines[at - 1] = 'status: ' + status
            if reason:
                lines.insert(at, 'gate: ' + reason)
            return '\n'.join(lines)
    return text


def load(argv, position):
    """Every command takes an optional trailing file, so fixtures need no environment."""
    path = argv[position] if len(argv) > position else 'TASKS.md'
    if not os.path.exists(path):
        return path, ''
    return path, read(path)


def main(argv):
    command = argv[1] if len(argv) > 1 else ''

    if command == 'list':
        # File order, never sorted: ready_unattended takes the FIRST ready block in file order, so
        # position in the file is queue priority and re-ordering the view would lie about it.
        _, text = load(argv, 2)
        for block in parse(text):
            print(u'%s  %s  \u2192 %s'
                  % (block['id'], block['title'][:44], field(block, 'status') or '?'))
        return 0

    if command == 'ready-unattended':
        _, text = load(argv, 2)
        found = ready_unattended(parse(text))
        if found:
            print(found)
        return 0

    if command == 'ids-at':
        _, text = load(argv, 3)
        for block in parse(text):
            if field(block, 'status') == argv[2]:
                print(block['id'])
        return 0

    if command == 'block':
        _, text = load(argv, 3)
        for block in parse(text):
            if block['id'] == argv[2]:
                lines = text.split('\n')
                print('\n'.join(lines[block['line'] - 1:block['line'] - 1 + len(block['body']) + 1]))
        return 0

    if command == 'field':
        _, text = load(argv, 4)
        for block in parse(text):
            if block['id'] == argv[2]:
                value = field(block, argv[3])
                if value is not None:
                    print(value)
        return 0

    if command == 'set-status':
        path, text = load(argv, 5)
        out = set_status(text, argv[2], argv[3], argv[4] if len(argv) > 4 else '')
        if out == text:
            print('tasks.py: no block %s, nothing written' % argv[2], file=sys.stderr)
            return 1
        io.open(path, 'w', encoding='utf-8').write(out)
        return 0

    if command == 'unblock':
        path, text = load(argv, 2)
        out = unblock(text)
        if out != text:
            io.open(path, 'w', encoding='utf-8').write(out)
        return 0

    if command == 'rejections':
        path = argv[2] if len(argv) > 2 else 'DECISIONS.md'
        if not os.path.exists(path):
            return 0
        inside = False
        for line in read(path).split('\n'):
            if line.startswith('## Rejected findings'):
                inside = True
                continue
            if inside and HEADING.match(line):
                break
            if inside and line.startswith('- ['):
                print(line)
        return 0

    if command == '--selftest':
        return selftest()

    print(__doc__, file=sys.stderr)
    return 2


FIXTURE = '''# TASKS

## Block format

```
## [T-042] the example in the documentation, which is not a task
blockedBy:
status: ready
```

---

## [T-001] a blocker that is done
blockedBy:
status: done

## [T-002] ready, and its blocker is not
blockedBy: T-009
status: ready

## [T-003] ready, attended, not a lane's
blockedBy: T-001
status: ready
attended: true

## [T-004] the one a lane may take
blockedBy: T-001
status: ready
rows: none — harness

## [T-005] blocked, and every blocker is done
blockedBy: T-001
status: blocked

## [T-006] blockedBy spelled none
blockedBy: none
status: review

## [T-007] blocked, and its blocker is not done
blockedBy: T-009
status: blocked

## [T-009] the blocker T-002 waits on
blockedBy:
status: ready
'''


def selftest():
    failures = []

    def check(name, want, got):
        if want == got:
            print('ok    ' + name)
        else:
            print('FAIL  ' + name)
            print('      want [%s] got [%s]' % (want, got))
            failures.append(name)

    blocks = parse(FIXTURE)
    check('a title is read off the heading', 'the one a lane may take',
          [b['title'] for b in blocks if b['id'] == 'T-004'][0])
    check('a heading inside a code fence is not a task', False, 'T-042' in [b['id'] for b in blocks])
    check('every real block is parsed', 8, len(blocks))
    check('a ready task whose blocker is not done is not selected', True,
          ready_unattended(blocks) != 'T-002')
    check('an attended ready task is skipped', True, ready_unattended(blocks) != 'T-003')
    check('the first takeable task is selected', 'T-004', ready_unattended(blocks))
    check('blockedBy none is no blocker', [], blockers([b for b in blocks if b['id'] == 'T-006'][0]))
    check('a field is read off its own block', 'none — harness',
          field([b for b in blocks if b['id'] == 'T-004'][0], 'rows'))
    check('a missing field is None', None, field(blocks[0], 'rows'))

    released = unblock(FIXTURE)
    check('a blocked task whose blockers are done is released', True,
          '## [T-005] blocked, and every blocker is done\nblockedBy: T-001\nstatus: ready' in released)
    check('unblock leaves every other status alone', FIXTURE.count('status:'),
          released.count('status:'))
    check('unblock never releases a task whose blocker is not done', 'blocked',
          field([b for b in parse(released) if b['id'] == 'T-007'][0], 'status'))

    changed = set_status(FIXTURE, 'T-004', 'ready', 'the gate was red')
    check('set-status rewrites the named block', True,
          '## [T-004] the one a lane may take\nblockedBy: T-001\nstatus: ready\ngate: the gate was red' in changed)
    check('set-status rewrites exactly one status line', 1, changed.count('gate: '))
    check('set-status leaves the file otherwise intact', True, '## [T-009]' in changed)
    check('set-status on an unknown id changes nothing', FIXTURE, set_status(FIXTURE, 'T-999', 'done'))

    print('tasks.py selftest: %s' % ('all assertions passed.' if not failures else 'FAILURES above.'))
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
