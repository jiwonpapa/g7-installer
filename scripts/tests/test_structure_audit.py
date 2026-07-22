import tempfile
import unittest
from io import StringIO
from pathlib import Path
from unittest import mock

import importlib.util


SCRIPT = Path(__file__).parents[1] / "structure-audit.py"
SPEC = importlib.util.spec_from_file_location("structure_audit", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class StructureAuditTests(unittest.TestCase):
    def test_new_large_file_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "new_big.rs"
            source.write_text("fn main() {}\n" * (MODULE.NEW_LARGE_FILE_LIMIT + 1), encoding="utf-8")

            failures = MODULE.check_large_files(root, [source])

        self.assertEqual(len(failures), 1)
        self.assertIn("new large file", failures[0])

    def test_baseline_file_can_grow_only_inside_allowance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "web" / "app.js"
            source.parent.mkdir()
            source.write_text(
                "console.log('x');\n"
                * (
                    MODULE.LARGE_FILE_BASELINE["web/app.js"]
                    + MODULE.BASELINE_GROWTH_ALLOWANCE
                    + 1
                ),
                encoding="utf-8",
            )

            failures = MODULE.check_large_files(root, [source])

        self.assertEqual(len(failures), 1)
        self.assertIn("large file grew", failures[0])

    def test_new_shell_command_construction_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "crates" / "x.rs"
            source.parent.mkdir(parents=True)
            source.write_text('CommandSpec::new("sh").arg("-c");\n', encoding="utf-8")

            failures = MODULE.check_shell_exceptions(root, [source])

        self.assertEqual(len(failures), 1)
        self.assertIn("new shell command", failures[0])

    def test_live_fixture_allowed_only_in_fixture_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            allowed = root / "scripts" / "web-ui-e2e.spec.mjs"
            allowed.parent.mkdir(parents=True)
            allowed.write_text("const domain = 'g7devops.com';\n", encoding="utf-8")
            leaked = root / "web" / "app.js"
            leaked.parent.mkdir()
            leaked.write_text("const domain = 'g7devops.com';\n", encoding="utf-8")

            failures = MODULE.check_live_fixtures(root, [allowed, leaked])

        self.assertEqual(len(failures), 1)
        self.assertIn("web/app.js", failures[0])

    def test_build_artifact_check_is_explicit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "target").mkdir()

            failures = MODULE.check_build_artifacts(root)

        self.assertEqual(failures, ["repo-local build artifact directory is present: target"])

    def test_main_passes_without_build_artifact_mode(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "target").mkdir()
            with (
                mock.patch.object(MODULE.sys, "argv", ["structure-audit.py", "--root", str(root)]),
                mock.patch.object(MODULE.sys, "stdout", StringIO()),
            ):
                status = MODULE.main()

        self.assertEqual(status, 0)


if __name__ == "__main__":
    unittest.main()
