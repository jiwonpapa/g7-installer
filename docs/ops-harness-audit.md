# Server Operations Harness Audit

Date: 2026-07-06

## Current Status

- Local regression gate exists: `scripts/quality-gate.sh`.
- Local gate covers `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps`, `cargo llvm-cov --fail-under-lines 60`, and web CSS build.
- Current measured line coverage is 65.19%.
- Release `v0.2.7` has both Linux musl assets and `checksums.txt`.
- `scripts/ops-harness.sh` now verifies a disposable Ubuntu 24.04 server through install, report validation, rollback, removed-package checks, and reinstall.

## Gaps Found

- No active GitHub Actions workflow is present in `.github/workflows`.
- `main` branch is not protected.
- Previous workflow push was rejected because the current GitHub token lacks `workflow` scope.
- The old `scripts/g7-test-smoke.sh` treated `g7inst doctor` exit status as install permission. `doctor` reports `install_allowed` in stdout, so the smoke check could produce a false result.
- Package rollback verification used `ssh` inside a file-fed loop. Without `ssh -n`, the first SSH call could consume the remaining package list from stdin.
- Web controller coverage is weak compared with core command coverage.
- No browser-driven web UI E2E harness exists yet.

## Improvements Applied

- Added `scripts/ops-harness.sh`.
- Reworked `scripts/g7-test-smoke.sh` as a wrapper over the ops harness.
- Remote harness commands now run with `ssh -n` so package verification loops inspect every package.
- Shell compound checks that require sudo now run through `sudo sh -c` instead of relying on partial sudo command parsing.
- Ops harness checks:
  - disposable-server confirmation guard
  - Ubuntu 24.04 host check
  - release bootstrap or local binary install
  - expected `g7inst --version`
  - pre-install `doctor` must report `install_allowed: true`
  - local-test plan generation
  - package install phase must finish as `packages-installed`
  - `/var/log/g7-installer/report.json` must pass JSON contract checks
  - post-install `doctor` must report `install_allowed: false`
  - `rollback --dry-run` must be available
  - `rollback --yes` must complete
  - packages absent before install must be absent after rollback
  - installer metadata files and installer-owned directories must be removed after rollback
  - post-rollback `doctor` must report `install_allowed: true`
  - optional second install/rollback cycle validates repeatability

## Required Manual Follow-Up

The following cannot be completed with the current GitHub token:

1. Re-authenticate GitHub CLI with `workflow` scope.
2. Add an active workflow under `.github/workflows`.
3. Enable branch protection for `main`.
4. Require the workflow checks before merging or pushing to protected branches.

Suggested CI jobs:

- Rust gate: fmt, test, clippy, doc, llvm-cov line floor.
- Web gate: `bun install --frozen-lockfile` and `bun run build`.
- Release dry run: build x86_64 and aarch64 musl binaries and checksums.
