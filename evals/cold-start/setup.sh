#!/usr/bin/env bash
# An ordinary repository the harness did not grow up in: source, tests, docs, a README, a licence,
# a lockfile and a green check. `enallagi init` runs against it exactly once, with no hand editing.
set -e

mkdir -p fixture/src fixture/tests fixture/docs
cd fixture

printf '# widget\n\nA widget prints its name.\n' >README.md
printf 'MIT License\n\nCopyright (c) 2026 widget\n' >LICENSE
printf '# Usage\n\nSource `src/widget.sh` and call `widget`.\n' >docs/usage.md
printf '#!/usr/bin/env bash\nwidget() { echo widget; }\n' >src/widget.sh
printf '#!/usr/bin/env bash\nset -e\n. src/widget.sh\n[ "$(widget)" = widget ]\necho "test result: ok. 1 passed; 0 failed; 0 ignored"\n' >tests/widget_test.sh
printf '{ "lockfileVersion": 1, "packages": {} }\n' >widget.lock
printf '#!/usr/bin/env bash\nset -e\nbash tests/widget_test.sh\n' >check.sh
chmod +x check.sh src/widget.sh tests/widget_test.sh

git init -q .
git config user.email fixture@example.com
git config user.name fixture
git add -A
git -c commit.gpgsign=false commit -qm "widget"

bash check.sh
enallagi init --adapter claude
# init ignores the harness directory rather than tracking it, so there may be nothing to commit
git add -A
git diff --cached --quiet || git -c commit.gpgsign=false commit -qm "enallagi init"
