#!/usr/bin/env bash
# End-to-end: the legacy ArmA3Sync client syncs from repositories built by
# `armaxsync build`, served by a range-capable static HTTP server.
# Requires: ./build-legacy-app.sh, cargo build -p armaxsync, python3, java.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi; }
AXS="${AXS_BIN:-$ROOT/target/debug/armaxsync}"
CP="$(cat "$HERE/build/legacy-app/classpath.txt")"
PORT="${PORT:-18734}"
PY=""; for c in python3 python; do if command -v "$c" >/dev/null 2>&1 && "$c" -c "import sys" >/dev/null 2>&1; then PY="$(command -v "$c")"; break; fi; done
SRV=""
WORK="$(mktemp -d)"
trap 'kill $SRV 2>/dev/null || true; if [ -z "${KEEP:-}" ]; then rm -rf "$WORK"; else echo "kept $WORK"; fi' EXIT
mkdir -p "$WORK/www/repo" "$WORK/client" "$WORK/dest"
REPO="$WORK/www/repo"

gen() { "$PY" -c "import os,sys,random; random.seed(int(sys.argv[2])); open(sys.argv[1],'wb').write(bytes(random.getrandbits(8) for _ in range(int(sys.argv[3]))))" "$1" "$2" "$3"; }
mkdir -p "$REPO/@ace/addons" "$REPO/@cba/addons" "$REPO/@cba/keys" "$REPO/userconfig/ace" "$REPO/@Mód ü/addons"
gen "$REPO/@ace/addons/ace_main.pbo" 1 300000
gen "$REPO/@ace/addons/ace_big.pbo" 2 2000000
printf 'sig' > "$REPO/@ace/addons/ace_main.pbo.ace.bisign"
gen "$REPO/@cba/addons/cba.pbo" 3 50000
printf 'key' > "$REPO/@cba/keys/cba.bikey"
printf 'cfg' > "$REPO/userconfig/ace/settings.hpp"
gen "$REPO/@Mód ü/addons/uni code.pbo" 4 9000
: > "$REPO/empty.txt"

"$AXS" build "$(winpath "$REPO")" --name "E2E Repo" --url "http://127.0.0.1:$PORT/repo" --connections 2 --quiet
RANGESERVER_LOG="$WORK/requests1.log" "$PY" "$(winpath "$HERE/rangeserver.py")" "$(winpath "$WORK/www")" "$PORT" 2>/dev/null &
SRV=$!
sleep 1

legacy() { (cd "$WORK/client" && java -cp "$CP" --add-exports java.base/sun.reflect.generics.reflectiveObjects=ALL-UNNAMED -Djava.awt.headless=true fr.soe.a3s.main.ArmA3Sync "$@"); }

# Create the repository in the legacy client through its console.
printf 'NEW\nE2E\nHTTP\n%s\nanonymous\n\n127.0.0.1/repo\n\nQUIT\n' "$PORT" | legacy -console > "$WORK/console.log" 2>&1 || true
grep -q "Repository creation finished" "$WORK/console.log" || { cat "$WORK/console.log"; echo "FAIL: legacy repo creation"; exit 1; }

compare() {
  local label="$1"
  if diff -r -x .a3s -x .axs -x '*.zsync' -x '*.pbo.zip' "$REPO" "$WORK/dest" > "$WORK/diff.txt"; then
    echo "PASS: $label"
  else
    cat "$WORK/diff.txt"; echo "FAIL: $label"; exit 1
  fi
}

legacy -sync E2E "$(winpath "$WORK/dest")" true > "$WORK/sync1.log" 2>&1
grep -qi "finished" "$WORK/sync1.log" || { tail -40 "$WORK/sync1.log"; echo "FAIL: sync 1"; exit 1; }
compare "fresh sync from Rust-built repo"

# Delta: modify part of a large file, delete one, add one, rebuild, resync.
"$PY" - "$REPO/@ace/addons/ace_big.pbo" <<'PYEOF'
import sys
p=sys.argv[1]; b=bytearray(open(p,'rb').read())
b[1000000:1000100]=b'X'*100
b[50000:50000]=b'inserted bytes'*10
open(p,'wb').write(bytes(b))
PYEOF
rm "$REPO/@cba/keys/cba.bikey"
gen "$REPO/@cba/addons/cba_new.pbo" 5 12345
export RANGESERVER_LOG="$WORK/requests.log"
kill $SRV; "$PY" "$(winpath "$HERE/rangeserver.py")" "$(winpath "$WORK/www")" "$PORT" 2>/dev/null & SRV=$!; sleep 1
"$AXS" build "$(winpath "$REPO")" --quiet
# The legacy client sometimes corrupts its own encrypted repository file
# (it serializes its SHA-1 cache while worker threads still mutate it), so
# register the repository again; the destination keeps its old files.
rm -f "$WORK"/client/resources/ftp/*
printf 'NEW
E2E
HTTP
%s
anonymous

127.0.0.1/repo

QUIT
' "$PORT" | legacy -console > "$WORK/console2.log" 2>&1 || true
legacy -sync E2E "$(winpath "$WORK/dest")" true > "$WORK/sync2.log" 2>&1
grep -qi "finished" "$WORK/sync2.log" || { tail -40 "$WORK/sync2.log"; echo "FAIL: sync 2"; exit 1; }
compare "delta sync after rebuild"
if grep -q "Range" "$WORK/requests.log" 2>/dev/null || grep -q " 206 " "$WORK/requests.log"; then
  echo "INFO: partial (206) requests used: $(grep -c ' 206 ' "$WORK/requests.log")"
fi
# Compressed mode: clients download .pbo.zip and unpack it.
"$AXS" build "$(winpath "$REPO")" --compress --save --quiet
rm -f "$WORK/dest/@cba/addons/cba.pbo" "$WORK/dest/@ace/addons/ace_main.pbo"
rm -f "$WORK"/client/resources/ftp/*
printf 'NEW
E2E
HTTP
%s
anonymous

127.0.0.1/repo

QUIT
' "$PORT" | legacy -console > "$WORK/console3.log" 2>&1 || true
legacy -sync E2E "$(winpath "$WORK/dest")" true > "$WORK/sync3.log" 2>&1
grep -qi "finished" "$WORK/sync3.log" || { tail -40 "$WORK/sync3.log"; echo "FAIL: sync 3"; exit 1; }
compare "compressed (.pbo.zip) sync"
grep -q "cba.pbo.zip" "$WORK/requests.log" && echo "INFO: .pbo.zip downloaded"
echo "ALL PASS"
