#!/usr/bin/env bash
# Remove the role and nothing else: an empty installed auditor.md takes the place of the shipped one,
# so `enallagi audit` hands the agent each class with no role to read it by.
set -u
mkdir -p .enallagi/roles
: >.enallagi/roles/auditor.md
