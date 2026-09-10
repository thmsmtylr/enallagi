#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: the LEARNINGS.md entry saying a change
# to the check changes `fail_name` with it. The runner calls ablate.sh BEFORE setup.sh (eval.rs,
# "ablation runs before setup"), and setup.sh is what writes that entry, so the removal is this
# marker: setup.sh then builds the same fixture without the entry and changes nothing else.
set -u
: >.eval-ablated
