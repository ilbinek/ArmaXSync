#!/usr/bin/env bash
# Compiles the subset of the original Arma3Sync domain classes that make up the
# repository wire format, plus the ArmaXSync compatibility harness.
# Usage: tools/java-compat/build.sh [path-to-arma3sync-src]
set -Eeuo pipefail
trap 'echo "FAIL: line $LINENO: $BASH_COMMAND (exit $?)" >&2' ERR
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="${1:-${A3S_SRC:-$HERE/../../../arma3-sync/ArmA3Sync/src}}"
OUT="$HERE/build"
rm -rf "$OUT" && mkdir -p "$OUT/src" "$OUT/classes"
FILES=(
  fr/soe/a3s/domain/repository/AutoConfig.java
  fr/soe/a3s/domain/repository/Changelog.java
  fr/soe/a3s/domain/repository/Changelogs.java
  fr/soe/a3s/domain/repository/Event.java
  fr/soe/a3s/domain/repository/Events.java
  fr/soe/a3s/domain/repository/ServerInfo.java
  fr/soe/a3s/domain/repository/SyncTreeDirectory.java
  fr/soe/a3s/domain/repository/SyncTreeLeaf.java
  fr/soe/a3s/domain/repository/SyncTreeNode.java
  fr/soe/a3s/domain/repository/SyncTreeNodeMethods.java
  fr/soe/a3s/domain/AbstractProtocole.java
  fr/soe/a3s/domain/Http.java
  fr/soe/a3s/domain/Ftp.java
  fr/soe/a3s/domain/configration/FavoriteServer.java
  fr/soe/a3s/constant/ProtocolType.java
  fr/soe/a3s/constant/EncryptionMode.java
  fr/soe/a3s/constant/TimeOutValues.java
  fr/soe/a3s/exception/CheckException.java
  fr/soe/a3s/exception/ApplicationException.java
  fr/soe/a3s/jazsync/ChecksumPair.java
  fr/soe/a3s/jazsync/Configuration.java
  fr/soe/a3s/jazsync/Generator.java
  fr/soe/a3s/jazsync/JarsyncProvider.java
  fr/soe/a3s/jazsync/MD4.java
  fr/soe/a3s/jazsync/BrokenMD4.java
  fr/soe/a3s/jazsync/RollingChecksum.java
  fr/soe/a3s/jazsync/Rsum.java
  fr/soe/a3s/jazsync/Util.java
  fr/soe/a3s/jazsyncmake/MetaFileMaker.java
  fr/soe/a3s/jazsyncmake/HeaderMaker.java
)
for f in "${FILES[@]}"; do
  mkdir -p "$OUT/src/$(dirname "$f")"
  cp "$SRC/$f" "$OUT/src/$f"
done
# Git Bash on Windows: hand Windows-style paths to the JDK tools.
winpath() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else echo "$1"; fi; }
JAVAC_OPTS=(-encoding Cp1252 -nowarn -d "$(winpath "$OUT/classes")")
if javac --release 8 -version >/dev/null 2>&1; then JAVAC_OPTS+=(--release 8); fi
find "$OUT/src" "$HERE/src" -name '*.java' | while read -r f; do winpath "$f"; done > "$OUT/sources.txt"
javac "${JAVAC_OPTS[@]}" @"$(winpath "$OUT/sources.txt")" 2>&1 | grep -v '^Note:' || true
test -f "$OUT/classes/axs/Dump.class"
echo "java-compat built: $OUT/classes"
