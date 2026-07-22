import importlib.util
import tempfile
import unittest
from io import StringIO
from pathlib import Path
from unittest import mock


SCRIPT = Path(__file__).parents[1] / "check-coverage-ratchet.py"
SPEC = importlib.util.spec_from_file_location("coverage_ratchet", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class CoverageRatchetTests(unittest.TestCase):
    def report(self, root: Path, percentages: dict[str, float]) -> dict:
        return {
            "data": [{
                "files": [
                    {
                        "filename": str(root / path),
                        "summary": {"lines": {"percent": percent}},
                    }
                    for path, percent in percentages.items()
                ]
            }]
        }

    def test_accepts_all_critical_modules_at_their_floor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = self.report(root, MODULE.FLOORS)
            with mock.patch.object(MODULE.sys, "stdout", StringIO()):
                self.assertEqual(MODULE.check_report(report, root), [])

    def test_rejects_missing_and_regressed_critical_modules(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            percentages = dict(MODULE.FLOORS)
            missing = next(iter(percentages))
            percentages.pop(missing)
            regressed = next(iter(percentages))
            percentages[regressed] -= 0.01
            with mock.patch.object(MODULE.sys, "stdout", StringIO()):
                failures = MODULE.check_report(self.report(root, percentages), root)
            self.assertTrue(any(missing in item for item in failures))
            self.assertTrue(any(regressed in item for item in failures))


if __name__ == "__main__":
    unittest.main()
