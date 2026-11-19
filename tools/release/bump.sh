#!/usr/bin/env bash
# Set the release version everywhere it is recorded, and open its CHANGELOG section.
#   tools/release/bump.sh 0.2.0        (or: task release:bump -- 0.2.0)
# Cargo.toml [workspace.package] is the source of truth; the release workflow
# refuses tags that do not match it.
set -euo pipefail
v="${1:?usage: bump.sh X.Y.Z[-pre]}"
v="${v#v}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a semver version: $v" >&2; exit 2; }
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

# First `version = "…"` in Cargo.toml is [workspace.package].
sed -i -E "0,/^version = \"[^\"]+\"/s//version = \"$v\"/" Cargo.toml
for f in apps/desktop/tauri.conf.json ui/apps/admin/package.json ui/apps/desktop/package.json ui/packages/kit/package.json; do
  sed -i -E "0,/\"version\": \"[^\"]+\"/s//\"version\": \"$v\"/" "$f"
done
cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null 2>&1 \
  || echo "warning: could not refresh Cargo.lock; run 'cargo update --workspace'" >&2

# CHANGELOG: turn "## [Unreleased]" into the new version and start a fresh one.
if grep -q '^## \[Unreleased\]' CHANGELOG.md && ! grep -q "^## \[$v\]" CHANGELOG.md; then
  sed -i -E "0,/^## \[Unreleased\]/s//## [Unreleased]\n\n## [$v] - $(date +%Y-%m-%d)/" CHANGELOG.md
fi

echo "version set to $v"
echo "next: review CHANGELOG.md, commit, then: git tag v$v && git push origin v$v"
