#!/usr/bin/env bash
# A fresh install already emits FINDING lines (spec-untested, rail-unenforced, hash-uncovered), so
# the fixture is the install itself. Record what the queue looks like before the role runs.
set -u
printf 'ready=%s triage=%s\n' "$(enallagi tasks ids-at ready-for-agent | grep -c .)" "$(enallagi tasks ids-at needs-triage | grep -c .)" >evalbase.txt
git add evalbase.txt && git commit -qm 'eval: baseline' >/dev/null # tracked, so the assert can tell the scout's files from this one
git -C .enallagi add -A && git -C .enallagi commit -qm 'eval: baseline' >/dev/null # so the assert can tell the scout's instance edits from the install
