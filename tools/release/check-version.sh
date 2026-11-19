#!/usr/bin/env bash
# Verify a release tag matches every recorded version; print the version.
#   tools/release/check-version.sh v0.2.0
set -euo pipefail
tag="${1:?usage: check-version.sh vX.Y.Z}"
v="${tag#v}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

fail=0
check() { # file actual
  if [[ "$2" != "$v" ]]; then echo "::error file=$1::version is '$2' but the tag is '$tag' (run tools/release/bump.sh $v)" >&2; fail=1; fi
}
check Cargo.toml "$(sed -n -E '0,/^version = "([^"]+)"/s//\1/p' Cargo.toml | tail -1)"
for f in apps/desktop/tauri.conf.json ui/apps/admin/package.json ui/apps/desktop/package.json ui/packages/kit/package.json; do
  check "$f" "$(grep -m1 -oE '"version": "[^"]+"' "$f" | sed -E 's/.*"([^"]+)"$/\1/')"
done
[[ $fail -eq 0 ]] || exit 1
echo "$v"
