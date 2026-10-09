#!/usr/bin/env bash
# Remove the role and nothing else: an empty installed auditor.md takes the place of the shipped one,
# so `enallagi audit` hands the agent the findings with no role to group them by.
set -u
mkdir -p .enallagi/roles
: >.enallagi/roles/auditor.md
