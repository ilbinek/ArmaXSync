#!/bin/sh
# ArmaXSync server update helper — installed as /usr/lib/armaxsync/update-helper
# and run as root by armaxsync-server-update.service when the server stages an
# update (<update dir>/request, written by the "Update" button in the admin UI).
#
# The staged binary was written by the unprivileged service account, so it is
# copied somewhere root-owned first and then verified there by the *installed*
# binary: valid release signature, and newer than what is installed. After the
# swap the service must come back healthy, otherwise the previous binary is
# restored.
set -eu

UPDATE_DIR="${ARMAXSYNC_UPDATE_DIR:-/var/lib/armaxsync/update}"
CONFIG="${ARMAXSYNC_CONFIG:-/etc/armaxsync/config.toml}"
BIN="${ARMAXSYNC_BIN:-/usr/bin/armaxsync-server}"
SERVICE="${ARMAXSYNC_SERVICE:-armaxsync-server.service}"
WORK="${ARMAXSYNC_UPDATE_WORK:-/usr/lib/armaxsync/staging}"

REQ="$UPDATE_DIR/request"
NEW="$UPDATE_DIR/armaxsync-server.new"
RESULT="$UPDATE_DIR/last-result.json"

[ -f "$REQ" ] || exit 0

version=$(sed -n 's/.*"version":"\([^"]*\)".*/\1/p' "$REQ")
rm -f "$REQ"

result() { # status message
  msg=$(printf '%s' "$2" | tr -d '"\\' | tr '\n' ' ')
  printf '{"status":"%s","version":"%s","message":"%s","at":%s000}\n' \
    "$1" "$version" "$msg" "$(date +%s)" > "$RESULT.tmp"
  chmod 644 "$RESULT.tmp"
  mv -f "$RESULT.tmp" "$RESULT"
  echo "update $version: $1: $2"
}

fail() {
  result failed "$1"
  rm -f "$NEW" "$NEW.sig"
  rm -rf "$WORK"
  exit 1
}

case "$version" in
  ''|*[!0-9A-Za-z.+-]*) fail "invalid version in update request" ;;
esac
[ -f "$NEW" ] && [ -f "$NEW.sig" ] || fail "staged files are missing"

# Root-owned copy; the service account cannot touch it after verification.
rm -rf "$WORK"
(umask 077 && mkdir -p "$WORK")
cp "$NEW" "$WORK/armaxsync-server"
cp "$NEW.sig" "$WORK/armaxsync-server.sig"
rm -f "$NEW" "$NEW.sig"

out=$("$BIN" verify-update --signature "$WORK/armaxsync-server.sig" --version "$version" "$WORK/armaxsync-server" 2>&1) \
  || fail "verification failed: $out"
chmod 755 "$WORK/armaxsync-server"
reported=$("$WORK/armaxsync-server" --version 2>/dev/null | awk '{print $2}')
[ "$reported" = "$version" ] || fail "binary reports version '$reported', expected $version"

previous=$("$BIN" --version | awk '{print $2}')
cp -p "$BIN" "$BIN.prev"
install -m 755 "$WORK/armaxsync-server" "$BIN.new"
mv -f "$BIN.new" "$BIN"
rm -rf "$WORK"

systemctl restart "$SERVICE" || true

healthy() {
  systemctl is-active --quiet "$SERVICE" \
    && "$BIN" --config "$CONFIG" healthcheck 2>/dev/null | grep -q "^ok $1\$"
}

i=0
while [ $i -lt "${ARMAXSYNC_UPDATE_TRIES:-30}" ]; do
  sleep "${ARMAXSYNC_UPDATE_WAIT:-2}"
  if healthy "$version"; then
    result installed "updated from $previous to $version"
    exit 0
  fi
  i=$((i + 1))
done

# Did not come up: roll back.
mv -f "$BIN.prev" "$BIN"
systemctl restart "$SERVICE" || true
sleep 3
if healthy "$previous"; then
  result rolled_back "$version did not start; restored $previous (journalctl -u $SERVICE)"
else
  result failed "$version did not start and $previous did not come back either (journalctl -u $SERVICE)"
fi
exit 1
