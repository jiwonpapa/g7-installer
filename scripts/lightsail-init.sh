#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" != "0" ]]; then
  echo "lightsail-init.sh must run as root. Lightsail launch scripts run as root by default." >&2
  exit 1
fi

export DEBIAN_FRONTEND=noninteractive

LOG_FILE="${G7_LIGHTSAIL_BOOTSTRAP_LOG:-/var/log/g7-lightsail-bootstrap.log}"
TIMEZONE="${G7_TIMEZONE:-Asia/Seoul}"
BOOTSTRAP_URL="${G7_BOOTSTRAP_URL:-https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.17/bootstrap.sh}"
BOOTSTRAP_SCRIPT="$(mktemp)"

exec > >(tee -a "${LOG_FILE}") 2>&1
trap 'rm -f "${BOOTSTRAP_SCRIPT}"' EXIT

echo "g7 Lightsail bootstrap started at $(date -Is)"

timedatectl set-timezone "${TIMEZONE}" || true

apt-get update
apt-get install -y \
  ca-certificates \
  curl

mkdir -p /opt/g7-bootstrap
cat >/opt/g7-bootstrap/README.txt <<'README'
This server was prepared for g7inst.
The launch script only installed minimal bootstrap dependencies and g7inst.
OS updates, security baseline, swap, firewall, fail2ban, web server, PHP,
database, Redis, Certbot, and app files should be installed by g7inst.
README

apt-get clean

curl -fsSL "${BOOTSTRAP_URL}" -o "${BOOTSTRAP_SCRIPT}"
bash "${BOOTSTRAP_SCRIPT}"
g7inst --version
g7inst doctor || true

echo "g7 Lightsail bootstrap completed at $(date -Is)"
