#!/usr/bin/env bash
# Print the CHANGELOG.md section for a version (used as release notes).
#   tools/release/notes.sh 0.2.0
set -euo pipefail
v="${1:?usage: notes.sh X.Y.Z}"
v="${v#v}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
notes="$(awk -v ver="$v" '
  /^## \[/ { if (on) exit; if (index($0, "## [" ver "]") == 1) { on = 1; next } }
  on { print }
' "$root/CHANGELOG.md" | sed -e '/./,$!d')"
if [[ -z "${notes//[[:space:]]/}" ]]; then
  echo "See the commit history for changes in $v."
else
  printf '%s\n' "$notes"
fi
