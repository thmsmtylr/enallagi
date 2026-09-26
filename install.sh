#!/bin/sh
# Installs the enallagi release binary for this machine, checked against the release's SHA256SUMS.
#   ENALLAGI_VERSION=vX.Y.Z  pins a release (default: the latest)
#   ENALLAGI_INSTALL=<dir>   installs there (default: $HOME/.local/bin)
set -eu

repo=https://github.com/thmsmtylr/enallagi
if [ -n "${ENALLAGI_VERSION:-}" ]; then
  base=${ENALLAGI_BASE_URL:-$repo/releases/download/$ENALLAGI_VERSION}
else
  base=${ENALLAGI_BASE_URL:-$repo/releases/latest/download}
fi
dir=${ENALLAGI_INSTALL:-$HOME/.local/bin}

fail() {
  echo "install.sh: $*" >&2
  exit 1
}

case $(uname -sm) in
  "Darwin arm64" | "Darwin aarch64") target=aarch64-apple-darwin ;;
  "Darwin x86_64") target=x86_64-apple-darwin ;;
  "Linux aarch64" | "Linux arm64") target=aarch64-unknown-linux-musl ;;
  "Linux x86_64") target=x86_64-unknown-linux-musl ;;
  *) fail "no release asset for $(uname -sm)" ;;
esac
asset=enallagi-$target

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$base/$asset" -o "$tmp/$asset" || fail "download $base/$asset"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" || fail "download $base/SHA256SUMS"

want=$(awk -v f="$asset" '$2 == f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || fail "SHA256SUMS lists no $asset"
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$asset" | awk '{ print $1 }')
else
  got=$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')
fi
[ "$got" = "$want" ] || fail "$asset hashes to $got, SHA256SUMS says $want"

mkdir -p "$dir"
chmod +x "$tmp/$asset"
mv "$tmp/$asset" "$dir/enallagi"
echo "installed $dir/enallagi"
"$dir/enallagi" --version
