import tempfile
import sys
import unittest
from pathlib import Path

import importlib.util


SCRIPT = Path(__file__).parents[1] / "check-release-policy.py"
SPEC = importlib.util.spec_from_file_location("release_policy", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class ReleasePolicyTests(unittest.TestCase):
    def test_semver_rejects_v_prefix_and_accepts_prerelease(self):
        self.assertTrue(MODULE.is_semver("0.3.0-beta.17"))
        self.assertTrue(MODULE.is_semver("1.0.0+build.1"))
        self.assertFalse(MODULE.is_semver("v0.3.0"))

    def test_semver_order_handles_numeric_prerelease_identifiers(self):
        self.assertGreater(MODULE.compare_semver("0.3.0-beta.17", "0.3.0-beta.9"), 0)
        self.assertGreater(MODULE.compare_semver("0.3.0", "0.3.0-beta.17"), 0)
        self.assertGreater(MODULE.compare_semver("0.3.0-beta.1", "0.2.47"), 0)

    def test_release_without_change_type_fails(self):
        lines = [
            "# Changelog",
            "",
            "## [Unreleased]",
            "",
            "## [0.1.0] - 2026-01-01",
            "",
            "- missing section",
        ]

        _releases, failures = MODULE.parse_releases(lines)

        self.assertTrue(any("needs at least one change type section" in item for item in failures))

    def test_workspace_versions_fail_when_internal_pin_drifts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            crate = root / "crates" / "g7-cli"
            crate.mkdir(parents=True)
            crate.joinpath("Cargo.toml").write_text(
                "\n".join(
                    [
                        "[package]",
                        'name = "g7-cli"',
                        'version = "0.1.0"',
                        "",
                        "[dependencies]",
                        'g7-core = { path = "../g7-core", version = "=0.1.1" }',
                    ]
                ),
                encoding="utf-8",
            )

            failures = MODULE.check_workspace_versions(root)

        self.assertTrue(any("internal dependency pins" in item for item in failures))


if __name__ == "__main__":
    unittest.main()
