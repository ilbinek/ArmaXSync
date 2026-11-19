#!/bin/sh
set -e
if ! getent passwd armaxsync >/dev/null; then
  useradd --system --home-dir /var/lib/armaxsync --create-home --shell /usr/sbin/nologin armaxsync
fi
mkdir -p /var/lib/armaxsync /var/lib/armaxsync/update /etc/armaxsync
chown armaxsync:armaxsync /var/lib/armaxsync /var/lib/armaxsync/update
if [ ! -f /etc/armaxsync/config.toml ]; then
  /usr/bin/armaxsync-server example-config | sed 's#^data_dir = .*#data_dir = "/var/lib/armaxsync"#' > /etc/armaxsync/config.toml
fi
if [ -d /run/systemd/system ]; then
  systemctl daemon-reload >/dev/null 2>&1 || true
  # One-click updates from the admin UI.
  systemctl enable --now armaxsync-server-update.path >/dev/null 2>&1 || true
  # Package upgrade: restart a running server on the new binary.
  if systemctl is-active --quiet armaxsync-server.service; then
    systemctl restart armaxsync-server.service || true
  fi
fi
echo "armaxsync-server installed: edit /etc/armaxsync/config.toml, then: systemctl enable --now armaxsync-server"
