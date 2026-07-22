#!/usr/bin/env python3
"""Keep local development fast by ratcheting structural growth."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


SOURCE_EXTENSIONS = {".css", ".html", ".js", ".md", ".mjs", ".py", ".rs", ".sh"}
IGNORED_PARTS = {".git", "target", "dist", "node_modules"}
NEW_LARGE_FILE_LIMIT = 900
BASELINE_GROWTH_ALLOWANCE = 80

LARGE_FILE_BASELINE = {
    "web/app.js": 4985,
    "web/input.css": 2920,
    "crates/g7-core/src/commands/install/tests.rs": 2708,
    "crates/g7-core/src/commands/install/orchestrator.rs": 2070,
    "crates/g7-core/src/commands/reset.rs": 2093,
    "crates/g7-core/src/commands/finalize.rs": 1620,
    "crates/g7-core/src/commands/install/runtime.rs": 1408,
    "crates/g7-core/src/commands/rollback.rs": 1194,
    "crates/g7-cli/src/web_setup/api.rs": 1193,
    "crates/g7-cli/src/web_setup/tests.rs": 1181,
    "crates/g7-core/src/commands/install/report.rs": 980,
    "scripts/web-ui-e2e.spec.mjs": 977,
    "crates/g7-cli/src/web_setup/provision_actions.rs": 907,
}

SHELL_EXCEPTION_BASELINE = {
    "crates/g7-system/src/command.rs": 1,
}

LIVE_FIXTURE_PATTERNS = [
    re.compile(r"g7devops\.com"),
    re.compile(r"/home/g7devops\b"),
]
LIVE_FIXTURE_ALLOWED = {
    "crates/g7-core/src/commands/reset.rs",
    "docs/ops-harness-audit.md",
    "docs/promo-manifest.md",
    "scripts/structure-audit.py",
    "scripts/tests/test_structure_audit.py",
    "scripts/web-ui-e2e.spec.mjs",
    "web/promo.sample.json",
}


def relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def source_files(root: Path) -> list[Path]:
    files = []
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        if any(part in IGNORED_PARTS for part in path.relative_to(root).parts):
            continue
        if path.suffix in SOURCE_EXTENSIONS:
            files.append(path)
    return files


def line_count(path: Path) -> int:
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        return sum(1 for _ in handle)


def check_large_files(root: Path, files: list[Path]) -> list[str]:
    failures = []
    for path in files:
        name = relative(path, root)
        lines = line_count(path)
        baseline = LARGE_FILE_BASELINE.get(name)
        if baseline is None and lines > NEW_LARGE_FILE_LIMIT:
            failures.append(
                f"new large file exceeds {NEW_LARGE_FILE_LIMIT} lines: {name} ({lines})"
            )
            continue
        if baseline is not None and lines > baseline + BASELINE_GROWTH_ALLOWANCE:
            failures.append(
                f"large file grew beyond ratchet: {name} ({lines} > {baseline}+{BASELINE_GROWTH_ALLOWANCE})"
            )
    return failures


def check_shell_exceptions(root: Path, files: list[Path]) -> list[str]:
    failures = []
    pattern = re.compile(r'(CommandSpec|Command)::new\("sh"\)')
    for path in files:
        if path.suffix != ".rs":
            continue
        name = relative(path, root)
        count = len(pattern.findall(path.read_text(encoding="utf-8", errors="ignore")))
        allowed = SHELL_EXCEPTION_BASELINE.get(name, 0)
        if count > allowed:
            failures.append(f"new shell command construction is not allowed: {name} ({count} > {allowed})")
    return failures


def check_live_fixtures(root: Path, files: list[Path]) -> list[str]:
    failures = []
    for path in files:
        name = relative(path, root)
        if name in LIVE_FIXTURE_ALLOWED:
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        for pattern in LIVE_FIXTURE_PATTERNS:
            if pattern.search(text):
                failures.append(f"live fixture leaked outside allowed files: {name} ({pattern.pattern})")
                break
    return failures


def check_build_artifacts(root: Path) -> list[str]:
    failures = []
    for name in ("target", "dist"):
        path = root / name
        if path.exists():
            failures.append(f"repo-local build artifact directory is present: {name}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument(
        "--fail-on-build-artifacts",
        action="store_true",
        help="fail when repo-local target/ or dist/ exists",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    files = source_files(root)
    failures = []
    failures.extend(check_large_files(root, files))
    failures.extend(check_shell_exceptions(root, files))
    failures.extend(check_live_fixtures(root, files))
    if args.fail_on_build_artifacts:
        failures.extend(check_build_artifacts(root))

    if failures:
        for failure in failures:
            print(f"[structure-audit] FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "[structure-audit] ok: large-file ratchet, shell exceptions, and live fixture boundaries"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
