# Server Operations Harness Audit

Date: 2026-07-07

## Current Status

- Local regression gate exists: `scripts/quality-gate.sh`.
- Local gate covers shell syntax, web static smoke, setup auth smoke, JS syntax, `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps`, `cargo llvm-cov --fail-under-lines 75`, and web CSS build.
- Current measured line coverage is 77.46%.
- Web controller line coverage is 76.63%.
- Release `v0.2.7` has both Linux musl assets and `checksums.txt`.
- `scripts/ops-harness.sh` now verifies a disposable Ubuntu 24.04 server through install, report validation, rollback, removed-package checks, and reinstall. Current install reaches the Nginx HTTP vhost phase.
- GitHub Actions workflow file exists at `.github/workflows/quality-gate.yml`; it runs the local quality gate on push to `main` and pull requests after it is pushed to GitHub.

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
- Added `.github/workflows/quality-gate.yml` to run the local quality gate in CI.
- Extended the web static smoke to reject password login form/API/server-account verifier regressions.
- Raised the default local line-coverage gate from 60% to 75%.
- Remote harness commands now run with `ssh -n` so package verification loops inspect every package.
- Shell compound checks that require sudo now run through `sudo sh -c` instead of relying on partial sudo command parsing.
- Ops harness checks:
  - disposable-server confirmation guard
  - Ubuntu 24.04 host check
  - release bootstrap or local binary install
  - expected `g7inst --version`
  - pre-install `doctor` must report `install_allowed: true`
  - local-test plan generation
  - install phase must finish as `vhost-enabled`
  - `/var/log/g7-installer/report.json` must pass JSON contract checks
  - post-install `doctor` must report `install_allowed: false`
  - `rollback --dry-run` must be available
  - `rollback --yes` must complete
  - packages absent before install must be absent after rollback
  - installer metadata files, Nginx vhost files, and installer-owned webroot smoke files must be removed after rollback
  - post-rollback `doctor` must report `install_allowed: true`
  - optional second install/rollback cycle validates repeatability

## Required Manual Follow-Up

The following cannot be completed from the local workspace alone:

1. Push `.github/workflows/quality-gate.yml` to GitHub so Actions activates.
2. Enable branch protection for `main`.
3. Require the `quality-gate / local quality gate` check before merging or pushing to protected branches.
4. Add browser-driven web UI E2E once the controller can be exercised safely in an isolated root-capable environment.

Suggested CI jobs:

- Rust gate: fmt, test, clippy, doc, llvm-cov line floor.
- Web gate: `bun install --frozen-lockfile` and `bun run build`.
- Release dry run: build x86_64 and aarch64 musl binaries and checksums.
