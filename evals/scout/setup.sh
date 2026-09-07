#!/usr/bin/env bash
# A fresh install already emits FINDING lines (spec-untested, rail-unenforced, hash-uncovered), so
# the fixture is the install itself. Record what the queue looks like before the role runs.
set -u
printf 'ready=%s proposed=%s\n' "$(grep -c '^status: ready' TASKS.md)" "$(grep -c '^status: proposed' TASKS.md)" >evalbase.txt
git add evalbase.txt && git commit -qm 'eval: baseline' >/dev/null # tracked, so the assert can tell the scout's files from this one
