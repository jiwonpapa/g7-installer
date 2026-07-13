#!/usr/bin/env python3
"""Enforce coverage floors on installer modules where regressions are costly."""

import json
import sys
from pathlib import Path


FLOORS = {
    "crates/g7-core/src/commands/install/orchestrator.rs": 64.0,
    "crates/g7-core/src/commands/install/packages.rs": 57.0,
    "crates/g7-core/src/commands/install/site.rs": 62.0,
    "crates/g7-core/src/commands/install/transaction.rs": 72.0,
    "crates/g7-core/src/commands/reset.rs": 87.0,
    "crates/g7-core/src/commands/rollback.rs": 80.0,
    "crates/g7-cli/src/web_setup/api.rs": 64.0,
    "crates/g7-cli/src/web_setup/provision_actions.rs": 62.0,
    "crates/g7-cli/src/web_setup/routes.rs": 63.0,
}


def line_percent(file_entry: dict) -> float:
    lines = file_entry.get("summary", {}).get("lines", {})
    if "percent" in lines:
        return float(lines["percent"])
    count = int(lines.get("count", 0))
    covered = int(lines.get("covered", 0))
    return 100.0 if count == 0 else covered * 100.0 / count


def check_report(report: dict, root: Path) -> list[str]:
    files = report.get("data", [{}])[0].get("files", [])
    measured = {}
    for entry in files:
        filename = Path(entry.get("filename", ""))
        try:
            relative = filename.resolve().relative_to(root.resolve()).as_posix()
        except ValueError:
            relative = filename.as_posix()
        measured[relative] = line_percent(entry)

    failures = []
    for path, floor in FLOORS.items():
        if path not in measured:
            failures.append(f"coverage result missing critical module: {path}")
        elif measured[path] + 1e-9 < floor:
            failures.append(f"{path}: {measured[path]:.2f}% < {floor:.2f}%")
        else:
            print(f"[coverage-ratchet] {path}: {measured[path]:.2f}% >= {floor:.2f}%")
    return failures


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: check-coverage-ratchet.py <llvm-cov.json> <repo-root>", file=sys.stderr)
        return 2
    with open(sys.argv[1], "r", encoding="utf-8") as handle:
        report = json.load(handle)
    totals = report.get("data", [{}])[0].get("totals", {}).get("lines", {})
    if totals:
        print(f"[coverage-ratchet] total lines: {float(totals.get('percent', 0)):.2f}%")
    failures = check_report(report, Path(sys.argv[2]))
    if failures:
        print("\n".join(f"[coverage-ratchet] FAIL: {item}" for item in failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
