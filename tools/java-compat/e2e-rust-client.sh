#!/usr/bin/env bash
# End-to-end: the ArmaXSync CLI syncs from a repository built by the legacy
# ArmA3Sync builder (Java), served by a range-capable static HTTP server.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi; }
AXS="${AXS_BIN:-$ROOT/target/debug/armaxsync}"
CP="$(cat "$HERE/build/legacy-app/classpath.txt")"
PORT="${PORT:-18735}"
PY=""; for c in python3 python; do if command -v "$c" >/dev/null 2>&1 && "$c" -c "import sys" >/dev/null 2>&1; then PY="$(command -v "$c")"; break; fi; done
SRV=""
WORK="$(mktemp -d)"
trap 'kill $SRV 2>/dev/null || true; if [ -z "${KEEP:-}" ]; then rm -rf "$WORK"; else echo "kept $WORK"; fi' EXIT
mkdir -p "$WORK/www/repo" "$WORK/client" "$WORK/dest" "$WORK/home"
REPO="$WORK/www/repo"
export ARMAXSYNC_HOME="$(winpath "$WORK/home")"
gen() { "$PY" -c "import sys,random; random.seed(int(sys.argv[2])); open(sys.argv[1],'wb').write(bytes(random.getrandbits(8) for _ in range(int(sys.argv[3]))))" "$1" "$2" "$3"; }
mkdir -p "$REPO/@ace/addons" "$REPO/@cba/addons" "$REPO/userconfig/ace"
gen "$REPO/@ace/addons/ace_main.pbo" 1 300000
gen "$REPO/@ace/addons/ace_big.pbo" 2 1500000
gen "$REPO/@cba/addons/cba.pbo" 3 50000
printf 'cfg' > "$REPO/userconfig/ace/settings.hpp"

legacy() { (cd "$WORK/client" && java -cp "$CP" --add-exports java.base/sun.reflect.generics.reflectiveObjects=ALL-UNNAMED -Djava.awt.headless=true fr.soe.a3s.main.ArmA3Sync "$@"); }
legacy_build() {
  rm -f "$WORK"/client/resources/ftp/*
  printf 'NEW\nJRepo\nHTTP\n%s\nanonymous\n\n127.0.0.1/repo\n%s\nBUILD\nJRepo\n3\n%s\nyes\n\n\nQUIT\n' "$PORT" "$(winpath "$REPO")" "$1" \
    | legacy -console > "$WORK/legacy-build.log" 2>&1 || true
  grep -q "build finished" "$WORK/legacy-build.log" || { tail -30 "$WORK/legacy-build.log"; echo "FAIL: legacy build"; exit 1; }
}
compare() {
  if diff -r -x .a3s -x .axs -x '*.zsync' -x '*.pbo.zip' "$REPO" "$WORK/dest" > "$WORK/diff.txt"; then echo "PASS: $1"; else cat "$WORK/diff.txt"; echo "FAIL: $1"; exit 1; fi
}

legacy_build no
test ! -d "$REPO/.axs" || { echo "FAIL: legacy build produced .axs"; exit 1; }
"$PY" "$(winpath "$HERE/rangeserver.py")" "$(winpath "$WORK/www")" "$PORT" 2>/dev/null & SRV=$!
sleep 1

"$AXS" repo add "http://127.0.0.1:$PORT/repo/.a3s/autoconfig" --dest "$(winpath "$WORK/dest")"
"$AXS" check --fail-on-update && { echo "FAIL: expected updates"; exit 1; } || true
"$AXS" sync JRepo --quiet
compare "Rust client: fresh sync from legacy-built repo"
"$AXS" check --fail-on-update
echo "PASS: status OK after sync"

"$PY" - "$REPO/@ace/addons/ace_big.pbo" <<'PYEOF'
import sys
p=sys.argv[1]; b=bytearray(open(p,'rb').read()); b[700000:700050]=b'Z'*50; open(p,'wb').write(bytes(b))
PYEOF
legacy_build no
OUT=$("$AXS" sync JRepo --json --quiet)
echo "$OUT" | grep -q '"method":"delta"' && echo "PASS: delta used against legacy .zsync" || { echo "$OUT" | head -20; echo "FAIL: no delta"; exit 1; }
compare "Rust client: delta sync from legacy-built repo"

JAVA_MAJOR=$(java -version 2>&1 | head -1 | sed -E 's/.*version "(1\.)?([0-9]+).*/\2/')
if [ "$JAVA_MAJOR" -gt 8 ]; then
  echo "SKIP: compressed legacy build needs Java 8 (legacy ZipDAO breaks on Java $JAVA_MAJOR)"
  "$AXS" verify JRepo && echo "PASS: verify"
  "$AXS" check-remote JRepo && echo "PASS: check-remote"
  echo "ALL PASS"; exit 0
fi
legacy_build yes
rm "$WORK/dest/@cba/addons/cba.pbo"
OUT=$("$AXS" sync JRepo --json --quiet)
echo "$OUT" | grep -q '"method":"zip"' && echo "PASS: legacy .pbo.zip used" || { echo "$OUT" | head; echo "FAIL: zip not used"; exit 1; }
compare "Rust client: compressed legacy repo"
"$AXS" verify JRepo && echo "PASS: verify"
"$AXS" check-remote JRepo && echo "PASS: check-remote"
echo "ALL PASS"
