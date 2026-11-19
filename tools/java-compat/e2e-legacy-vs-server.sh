#!/usr/bin/env bash
# End-to-end: the legacy ArmA3Sync client syncs from armaxsync-server.
set -Eeuo pipefail
trap 'echo "FAIL: line $LINENO: $BASH_COMMAND (exit $?)" >&2' ERR
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi; }
SRVBIN="${AXS_SERVER_BIN:-$ROOT/target/debug/armaxsync-server}"
CP="$(cat "$HERE/build/legacy-app/classpath.txt")"
# Java 9+ needs the legacy client's internal-API access opened; Java 8 rejects the flag.
JAVA_EXTRA=""
# `sed -n 1p` reads all of java's output: `head -1` could SIGPIPE java, which
# fails the pipeline under `set -o pipefail`.
JAVA_VERSION="$(java -version 2>&1 | sed -n 1p)"
case "$JAVA_VERSION" in
  *'"1.'*) ;;  # Java 8
  *) JAVA_EXTRA="--add-exports java.base/sun.reflect.generics.reflectiveObjects=ALL-UNNAMED" ;;
esac
PORT="${PORT:-18736}"
PY=""; for c in python3 python; do if command -v "$c" >/dev/null 2>&1 && "$c" -c "import sys" >/dev/null 2>&1; then PY="$(command -v "$c")"; break; fi; done
WORK="$(mktemp -d)"; SRV=""
trap 'kill $SRV 2>/dev/null || true; if [ -z "${KEEP:-}" ]; then rm -rf "$WORK" 2>/dev/null || true; else echo "kept $WORK"; fi' EXIT
mkdir -p "$WORK/data" "$WORK/content" "$WORK/client" "$WORK/dest"
C="$WORK/content"
gen() { "$PY" -c "import sys,random; random.seed(int(sys.argv[2])); open(sys.argv[1],'wb').write(bytes(random.getrandbits(8) for _ in range(int(sys.argv[3]))))" "$1" "$2" "$3"; }
mkdir -p "$C/@ace/addons" "$C/@Mód ü/addons" "$C/userconfig"
gen "$C/@ace/addons/ace_big.pbo" 1 2000000
gen "$C/@Mód ü/addons/uni code.pbo" 2 9000
printf 'x' > "$C/userconfig/a.hpp"; : > "$C/empty.txt"

export ARMAXSYNC_DATA_DIR="$(winpath "$WORK/data")" ARMAXSYNC_BIND="127.0.0.1:$PORT" ARMAXSYNC_PUBLIC_URL="http://127.0.0.1:$PORT"
ARMAXSYNC_OWNER_PASSWORD=ownerpass123 "$SRVBIN" init --username owner >/dev/null
"$SRVBIN" > "$WORK/server.log" 2>&1 & SRV=$!
for i in $(seq 1 50); do curl -sf "http://127.0.0.1:$PORT/healthz" >/dev/null && break; sleep 0.2; done
B="http://127.0.0.1:$PORT/api/v1"
LOGIN=$(curl -s -D "$WORK/h.txt" -H 'content-type: application/json' -d '{"username":"owner","password":"ownerpass123"}' "$B/auth/login")
COOKIE=$(grep -i '^set-cookie' "$WORK/h.txt" | sed -E 's/^[^:]+: ([^;]+).*/\1/' | tr -d '\r')
CSRF=$(echo "$LOGIN" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['csrf'])")
TOKEN=$(curl -s -H "cookie: $COOKIE" -H "x-csrf-token: $CSRF" -H 'content-type: application/json' -d '{"name":"e2e","scopes":["admin"]}' "$B/tokens" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['token'])")
api() { curl -sf -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' "$@"; }
wait_job() { for i in $(seq 1 300); do S=$(api "$B/jobs/$1" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['status'])"); [ "$S" = succeeded ] && return 0; [ "$S" = failed ] && { api "$B/jobs/$1"; return 1; }; sleep 0.2; done; return 1; }
J=$(api -d "{\"slug\":\"e2e\",\"name\":\"E2E\",\"mode\":\"external\",\"path\":\"$(winpath "$C")\"}" "$B/repos" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['job'])")
wait_job "$J"

legacy() { (cd "$WORK/client" && java -cp "$CP" $JAVA_EXTRA -Djava.awt.headless=true fr.soe.a3s.main.ArmA3Sync "$@"); }
register() { rm -f "$WORK"/client/resources/ftp/*; printf 'NEW\nE2E\nHTTP\n%s\nanonymous\n\n127.0.0.1/r/e2e\n\nQUIT\n' "$PORT" | legacy -console > "$WORK/console.log" 2>&1 || true; }
compare() { if diff -r -x .a3s -x .axs -x '*.zsync' -x '*.pbo.zip' "$C" "$WORK/dest" > "$WORK/diff.txt"; then echo "PASS: $1"; else cat "$WORK/diff.txt"; echo "FAIL: $1"; exit 1; fi; }

register
legacy -sync E2E "$(winpath "$WORK/dest")" true > "$WORK/sync1.log" 2>&1
grep -qi "finished" "$WORK/sync1.log" && ! grep -qi "with errors" "$WORK/sync1.log" || { tail -30 "$WORK/sync1.log"; echo "FAIL: sync 1"; exit 1; }
compare "legacy client: fresh sync from armaxsync-server"

"$PY" - "$C/@ace/addons/ace_big.pbo" <<'PYEOF'
import sys
p=sys.argv[1]; b=bytearray(open(p,'rb').read()); b[900000:900100]=b'Q'*100; open(p,'wb').write(bytes(b))
PYEOF
J=$(api -X POST "$B/repos/e2e/build" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['job'])"); wait_job "$J"
register
legacy -sync E2E "$(winpath "$WORK/dest")" true > "$WORK/sync2.log" 2>&1
grep -qi "finished" "$WORK/sync2.log" && ! grep -qi "with errors" "$WORK/sync2.log" || { tail -30 "$WORK/sync2.log"; echo "FAIL: sync 2"; exit 1; }
compare "legacy client: delta sync from armaxsync-server"
echo "ALL PASS"
