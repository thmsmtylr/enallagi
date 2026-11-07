#!/usr/bin/env bash
# Remove the role and nothing else: a generic one-line decomposer.md takes the place of the
# installed one, so the agent has the stage prompt and no protocol, template or skill.
set -u
role=.enallagi/roles/decomposer.md
grep -q '^Protocol:' "$role" || {
  echo "ablate.sh: the installed role carries no protocol, so nothing was measured" >&2
  exit 1
}
printf '%s\n' 'You are the decomposer: do what the prompt asks.' >"$role"
