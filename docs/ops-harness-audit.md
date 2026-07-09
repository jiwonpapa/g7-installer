# Server Operations Harness Audit

Date: 2026-07-08

## Current Status

- Local quick gate exists: `scripts/quick-gate.sh`.
- Full regression gate exists: `scripts/quality-gate.sh`.
- Quick gate covers shell syntax, web static smoke, setup auth smoke, JS syntax, `cargo fmt --check`, `cargo test -p g7-core --lib`, and `cargo test -p g7-cli --bin g7inst`.
- Full gate runs the quick gate first, then full `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo doc --no-deps`, `cargo llvm-cov --fail-under-lines 75`, and web CSS build. Set `G7_WEB_E2E=1` to also run the browser wizard E2E locally.
- Current measured line coverage is 77.94% in the latest local `scripts/quality-gate.sh` run.
- `web_setup/provision_actions.rs` line coverage is 39.48% after adding command-free provision action tests; privileged command-path coverage remains delegated to the disposable VPS harness.
- Release assets are Linux musl binaries for x86_64 and aarch64 plus `checksums.txt`.
- `scripts/ops-harness.sh` verifies a disposable Ubuntu 24.04 server through independently selectable steps: fresh doctor, plan, install, report contract, setup-guide capture, optional app smoke, reset dry-run, full installer reset, and doctor after reset. The reset path removes installer-created services, account, DB/user, packages, owned files, and metadata for a fresh reinstall attempt while preserving Let's Encrypt certificates to avoid duplicate issuance limits.
- Ops harness requires `G7_OPS_DOMAIN=<real-domain>` and defaults to `G7_OPS_CERTBOT_SCOPE=staging`, so current server validation follows the public-domain path. `G7_OPS_CERTBOT_SCOPE=skip` runs `--local-test` and is blocked unless `G7_OPS_ALLOW_LOCAL_TEST=1` is explicitly set for legacy local VM checks. Production Let's Encrypt requires both `G7_OPS_CERTBOT_SCOPE=production` and `G7_OPS_ALLOW_PRODUCTION_LE=1`.
- Ops harness defaults to `G7_OPS_STEPS=fresh-doctor,plan,install,report-contract,setup-guide,app-smoke,post-install-doctor,reset-dry-run,reset,fresh-doctor-after-reset`. Use a comma-separated subset to rerun only one surface, for example `G7_OPS_STEPS=fresh-doctor,plan`, `G7_OPS_STEPS=report-contract,app-smoke`, or `G7_OPS_STEPS=reset-dry-run,reset,fresh-doctor-after-reset`.
- GitHub Actions workflow is present at `.github/workflows/quality-gate.yml`.
- Browser-driven wizard E2E exists at `scripts/web-ui-e2e.spec.mjs` and covers route rendering, report downloads, plan auto-review, and provision cards against mocked controller APIs.

## Gaps Found

- `main` branch is not protected.
- The old `scripts/g7-test-smoke.sh` treated `g7inst doctor` exit status as install permission. `doctor` reports `install_allowed` in stdout, so the smoke check could produce a false result.
- Package rollback verification used `ssh` inside a file-fed loop. Without `ssh -n`, the first SSH call could consume the remaining package list from stdin.
- Browser-driven web UI E2E uses mocked controller APIs locally. Full root-capable install E2E remains covered by `scripts/ops-harness.sh` on a disposable Ubuntu VPS.
- CLI print-path coverage is still weaker than core command coverage.
- App provision action coverage is still newer than the core install coverage and should stay focused on safe, command-free checks plus mocked command paths.

## Improvements Applied

- Added `scripts/ops-harness.sh`.
- Reworked `scripts/g7-test-smoke.sh` as a wrapper over the ops harness.
- Added web controller API/session/error-path regression coverage.
- Added `scripts/web-static-smoke.sh` to catch wizard, theme, progress, reset, rollback, and localized error UI regressions.
- Added `scripts/setup-auth-smoke.sh` to block password-login regressions and assert non-root `g7inst setup` fails with sudo guidance.
- Added `.github/workflows/quality-gate.yml` to run Rust/static/web gates and Playwright wizard E2E in GitHub Actions.
- Added `scripts/web-ui-e2e.spec.mjs` to exercise route-based wizard screens, report download controls, and provision cards in a browser.
- Extended the web static smoke to reject password login form/API/server-account verifier regressions.
- Raised the default local line-coverage gate from 60% to 75%.
- Remote harness commands now run with `ssh -n` so package verification loops inspect every package.
- Shell compound checks that require sudo now run through `sudo sh -c` instead of relying on partial sudo command parsing.
- Ops harness now expects the full install phase to reach `completed`, captures `/var/log/g7-installer/setup-guide.md`, and uses `reset --yes` for full installer-created resource cleanup instead of unsafe package rollback after DB/certificate mutation.
- Ops harness checks reset output, verifies packages that were absent before install are absent again, verifies installer-owned paths are gone, and confirms `doctor` returns `install_allowed: true` after reset.
- Added `scripts/quick-gate.sh` so local development can avoid full coverage/doc/web-build cost on every small change.
- Full quality gate now runs the quick gate first.
- Ops harness separates Let's Encrypt scope, reset dry-run, and optional app smoke. Production LE issuance is opt-in only.
- `install.rs`, `web_setup.rs`, and `plan.rs` were physically split into focused files as a behavior-preserving first cut.
- Transitional `include!` splitting was replaced with real Rust submodules and narrower public boundaries.
- App install logic is now split under `commands/install/apps/` by app/runtime responsibility.
- Web controller and install tests were moved into `web_setup/tests.rs` and `install/tests.rs` so production module files stay smaller.
- Ops harness now supports `G7_OPS_STEPS` so staging LE, reset dry-run, fresh doctor, install report contract, and app smoke can run as separate disposable-VPS checks.
- Ops harness no longer falls back to `.local` or `--local-test` by default. Real-domain staging is the default proof path; legacy local VM smoke needs `G7_OPS_ALLOW_LOCAL_TEST=1` or `G7_SMOKE_ALLOW_LOCAL_TEST=1`.
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
  - optional second install cycle can run after reset; external VPS backup/snapshot restore is separate from installer rollback and may add cost/time

Example disposable VPS runs:

```bash
G7_OPS_CONFIRM_DISPOSABLE=1 G7_OPS_DOMAIN=example.com G7_OPS_STEPS=fresh-doctor,plan scripts/ops-harness.sh
G7_OPS_CONFIRM_DISPOSABLE=1 G7_OPS_DOMAIN=example.com G7_OPS_CERTBOT_SCOPE=staging G7_OPS_STEPS=install,report-contract scripts/ops-harness.sh
G7_OPS_CONFIRM_DISPOSABLE=1 G7_OPS_DOMAIN=example.com G7_OPS_APP_SMOKE=1 G7_OPS_STEPS=report-contract,app-smoke scripts/ops-harness.sh
G7_OPS_CONFIRM_DISPOSABLE=1 G7_OPS_DOMAIN=example.com G7_OPS_STEPS=reset-dry-run,reset,fresh-doctor-after-reset scripts/ops-harness.sh
```

## Required Manual Follow-Up

The following cannot be completed from the local workspace alone:

1. Enable branch protection for `main`.
2. Require the `quality-gate / local quality gate` check before merging or pushing to protected branches.
3. Keep `scripts/ops-harness.sh` as the disposable-server proof because local browser E2E intentionally mocks privileged server mutation APIs.

Suggested CI jobs:

- Rust gate: fmt, test, clippy, doc, llvm-cov line floor.
- Web gate: `bun install --frozen-lockfile` and `bun run build`.
- Release dry run: build x86_64 and aarch64 musl binaries and checksums.
