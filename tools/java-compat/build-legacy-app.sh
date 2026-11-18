#!/usr/bin/env bash
# Builds the complete legacy ArmA3Sync client from source (for end-to-end
# tests on JDK 8 or newer). Output: tools/java-compat/build/legacy-app/{classes,lib}
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="${1:-${A3S_APP:-$HERE/../../../arma3-sync/ArmA3Sync}}"
OUT="$HERE/build/legacy-app"
winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi; }
SEP=":"; command -v cygpath >/dev/null 2>&1 && SEP=";"
rm -rf "$OUT" && mkdir -p "$OUT/classes" "$OUT/lib"
cp "$APP"/resources/lib/*.jar "$OUT/lib/"
CP=""
for j in "$OUT"/lib/*.jar; do CP="$CP$(winpath "$j")$SEP"; done
find "$APP/src" "$HERE/shims" -name '*.java' | while read -r f; do winpath "$f"; done > "$OUT/sources.txt"
OPTS=(-encoding Cp1252 -nowarn -d "$(winpath "$OUT/classes")" -cp "$CP")
VER=$(javac -version 2>&1 | sed -E 's/javac ([0-9]+).*/\1/')
if [ "$VER" != "1" ] && [ "${VER:-8}" -ge 9 ] 2>/dev/null; then
  OPTS+=(--add-exports java.base/sun.reflect.generics.reflectiveObjects=ALL-UNNAMED)
fi
javac "${OPTS[@]}" @"$(winpath "$OUT/sources.txt")" 2>&1 | grep -vE '^Note:|warning' || true
cp -r "$APP/src/resources" "$(winpath "$OUT/classes")/" 2>/dev/null || true
test -f "$OUT/classes/fr/soe/a3s/main/ArmA3Sync.class"
echo "$CP$(winpath "$OUT/classes")" > "$OUT/classpath.txt"
echo "legacy app built: $OUT"
