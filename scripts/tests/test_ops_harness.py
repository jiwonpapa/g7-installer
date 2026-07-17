import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "ops_harness.py"
SPEC = importlib.util.spec_from_file_location("ops_harness", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class OpsHarnessTests(unittest.TestCase):
    def test_install_args_uses_local_test_only_for_skip_scope(self):
        config = MODULE.HarnessConfig(
            domain="g7-test.local",
            certbot_scope="skip",
            allow_local_test=True,
            confirm_disposable=True,
        )

        self.assertIn("--local-test", config.install_args())
        self.assertIn("--domain g7-test.local", config.install_args())

    def test_rejects_destructive_run_without_disposable_confirmation(self):
        config = MODULE.HarnessConfig(domain="example.com", confirm_disposable=False)

        with self.assertRaises(MODULE.HarnessError):
            config.validate()

    def test_validate_report_rejects_failed_sections(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            report = {
                "schema_version": 1,
                "phase": "completed",
                "domain": "example.com",
                "deployment_mode": "public",
                "app_profile": "gnuboard7",
                "web_server": "nginx",
                "php_version": "8.5",
                "database": "mysql",
                "database_name": "g7",
                "database_user": "g7",
                "site_user": "g7",
                "web_root": "/home/g7/public_html",
                "preinstall_package_checks": [{"name": "nginx", "status": "installed"}],
            }
            for section in (
                "safety_checks",
                "package_checks",
                "service_checks",
                "port_checks",
                "network_checks",
                "runtime_checks",
                "database_checks",
                "firewall_checks",
                "mail_checks",
                "certbot_checks",
                "vhost_checks",
                "app_checks",
            ):
                report[section] = []
            report["certbot_checks"] = [{"name": "tls", "status": "fail", "message": "broken"}]
            path.write_text(json.dumps(report), encoding="utf-8")

            with self.assertRaises(MODULE.HarnessError):
                MODULE.validate_report(path)

    def test_validate_state_contract_accepts_completed_v2_steps(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            path.write_text(
                json.dumps(
                    {
                        "version": 2,
                        "phase": "completed",
                        "current_step": None,
                        "steps": [
                            {"id": step, "status": "completed", "attempts": 1}
                            for step in ("packages", "site", "vhost", "runtime", "database", "tls", "app")
                        ],
                    }
                ),
                encoding="utf-8",
            )

            MODULE.validate_state_contract(path)


if __name__ == "__main__":
    unittest.main()
