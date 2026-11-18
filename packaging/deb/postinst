#!/bin/sh
set -e
if ! getent passwd armaxsync >/dev/null; then
  useradd --system --home-dir /var/lib/armaxsync --create-home --shell /usr/sbin/nologin armaxsync
fi
mkdir -p /var/lib/armaxsync /etc/armaxsync
chown armaxsync:armaxsync /var/lib/armaxsync
if [ ! -f /etc/armaxsync/config.toml ]; then
  /usr/bin/armaxsync-server example-config | sed 's#^data_dir = .*#data_dir = "/var/lib/armaxsync"#' > /etc/armaxsync/config.toml
fi
systemctl daemon-reload >/dev/null 2>&1 || true
echo "armaxsync-server installed: edit /etc/armaxsync/config.toml, then: systemctl enable --now armaxsync-server"
