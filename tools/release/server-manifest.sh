#!/usr/bin/env bash
# Write server-latest.json for the server's update checker.
#   tools/release/server-manifest.sh <dist-dir> <version> <notes-file> <download-base-url>
# Expects <dist-dir>/armaxsync-server-<target>[.exe] and a matching .sig next to each.
set -euo pipefail
dist="${1:?dist dir}"; v="${2:?version}"; notes="${3:?notes file}"; base="${4:?download base url}"

platforms='{}'
for bin in "$dist"/armaxsync-server-*; do
  case "$bin" in *.sig|*.deb|*.rpm) continue ;; esac
  name="$(basename "$bin")"
  target="${name#armaxsync-server-}"; target="${target%.exe}"
  [[ -f "$bin.sig" ]] || { echo "missing signature for $name" >&2; exit 1; }
  sha="$(sha256sum "$bin" | cut -d' ' -f1)"
  platforms="$(jq --arg t "$target" --arg url "$base/$name" --arg sig "$(cat "$bin.sig")" \
    --arg sha "$sha" --argjson size "$(stat -c %s "$bin")" \
    '. + {($t): {url: $url, signature: $sig, sha256: $sha, size: $size}}' <<<"$platforms")"
done

jq -n --arg v "$v" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --rawfile notes "$notes" \
  --argjson platforms "$platforms" \
  '{version: $v, pub_date: $date, notes: $notes, platforms: $platforms}'
