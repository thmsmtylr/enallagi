#!/usr/bin/env bash
# The example driver. Copy it, fill in the three sections, then point harness.json at it:
#
#   "driverCommand": "$HARNESS_ROOT/.harness/driver.sh"
#
# and run the probes with HARNESS_DRIVER=1. Until then this file finds nothing, and **finding
# nothing is not the same as nothing being wrong** — it is the same as not having looked.
#
# You get a throwaway working directory and a stripped environment. `$HARNESS_ROOT` is the repo.
# Exit 0 whenever you REACHED the artifact, whatever you found. Exit non-zero only when you could
# not reach it: that is `PROBE driver ERROR`, and a scout proposes nothing from a probe that did
# not run. Print one line per shortfall, each beginning `FINDING `.
#
# `driver.sh` in the package this was installed from is a worked example.
set -u

# 1. REACH THE ARTIFACT. Build it, start it, open it — through the surface a user touches, not
#    through your source tree. If this fails, exit non-zero: you have no result, not a clean one.
#
#    Example:  "$HARNESS_ROOT/build.sh" >/dev/null 2>&1 || exit 3

# 2. DRIVE ONE APPROVED REQUEST through a FRESH interaction. If what you drive is itself an agent,
#    it gets its own session, this directory, and only the tools your product exposes. An artifact
#    that shares context with the loop is not being tested, it is being interviewed.
#
#    Example:  answer=$(printf 'add milk to my list\n' | "$HARNESS_ROOT/build/cli")

# 3. READ THE PERSISTENT EFFECT, never the answer. Diff the store, the file, the row your artifact
#    was supposed to change. Software describes the correct action without performing it all day.
#    Print a FINDING for each shortfall, in the words you would want in a task block.
#
#    Example:  grep -q milk "$HARNESS_ROOT/build/store.json" \
#                || echo "FINDING the CLI answered 'added milk' and the store has no milk row"

exit 0
