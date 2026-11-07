#!/usr/bin/env bash
# Remove the role and nothing else: a generic one-line role takes the place of every installed
# decomposer.md, the adapter's agent file included, so the agent has the stage prompt and no
# protocol, template or skill.
set -u
role=.enallagi/roles/decomposer.md
grep -q '^Protocol:' "$role" || {
  echo "ablate.sh: the installed role carries no protocol, so nothing was measured" >&2
  exit 1
}
find .enallagi -name decomposer.md -not -path '*/.git/*' | while read -r f; do
  printf '%s\n' 'You are the decomposer: do what the prompt asks.' >"$f"
done
