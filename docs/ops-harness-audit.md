# Server Operations Harness Audit

Date: 2026-07-08

## Current Status

- Local regression gate exists: `scripts/quality-gate.sh`.
- Local gate covers shell syntax, web static smoke, setup auth smoke, JS syntax, `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps`, `cargo llvm-cov --fail-under-lines 75`, and web CSS build.
- Current measured line coverage is 79.30%.
- Web controller line coverage is 81.72%.
- Release assets are Linux musl binaries for x86_64 and aarch64 plus `checksums.txt`.
- `scripts/ops-harness.sh` verifies a disposable Ubuntu 24.04 server through install, report validation, setup-guide capture, and full installer reset. The reset path removes installer-created services, account, DB/user, packages, owned files, and metadata for a fresh reinstall attempt while preserving Let's Encrypt certificates to avoid duplicate issuance limits.
- GitHub Actions workflow is not present in this workspace yet.

## Gaps Found

- `main` branch is not protected.
- The old `scripts/g7-test-smoke.sh` treated `g7inst doctor` exit status as install permission. `doctor` reports `install_allowed` in stdout, so the smoke check could produce a false result.
- Package rollback verification used `ssh` inside a file-fed loop. Without `ssh -n`, the first SSH call could consume the remaining package list from stdin.
- No browser-driven web UI E2E harness exists yet.
- CLI print-path coverage is still weaker than core command coverage.

## Improvements Applied

- Added `scripts/ops-harness.sh`.
- Reworked `scripts/g7-test-smoke.sh` as a wrapper over the ops harness.
- Added web controller API/session/error-path regression coverage.
- Added `scripts/web-static-smoke.sh` to catch wizard, theme, progress, reset, rollback, and localized error UI regressions.
- Added `scripts/setup-auth-smoke.sh` to block password-login regressions and assert non-root `g7inst setup` fails with sudo guidance.
- Extended the web static smoke to reject password login form/API/server-account verifier regressions.
- Raised the default local line-coverage gate from 60% to 75%.
- Remote harness commands now run with `ssh -n` so package verification loops inspect every package.
- Shell compound checks that require sudo now run through `sudo sh -c` instead of relying on partial sudo command parsing.
- Ops harness now expects the full install phase to reach `completed`, captures `/var/log/g7-installer/setup-guide.md`, and uses `reset --yes` for full installer-created resource cleanup instead of unsafe package rollback after DB/certificate mutation.
- Ops harness checks reset output, verifies packages that were absent before install are absent again, verifies installer-owned paths are gone, and confirms `doctor` returns `install_allowed: true` after reset.
- Ops harness checks:
  - disposable-server confirmation guard
  - Ubuntu 24.04 host check
  - release bootstrap or local binary install
  - expected `g7inst --version`
  - pre-install `doctor` must report `install_allowed: true`
  - install plan generation
  - install phase must finish as `completed`
  - `/var/log/g7-installer/report.json` must pass JSON contract checks
  - `/var/log/g7-installer/setup-guide.md` must exist and be readable
  - post-install `doctor` must report `install_allowed: false`
  - optional `reset --yes` cleans installer-created resources and owned files
  - optional second install cycle can run after reset; external VPS snapshot restore remains the strongest safety net

## Required Manual Follow-Up

The following cannot be completed from the local workspace alone:

1. Add and push `.github/workflows/quality-gate.yml` so Actions activates.
2. Enable branch protection for `main`.
3. Require the `quality-gate / local quality gate` check before merging or pushing to protected branches.
4. Add browser-driven web UI E2E once the controller can be exercised safely in an isolated root-capable environment.

Suggested CI jobs:

- Rust gate: fmt, test, clippy, doc, llvm-cov line floor.
- Web gate: `bun install --frozen-lockfile` and `bun run build`.
- Release dry run: build x86_64 and aarch64 musl binaries and checksums.
