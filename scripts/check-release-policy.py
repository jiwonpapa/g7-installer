#!/usr/bin/env python3
"""Validate Keep a Changelog and Semantic Versioning release policy."""

from __future__ import annotations

import datetime as dt
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REPO_URL = "https://github.com/jiwonpapa/g7-installer"
CHANGELOG = ROOT / "CHANGELOG.md"
CRATES_DIR = ROOT / "crates"
ALLOWED_CHANGE_TYPES = {"Added", "Changed", "Deprecated", "Removed", "Fixed", "Security"}
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)\."
    r"(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)
VERSION_LINE_RE = re.compile(r'^version = "([^"]+)"$')
INTERNAL_DEP_RE = re.compile(r'^g7-[\w-]+ = \{ path = "[^"]+", version = "=([^"]+)"')
RELEASE_HEADING_RE = re.compile(r"^## \[([^\]]+)\] - (\d{4}-\d{2}-\d{2})$")
UNLINKED_RELEASE_RE = re.compile(r"^## (0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)")
SECTION_RE = re.compile(r"^### ([A-Za-z]+)$")
LINK_RE = re.compile(r"^\[([^\]]+)\]: (.+)$")


@dataclass(frozen=True)
class Release:
    version: str
    date: dt.date
    line_number: int
    sections: tuple[str, ...]


def fail(message: str) -> str:
    return f"[release-policy] FAIL: {message}"


def is_semver(version: str) -> bool:
    return SEMVER_RE.match(version) is not None


def parse_semver(version: str) -> tuple[tuple[int, int, int], list[str] | None]:
    match = SEMVER_RE.match(version)
    if not match:
        raise ValueError(f"invalid SemVer: {version}")
    major, minor, patch, prerelease, _build = match.groups()
    return (int(major), int(minor), int(patch)), prerelease.split(".") if prerelease else None


def compare_prerelease(left: list[str] | None, right: list[str] | None) -> int:
    if left is None and right is None:
        return 0
    if left is None:
        return 1
    if right is None:
        return -1
    for left_item, right_item in zip(left, right, strict=False):
        left_numeric = left_item.isdigit()
        right_numeric = right_item.isdigit()
        if left_numeric and right_numeric:
            left_value = int(left_item)
            right_value = int(right_item)
            if left_value != right_value:
                return 1 if left_value > right_value else -1
            continue
        if left_numeric != right_numeric:
            return -1 if left_numeric else 1
        if left_item != right_item:
            return 1 if left_item > right_item else -1
    if len(left) == len(right):
        return 0
    return 1 if len(left) > len(right) else -1


def compare_semver(left: str, right: str) -> int:
    left_base, left_pre = parse_semver(left)
    right_base, right_pre = parse_semver(right)
    if left_base != right_base:
        return 1 if left_base > right_base else -1
    return compare_prerelease(left_pre, right_pre)


def package_versions(root: Path = ROOT) -> dict[str, str]:
    versions: dict[str, str] = {}
    for cargo_toml in sorted((root / "crates").glob("*/Cargo.toml")):
        package_name = cargo_toml.parent.name
        for line in cargo_toml.read_text(encoding="utf-8").splitlines():
            match = VERSION_LINE_RE.match(line)
            if match:
                versions[package_name] = match.group(1)
                break
        else:
            versions[package_name] = ""
    return versions


def internal_dependency_versions(root: Path = ROOT) -> dict[str, list[str]]:
    dependencies: dict[str, list[str]] = {}
    for cargo_toml in sorted((root / "crates").glob("*/Cargo.toml")):
        versions = [
            match.group(1)
            for line in cargo_toml.read_text(encoding="utf-8").splitlines()
            if (match := INTERNAL_DEP_RE.match(line))
        ]
        dependencies[cargo_toml.parent.name] = versions
    return dependencies


def check_workspace_versions(root: Path = ROOT) -> list[str]:
    failures: list[str] = []
    versions = package_versions(root)
    if not versions:
        return [fail("no crate package versions found")]
    unique_versions = sorted(set(versions.values()))
    if len(unique_versions) != 1:
        failures.append(fail(f"crate versions must match: {versions}"))
    for package, version in versions.items():
        if not is_semver(version):
            failures.append(fail(f"{package} version is not SemVer: {version}"))
    expected = versions.get("g7-cli") or unique_versions[0]
    for package, dependency_versions in internal_dependency_versions(root).items():
        for dependency_version in dependency_versions:
            if dependency_version != expected:
                failures.append(
                    fail(
                        f"{package} internal dependency pins {dependency_version}, expected {expected}"
                    )
                )
    return failures


def parse_links(lines: list[str]) -> dict[str, str]:
    links: dict[str, str] = {}
    for line in lines:
        match = LINK_RE.match(line)
        if match:
            links[match.group(1)] = match.group(2)
    return links


def parse_releases(lines: list[str]) -> tuple[list[Release], list[str]]:
    failures: list[str] = []
    heading_positions: list[tuple[int, str, dt.date]] = []
    for index, line in enumerate(lines):
        if UNLINKED_RELEASE_RE.match(line):
            failures.append(fail(f"release heading must be linked at line {index + 1}: {line}"))
        match = RELEASE_HEADING_RE.match(line)
        if not match:
            continue
        version = match.group(1)
        date_text = match.group(2)
        if not is_semver(version):
            failures.append(fail(f"release heading is not SemVer at line {index + 1}: {version}"))
        try:
            release_date = dt.date.fromisoformat(date_text)
        except ValueError:
            failures.append(fail(f"release date must be ISO 8601 at line {index + 1}: {date_text}"))
            release_date = dt.date.min
        heading_positions.append((index, version, release_date))

    releases: list[Release] = []
    for position, (index, version, release_date) in enumerate(heading_positions):
        next_index = heading_positions[position + 1][0] if position + 1 < len(heading_positions) else len(lines)
        sections: list[str] = []
        for body_line in lines[index + 1 : next_index]:
            match = SECTION_RE.match(body_line)
            if match:
                section = match.group(1)
                if section not in ALLOWED_CHANGE_TYPES:
                    failures.append(
                        fail(f"unsupported changelog section under {version}: {section}")
                    )
                sections.append(section)
        if not sections:
            failures.append(fail(f"release {version} needs at least one change type section"))
        releases.append(Release(version, release_date, index + 1, tuple(sections)))
    return releases, failures


def check_changelog(root: Path = ROOT) -> list[str]:
    path = root / "CHANGELOG.md"
    lines = path.read_text(encoding="utf-8").splitlines()
    failures: list[str] = []
    if not lines or lines[0] != "# Changelog":
        failures.append(fail("CHANGELOG.md must start with '# Changelog'"))
    if "## [Unreleased]" not in lines:
        failures.append(fail("CHANGELOG.md must contain '## [Unreleased]'"))
    intro = "\n".join(lines[:8])
    if "https://keepachangelog.com/ko/1.1.0/" not in intro:
        failures.append(fail("CHANGELOG.md must reference Keep a Changelog 1.1.0"))
    if "https://semver.org/lang/ko/" not in intro:
        failures.append(fail("CHANGELOG.md must reference Semantic Versioning"))

    releases, parse_failures = parse_releases(lines)
    failures.extend(parse_failures)
    if not releases:
        return failures + [fail("CHANGELOG.md must contain at least one release")]

    cli_version = package_versions(root).get("g7-cli")
    if cli_version and releases[0].version != cli_version:
        failures.append(
            fail(f"latest changelog release {releases[0].version} must match g7-cli {cli_version}")
        )

    for previous, current in zip(releases, releases[1:], strict=False):
        if previous.date < current.date:
            failures.append(
                fail(
                    f"changelog dates must be newest first: {previous.version} before {current.version}"
                )
            )
        if compare_semver(previous.version, current.version) < 0:
            failures.append(
                fail(
                    f"changelog versions must be newest first: {previous.version} before {current.version}"
                )
            )

    links = parse_links(lines)
    expected_links = {"Unreleased": f"{REPO_URL}/compare/v{releases[0].version}...HEAD"}
    for index, release in enumerate(releases):
        if index + 1 < len(releases):
            previous_version = releases[index + 1].version
            expected_links[release.version] = (
                f"{REPO_URL}/compare/v{previous_version}...v{release.version}"
            )
        else:
            expected_links[release.version] = f"{REPO_URL}/releases/tag/v{release.version}"
    for label, expected_url in expected_links.items():
        actual_url = links.get(label)
        if actual_url != expected_url:
            failures.append(
                fail(f"link [{label}] must be {expected_url}, got {actual_url or 'missing'}")
            )
    return failures


def main() -> int:
    failures = []
    failures.extend(check_workspace_versions(ROOT))
    failures.extend(check_changelog(ROOT))
    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1
    print("[release-policy] ok: Keep a Changelog and SemVer policy")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
