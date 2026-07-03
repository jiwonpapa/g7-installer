# G7 Installer

Rust based CLI installer for preparing a fresh Ubuntu 24.04 VPS for Gnuboard 7.

## Test Install From GitHub

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
g7 doctor
sudo g7 install --domain example.com
```

The bootstrap script downloads the latest GitHub Release binary, verifies
`checksums.txt`, and installs `g7` to `/usr/local/bin/g7`.

## Current MVP Commands

```bash
g7 doctor
g7 plan --domain example.com
sudo g7 install --domain example.com
g7 status
g7 logs
sudo g7 update
sudo g7 self-update
```

Implemented now:

- `doctor`: checks Ubuntu 24.04, root status, Nginx/Apache, ports 80/443, Nginx config, `/var/www/g7`, installer state, owned files, and Certbot live directory.
- `plan`: prints a dry-run install contract: gates, packages, files, services, ports, and stop conditions.
- `install`: MVP prepare phase only. It runs the same preflight gate, requires root, then writes installer state, owned-files metadata, config, log, and `/var/www/g7`.
- `logs`: prints installer log path.
- `status`: placeholder status.

Not implemented yet:

- apt package installation
- Nginx vhost rendering
- PHP-FPM/MariaDB provisioning
- G7 release download and extraction
- Certbot certificate issue
- update and self-update execution

## Release

Create and push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions builds:

- `g7-x86_64-unknown-linux-musl`
- `g7-aarch64-unknown-linux-musl`
- `checksums.txt`

For local smoke against the `g7-test` VM:

```bash
scripts/g7-test-smoke.sh
```

To reset the VM during smoke:

```bash
G7_SMOKE_RESET=1 scripts/g7-test-smoke.sh
```
