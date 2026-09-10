#!/usr/bin/env bash
# Remove the rule this eval exists to hold, and nothing else: the LEARNINGS.md entry saying a file
# the task merely reads is not scope. The runner calls ablate.sh BEFORE setup.sh (eval.rs, "ablation
# runs before setup"), and setup.sh is what writes that entry into the fixture, so the removal is
# this marker — setup.sh then builds the same fixture without the entry and changes nothing else.
# setup.sh asserts it wrote the entry in the unablated arm, so that arm cannot pass having written
# nothing and left the two arms identical.
set -u
: >.eval-ablated
