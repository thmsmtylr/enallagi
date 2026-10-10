#!/bin/sh
# Writes the release bump onto the checked-out branch: the version after origin/main's, and its
# CHANGELOG.md section. Usage: bump.sh <pull request title>. Prints the new version.
set -eu

title=${1:?usage: bump.sh <pull request title>}
manifest=crates/harness/Cargo.toml

current=$(git show "origin/main:$manifest" | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
case $current in
  *-*.*) next=${current%.*}.$((${current##*.} + 1)) ;;
  *-*) echo "bump.sh: no counter in pre-release $current" >&2; exit 1 ;;
  *.*.*) next=${current%.*}.$((${current##*.} + 1)) ;;
  *) echo "bump.sh: no version on origin/main" >&2; exit 1 ;;
esac

tmp=$(mktemp "${TMPDIR:-/tmp}/bump.XXXXXX")
trap 'rm -f "$tmp"' EXIT

# only the first `version =` line is the package's own
awk -v ver="$next" '!done && /^version = "/ { $0 = "version = \"" ver "\""; done = 1 } { print }' \
  "$manifest" > "$tmp"
cat "$tmp" > "$manifest"
cargo update -w -q

fallback="- $(printf '%s\n' "$title" | sed 's/^[a-z]*\(([^)]*)\)\{0,1\}!\{0,1\}: //')"
heading="## v$next ($(date -u +%Y-%m-%d))"
awk -v heading="$heading" -v fallback="$fallback" '
  function flush() {
    while (n > 0 && lines[n] == "") n--
    print "## Unreleased"; print ""; print heading; print ""
    if (n == 0) print fallback
    for (i = 1; i <= n; i++) print lines[i]
    print ""
  }
  state == 0 && $0 == "## Unreleased" { state = 1; next }
  state == 1 && /^## / { flush(); state = 2 }
  state == 1 { if ($0 != "" || n > 0) lines[++n] = $0; next }
  { print }
  END { if (state == 1) flush() }
' CHANGELOG.md > "$tmp"
cat "$tmp" > CHANGELOG.md
echo "$next"
