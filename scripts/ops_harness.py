#!/usr/bin/env python3
"""Disposable VPS operations harness for G7 Installer.

The harness intentionally keeps destructive server orchestration outside the
installer binary. It verifies a disposable VPS from SSH, captures evidence, and
checks the report/state contracts that prove install, reset, and fresh-server
gates still work.
"""

from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from urllib.parse import urlparse


ROOT_DIR = Path(__file__).resolve().parents[1]
DEFAULT_STEPS = (
    "fresh-doctor,plan,install,report-contract,state-contract,setup-guide,"
    "app-smoke,post-install-doctor,reset-dry-run,reset,fresh-doctor-after-reset"
)


class HarnessError(RuntimeError):
    """Expected harness failure with a user-readable message."""


def env(name: str, default: str = "") -> str:
    return os.environ.get(name, default)


def shell_quote(value: str) -> str:
    return shlex.quote(value)


def read_cli_version() -> str:
    cargo_toml = ROOT_DIR / "crates/g7-cli/Cargo.toml"
    for line in cargo_toml.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("version = "):
            return line.split('"', 2)[1]
    raise HarnessError(f"could not read version from {cargo_toml}")


@dataclass(frozen=True)
class HarnessConfig:
    host: str = field(default_factory=lambda: env("G7_OPS_HOST", "g7-test"))
    domain: str = field(default_factory=lambda: env("G7_OPS_DOMAIN"))
    source: str = field(default_factory=lambda: env("G7_OPS_SOURCE", "release"))
    target: str = field(default_factory=lambda: env("G7_TARGET", "x86_64-unknown-linux-musl"))
    cli_bin: str = field(default_factory=lambda: env("G7_CLI_BIN", "g7inst"))
    repo: str = field(default_factory=lambda: env("G7_INSTALL_REPO", "jiwonpapa/g7-installer"))
    expected_version: str = field(default_factory=lambda: env("G7_OPS_EXPECT_VERSION") or read_cli_version())
    sudo: str = field(default_factory=lambda: env("G7_OPS_SUDO", "sudo -n"))
    verify_reinstall: bool = field(default_factory=lambda: env("G7_OPS_VERIFY_REINSTALL", "0") == "1")
    cleanup: bool = field(default_factory=lambda: env("G7_OPS_CLEANUP", "1") == "1")
    certbot_scope: str = field(default_factory=lambda: env("G7_OPS_CERTBOT_SCOPE", "staging"))
    app_smoke: bool = field(default_factory=lambda: env("G7_OPS_APP_SMOKE", "1") == "1")
    app_profile: str = field(default_factory=lambda: env("G7_OPS_APP", "gnuboard7"))
    web_server: str = field(default_factory=lambda: env("G7_OPS_WEB_SERVER", "nginx"))
    php_version: str = field(default_factory=lambda: env("G7_OPS_PHP_VERSION", "8.5"))
    php_source: str = field(default_factory=lambda: env("G7_OPS_PHP_SOURCE", "auto"))
    database: str = field(default_factory=lambda: env("G7_OPS_DATABASE", "mysql"))
    database_version: str = field(default_factory=lambda: env("G7_OPS_DATABASE_VERSION", "8.0"))
    redis: str = field(default_factory=lambda: env("G7_OPS_REDIS", "enable"))
    mail_mode: str = field(default_factory=lambda: env("G7_OPS_MAIL_MODE", "none"))
    www_mode: str = field(default_factory=lambda: env("G7_OPS_WWW_MODE", "redirect-to-root"))
    steps_raw: str = field(default_factory=lambda: env("G7_OPS_STEPS", DEFAULT_STEPS).replace(" ", ""))
    pre_clean: str = field(default_factory=lambda: env("G7_OPS_PRE_CLEAN", "auto"))
    allow_local_test: bool = field(default_factory=lambda: env("G7_OPS_ALLOW_LOCAL_TEST", "0") == "1")
    confirm_disposable: bool = field(default_factory=lambda: env("G7_OPS_CONFIRM_DISPOSABLE", "0") == "1")
    report_dir: Path = field(
        default_factory=lambda: Path(
            env(
                "G7_OPS_REPORT_DIR",
                str(ROOT_DIR / "target/ops-harness" / datetime.now().strftime("%Y%m%d-%H%M%S")),
            )
        )
    )

    @property
    def install_version(self) -> str:
        return env("G7_OPS_VERSION", f"v{self.expected_version}")

    @property
    def bootstrap_url(self) -> str:
        return f"https://github.com/{self.repo}/releases/download/{self.install_version}/bootstrap.sh"

    @property
    def remote_bin(self) -> str:
        default = f"/usr/local/bin/{self.cli_bin}" if self.source == "release" else f"/tmp/{self.cli_bin}"
        return env("G7_OPS_REMOTE_BIN", default)

    @property
    def steps(self) -> set[str]:
        return {step for step in self.steps_raw.split(",") if step}

    def step_enabled(self, step: str) -> bool:
        steps = self.steps
        return "all" in steps or step in steps

    def pre_clean_enabled(self) -> bool:
        value = self.pre_clean
        if value in {"1", "true", "yes"}:
            return True
        if value in {"0", "false", "no"}:
            return False
        if value == "auto":
            return self.step_enabled("install")
        raise HarnessError(f"unsupported G7_OPS_PRE_CLEAN: {value} (use auto, 1, or 0)")

    def install_env_prefix(self) -> str:
        return "env G7_CERTBOT_STAGING=1 " if self.certbot_scope == "staging" else ""

    def install_args(self) -> str:
        common = " ".join(
            [
                "--app",
                shell_quote(self.app_profile),
                "--web-server",
                shell_quote(self.web_server),
                "--php-version",
                shell_quote(self.php_version),
                "--php-source",
                shell_quote(self.php_source),
                "--database",
                shell_quote(self.database),
                "--database-version",
                shell_quote(self.database_version),
                "--redis",
                shell_quote(self.redis),
                "--mail-mode",
                shell_quote(self.mail_mode),
                "--www-mode",
                shell_quote(self.www_mode),
            ]
        )
        if self.certbot_scope == "skip":
            return f"--local-test --domain {shell_quote(self.domain)} {common}"
        return f"--domain {shell_quote(self.domain)} {common}"

    def validate(self) -> None:
        if self.source not in {"release", "local"}:
            raise HarnessError(f"unsupported G7_OPS_SOURCE: {self.source} (use release or local)")
        if self.app_profile not in {"gnuboard7", "laravel"}:
            raise HarnessError(f"unsupported G7_OPS_APP: {self.app_profile} (use gnuboard7 or laravel)")
        if self.web_server not in {"nginx", "apache"}:
            raise HarnessError(f"unsupported G7_OPS_WEB_SERVER: {self.web_server} (use nginx or apache)")
        if self.php_version not in {"8.3", "8.5"}:
            raise HarnessError(f"unsupported G7_OPS_PHP_VERSION: {self.php_version} (use 8.3 or 8.5)")
        if self.database != "mysql":
            raise HarnessError(f"unsupported G7_OPS_DATABASE: {self.database} (use mysql)")
        if self.database_version not in {"8.0", "8.4"}:
            raise HarnessError(
                f"unsupported G7_OPS_DATABASE_VERSION: {self.database_version} (use 8.0 or 8.4)"
            )
        if self.certbot_scope not in {"skip", "staging", "production"}:
            raise HarnessError(
                f"unsupported G7_OPS_CERTBOT_SCOPE: {self.certbot_scope} "
                "(use skip, staging, or production)"
            )
        if self.certbot_scope == "production" and env("G7_OPS_ALLOW_PRODUCTION_LE", "0") != "1":
            raise HarnessError("G7_OPS_CERTBOT_SCOPE=production requires G7_OPS_ALLOW_PRODUCTION_LE=1")
        if not self.domain:
            raise HarnessError("G7_OPS_DOMAIN is required. Use a real DNS domain for the ops harness.")
        if self.certbot_scope == "skip" and not self.allow_local_test:
            raise HarnessError(
                "G7_OPS_CERTBOT_SCOPE=skip runs --local-test and is disabled by default. "
                "Use staging with a real DNS domain, or set G7_OPS_ALLOW_LOCAL_TEST=1."
            )
        if self.domain.endswith(".local") and not self.allow_local_test:
            raise HarnessError(f"local-test domain {self.domain} requires G7_OPS_ALLOW_LOCAL_TEST=1.")
        if self.certbot_scope != "skip" and self.domain.endswith(".local"):
            raise HarnessError(
                f"G7_OPS_CERTBOT_SCOPE={self.certbot_scope} requires a real DNS domain, not {self.domain}"
            )
        if not self.confirm_disposable:
            raise HarnessError(
                "Refusing to run destructive ops harness. Set G7_OPS_CONFIRM_DISPOSABLE=1 "
                f"after confirming {self.host} is a disposable Ubuntu test VPS."
            )


def run_local(args: list[str], *, cwd: Path = ROOT_DIR, capture: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.STDOUT if capture else None,
        check=False,
    )


def require_local(command: str) -> None:
    if shutil.which(command) is None:
        raise HarnessError(f"missing local command: {command}")


def assert_contains(label: str, haystack: str, needle: str) -> None:
    if needle not in haystack:
        raise HarnessError(f"{label} did not contain expected text: {needle}")


def validate_report(path: Path) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1:
        raise HarnessError(f"unsupported schema_version: {data.get('schema_version')}")

    required = (
        "domain",
        "deployment_mode",
        "app_profile",
        "web_server",
        "php_version",
        "database",
        "database_name",
        "database_user",
        "site_user",
        "web_root",
    )
    missing = [key for key in required if not data.get(key)]
    if missing:
        raise HarnessError(f"missing required report fields: {', '.join(missing)}")
    if data.get("phase") != "completed":
        raise HarnessError(f"unexpected phase: {data.get('phase')}")
    if not data.get("preinstall_package_checks"):
        raise HarnessError("missing preinstall_package_checks")

    sections = (
        "safety_checks",
        "preinstall_package_checks",
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
    )
    for section in sections:
        checks = data.get(section)
        if not isinstance(checks, list):
            raise HarnessError(f"{section} is missing or is not a list")
        failed = [
            f"{item.get('name')}: {item.get('message')}"
            for item in checks
            if item.get("status") == "fail"
        ]
        if failed:
            raise HarnessError(f"{section} failed: {', '.join(failed)}")


def validate_state_contract(path: Path) -> None:
    state = json.loads(path.read_text(encoding="utf-8"))
    if state.get("version") != 2:
        raise HarnessError(f"unexpected state version: {state.get('version')}")
    if state.get("phase") != "completed":
        raise HarnessError(f"unexpected state phase: {state.get('phase')}")
    if state.get("current_step") is not None:
        raise HarnessError(f"current_step remains set: {state.get('current_step')}")

    steps = {item.get("id"): item for item in state.get("steps") or []}
    required = {"packages", "site", "vhost", "runtime", "database", "tls", "app"}
    missing = sorted(required - set(steps))
    if missing:
        raise HarnessError(f"missing installer steps: {', '.join(missing)}")
    failed = [
        f"{name}={steps[name].get('status')}"
        for name in sorted(required)
        if steps[name].get("status") != "completed"
    ]
    if failed:
        raise HarnessError(f"non-completed installer steps: {', '.join(failed)}")
    if any((steps[name].get("attempts") or 0) < 1 for name in required):
        raise HarnessError("one or more installer steps have no recorded attempt")


def new_package_names(report: dict) -> list[str]:
    return [
        item.get("name", "")
        for item in report.get("preinstall_package_checks") or []
        if item.get("status") == "not-installed" and item.get("name")
    ]


class OpsHarness:
    def __init__(self, config: HarnessConfig):
        self.config = config
        self.report_dir = config.report_dir

    def log(self, message: str) -> None:
        print(f"[ops-harness] {message}", flush=True)

    def fail(self, message: str) -> None:
        raise HarnessError(message)

    def remote(self, command: str) -> subprocess.CompletedProcess[str]:
        return run_local(["ssh", "-n", self.config.host, command])

    def capture_remote(self, label: str, command: str) -> str:
        result = self.remote(command)
        output = result.stdout or ""
        suffix = "log" if result.returncode == 0 else "failed.log"
        (self.report_dir / f"{label}.{suffix}").write_text(output, encoding="utf-8")
        if result.returncode != 0:
            self.fail(f"{label} command failed; see {self.report_dir / f'{label}.failed.log'}")
        return output

    def sudo_capture(self, label: str, command: str) -> str:
        return self.capture_remote(label, f"{self.config.sudo} {command}")

    def sudo_sh_capture(self, label: str, command: str) -> str:
        return self.capture_remote(label, f"{self.config.sudo} sh -c {shell_quote(command)}")

    def assert_not_installed(self, cycle: str, package: str) -> None:
        status = self.sudo_capture(
            f"{cycle}-package-{package}-status",
            f"dpkg-query -W -f='\\${{Status}}' {shell_quote(package)} 2>/dev/null || true",
        ).strip()
        if status == "install ok installed":
            self.fail(f"package still installed after rollback: {package}")

    def assert_installer_resources_absent(self, cycle: str, report_path: Path) -> None:
        report = json.loads(report_path.read_text(encoding="utf-8"))
        site_user = report["site_user"]
        web_root = report["web_root"]
        services = {"redis-server", "mysql"}
        services.add("apache2" if report.get("web_server") == "apache" else "nginx")
        if report.get("web_server") == "frankenphp":
            services.add("g7-frankenphp")

        quoted_paths = " ".join(
            shell_quote(path)
            for path in (
                "/etc/g7-installer",
                "/var/lib/g7-installer",
                "/var/log/g7-installer",
                "/var/backups/g7-installer",
                web_root,
            )
        )
        self.sudo_sh_capture(
            f"{cycle}-installer-paths-absent",
            f'for path in {quoted_paths}; do test ! -e "${{path}}" || exit 1; done',
        )
        self.sudo_sh_capture(f"{cycle}-site-account-absent", f"! id -u {shell_quote(site_user)} >/dev/null 2>&1")
        for service in sorted(services):
            self.sudo_sh_capture(
                f"{cycle}-service-{service}-inactive",
                f"! systemctl is-active --quiet {shell_quote(service)}",
            )

    def validate_effective_configuration(self, cycle: str, report_path: Path) -> None:
        report = json.loads(report_path.read_text(encoding="utf-8"))
        site_user = report["site_user"]
        database_name = report["database_name"]
        database_user = report["database_user"]
        php_pool = f"/etc/php/{self.config.php_version}/fpm/pool.d/{site_user}.conf"

        if self.config.web_server == "nginx":
            self.sudo_capture(f"{cycle}-nginx-configtest", "nginx -t")
            self.sudo_sh_capture(
                f"{cycle}-nginx-default-deny",
                "test -L /etc/nginx/sites-enabled/g7-default-deny "
                "&& test ! -e /etc/nginx/sites-enabled/default "
                "&& grep -Rqs 'return 444' /etc/nginx/sites-enabled/g7-default-deny",
            )
        else:
            self.sudo_capture(f"{cycle}-apache-configtest", "apache2ctl configtest")

        self.sudo_capture(f"{cycle}-php-fpm-configtest", f"php-fpm{self.config.php_version} -t")
        self.sudo_sh_capture(
            f"{cycle}-php-pool-contract",
            f"test -f {shell_quote(php_pool)} "
            f"&& test ! -e {shell_quote(f'/etc/php/{self.config.php_version}/fpm/pool.d/www.conf')} "
            f"&& grep -Fqs {shell_quote(f'user = {site_user}')} {shell_quote(php_pool)} "
            f"&& grep -Fqs 'group = www-data' {shell_quote(php_pool)}",
        )
        self.sudo_capture(f"{cycle}-mysql-configtest", "mysqld --validate-config")
        self.sudo_sh_capture(
            f"{cycle}-mysql-version",
            f"mysql -NBe 'SELECT VERSION()' | grep -Eq {shell_quote('^' + self.config.database_version + '[.]')}",
        )
        database_sql = (
            "SELECT SCHEMA_NAME FROM INFORMATION_SCHEMA.SCHEMATA "
            f"WHERE SCHEMA_NAME='{database_name}'"
        )
        account_sql = f"SELECT User FROM mysql.user WHERE User='{database_user}' AND Host='localhost'"
        self.sudo_capture(
            f"{cycle}-mysql-database",
            f"mysql -NBe {shell_quote(database_sql)}",
        )
        self.sudo_capture(
            f"{cycle}-mysql-account",
            f"mysql -NBe {shell_quote(account_sql)}",
        )

        if self.config.redis == "enable":
            self.sudo_sh_capture(
                f"{cycle}-redis-contract",
                "test \"$(redis-cli --raw CONFIG GET protected-mode | tail -n 1)\" = yes "
                "&& redis-cli --raw CONFIG GET bind | tail -n 1 "
                "| grep -Eq '(^|[[:space:]])127[.]0[.]0[.]1($|[[:space:]])'",
            )

    def run_app_smoke(self, cycle: str, report_path: Path) -> None:
        if not self.config.app_smoke:
            self.log(f"{cycle}: app smoke skipped (set G7_OPS_APP_SMOKE=1 to enable)")
            return

        report = json.loads(report_path.read_text(encoding="utf-8"))
        url = report.get("app_url") or ""
        if not url:
            self.fail(f"{cycle}: report did not contain app_url for app smoke")

        if report.get("deployment_mode") == "local-test":
            parsed = urlparse(url)
            host = parsed.hostname or ""
            port = parsed.port or (443 if parsed.scheme == "https" else 80)
            if not host:
                self.fail(f"{cycle}: could not parse app_url for local-test smoke: {url}")
            self.capture_remote(
                f"{cycle}-app-smoke",
                f"curl -fsSL --max-time 15 --resolve {shell_quote(f'{host}:{port}:127.0.0.1')} "
                f"{shell_quote(url)} >/dev/null",
            )
            return

        curl_flags = "-kfsSL" if self.config.certbot_scope == "staging" else "-fsSL"
        self.capture_remote(f"{cycle}-app-smoke", f"curl {curl_flags} --max-time 15 {shell_quote(url)} >/dev/null")

    def install_binary(self) -> None:
        if self.config.source == "release":
            self.log(f"installing release {self.config.install_version} on {self.config.host}")
            self.capture_remote(
                "bootstrap-download",
                f"curl -fsSL {shell_quote(self.config.bootstrap_url)} -o /tmp/g7-bootstrap.sh",
            )
            self.sudo_capture(
                "bootstrap-install",
                "env "
                f"G7_INSTALL_REPO={shell_quote(self.config.repo)} "
                f"G7_INSTALL_VERSION={shell_quote(self.config.install_version)} "
                "bash /tmp/g7-bootstrap.sh",
            )
            return

        self.log(f"building local {self.config.target} binary")
        result = run_local(
            [
                "cargo",
                "build",
                "--release",
                "--target",
                self.config.target,
                "-p",
                "g7-cli",
                "--bin",
                self.config.cli_bin,
            ]
        )
        if result.returncode != 0:
            (self.report_dir / "local-build.failed.log").write_text(result.stdout or "", encoding="utf-8")
            self.fail(f"local build failed; see {self.report_dir / 'local-build.failed.log'}")
        local_bin = ROOT_DIR / "target" / self.config.target / "release" / self.config.cli_bin
        scp = run_local(["scp", str(local_bin), f"{self.config.host}:{self.config.remote_bin}"])
        if scp.returncode != 0:
            (self.report_dir / "local-binary-copy.failed.log").write_text(scp.stdout or "", encoding="utf-8")
            self.fail(f"local binary copy failed; see {self.report_dir / 'local-binary-copy.failed.log'}")
        self.capture_remote("local-binary-chmod", f"chmod +x {shell_quote(self.config.remote_bin)}")

    def cleanup_previous_state(self) -> None:
        self.log("cleaning previous installer state if present")
        remote_bin = shell_quote(self.config.remote_bin)
        self.capture_remote(
            "pre-clean",
            f"if test -x {remote_bin}; then "
            f"{self.config.sudo} {remote_bin} rollback --yes >/tmp/g7-ops-pre-rollback.log 2>&1; "
            "rollback_status=$?; "
            f"{self.config.sudo} {remote_bin} reset --yes >/tmp/g7-ops-pre-reset.log 2>&1; "
            "reset_status=$?; "
            "printf 'rollback_status=%s reset_status=%s\\n' \"${rollback_status}\" \"${reset_status}\"; "
            "cat /tmp/g7-ops-pre-rollback.log /tmp/g7-ops-pre-reset.log; "
            "fi; true",
        )

    def run_install_cycle(self, cycle: str) -> None:
        remote_bin = shell_quote(self.config.remote_bin)
        args = self.config.install_args()
        env_prefix = self.config.install_env_prefix()
        report_path = self.report_dir / f"{cycle}-report.json"
        state_path = self.report_dir / f"{cycle}-state.json"
        package_list_path = self.report_dir / f"{cycle}-new-packages.txt"
        site_user = ""
        certificate_present = "no"

        if self.config.step_enabled("fresh-doctor"):
            self.log(f"{cycle}: preflight doctor")
            output = self.sudo_capture(f"{cycle}-doctor-before", f"{remote_bin} doctor")
            assert_contains(f"{cycle} doctor before", output, "install_allowed: true")
        else:
            self.log(f"{cycle}: preflight doctor skipped")

        if self.config.step_enabled("plan"):
            self.log(f"{cycle}: plan")
            self.capture_remote(f"{cycle}-plan", f"{remote_bin} plan {args}")
        else:
            self.log(f"{cycle}: plan skipped")

        if self.config.step_enabled("install"):
            self.log(f"{cycle}: install")
            output = self.sudo_capture(f"{cycle}-install", f"{env_prefix}{remote_bin} install {args}")
            assert_contains(f"{cycle} install", output, "phase: completed")
        else:
            self.log(f"{cycle}: install skipped")

        if any(
            self.config.step_enabled(step)
            for step in ("report-contract", "setup-guide", "app-smoke", "reset")
        ):
            report_json = self.sudo_capture(f"{cycle}-report-json", "cat /var/log/g7-installer/report.json")
            report_path.write_text(report_json, encoding="utf-8")
            report = json.loads(report_json)
            package_list_path.write_text("\n".join(new_package_names(report)) + "\n", encoding="utf-8")
            site_user = report.get("site_user") or ""
            certificate_present = self.sudo_sh_capture(
                f"{cycle}-certificate-before-reset",
                f"if test -d {shell_quote(f'/etc/letsencrypt/live/{self.config.domain}')}; then echo yes; else echo no; fi",
            ).strip()

        if self.config.step_enabled("report-contract"):
            self.log(f"{cycle}: install report contract")
            validate_report(report_path)
        else:
            self.log(f"{cycle}: install report contract skipped")

        if self.config.step_enabled("state-contract"):
            self.log(f"{cycle}: resumable state and transaction contract")
            state_path.write_text(
                self.sudo_capture(f"{cycle}-state-json", "cat /var/lib/g7-installer/state.json"),
                encoding="utf-8",
            )
            validate_state_contract(state_path)
            self.sudo_sh_capture(f"{cycle}-pending-secrets-absent", "test ! -e /var/lib/g7-installer/pending-secrets.toml")
            self.sudo_sh_capture(
                f"{cycle}-candidate-files-absent",
                "test ! -d /var/lib/g7-installer/candidates "
                "|| ! find /var/lib/g7-installer/candidates -type f -print -quit | grep -q .",
            )
            self.sudo_sh_capture(
                f"{cycle}-transactions-finished",
                "test ! -d /var/lib/g7-installer/transactions "
                "|| ! grep -R -q '\"status\": \"started\"' /var/lib/g7-installer/transactions",
            )
            self.sudo_sh_capture(
                f"{cycle}-secrets-mode",
                'test "$(stat -c %a /etc/g7-installer/secrets.toml)" = 600',
            )
            self.validate_effective_configuration(cycle, report_path)
        else:
            self.log(f"{cycle}: state contract skipped")

        if self.config.step_enabled("setup-guide"):
            self.log(f"{cycle}: setup guide capture")
            self.sudo_capture(f"{cycle}-setup-guide", "cat /var/log/g7-installer/setup-guide.md")
        else:
            self.log(f"{cycle}: setup guide capture skipped")

        if self.config.step_enabled("app-smoke"):
            self.run_app_smoke(cycle, report_path)
        else:
            self.log(f"{cycle}: app smoke step skipped")

        if self.config.step_enabled("post-install-doctor"):
            self.log(f"{cycle}: post-install doctor must block fresh install")
            output = self.sudo_capture(f"{cycle}-doctor-after-install", f"{remote_bin} doctor")
            assert_contains(f"{cycle} doctor after install", output, "install_allowed: false")
        else:
            self.log(f"{cycle}: post-install doctor skipped")

        if not self.config.cleanup:
            self.log(f"{cycle}: cleanup disabled; reset steps skipped")
            return

        if self.config.step_enabled("reset-dry-run"):
            self.log(f"{cycle}: reset dry-run preview")
            output = self.sudo_capture(f"{cycle}-reset-dry-run", f"{remote_bin} reset --yes --dry-run")
            assert_contains(f"{cycle} reset dry-run", output, "dry_run: true")
        else:
            self.log(f"{cycle}: reset dry-run skipped")

        if self.config.step_enabled("reset"):
            self.log(f"{cycle}: reset installer-created resources")
            output = self.sudo_capture(f"{cycle}-reset", f"{remote_bin} reset --yes")
            assert_contains(f"{cycle} reset", output, "G7 Installer Reset")
            assert_contains(f"{cycle} reset actions", output, "actions:")
            assert_contains(f"{cycle} database reset", output, " database -")
            assert_contains(f"{cycle} account reset", output, f"account:{site_user}")
            for package in package_list_path.read_text(encoding="utf-8").splitlines():
                if package:
                    self.assert_not_installed(cycle, package)
            self.assert_installer_resources_absent(cycle, report_path)
            if certificate_present == "yes":
                self.sudo_capture(
                    f"{cycle}-certificate-preserved",
                    f"test -d {shell_quote(f'/etc/letsencrypt/live/{self.config.domain}')}",
                )
        else:
            self.log(f"{cycle}: reset skipped")

        if self.config.step_enabled("fresh-doctor-after-reset"):
            self.log(f"{cycle}: doctor after reset must allow fresh install")
            output = self.sudo_capture(f"{cycle}-doctor-after-reset", f"{remote_bin} doctor")
            assert_contains(f"{cycle} doctor after reset", output, "install_allowed: true")
        else:
            self.log(f"{cycle}: doctor after reset skipped")

    def run(self) -> None:
        for command in ("ssh", "scp", "python3"):
            require_local(command)

        self.report_dir.mkdir(parents=True, exist_ok=True)
        self.log(f"writing artifacts to {self.report_dir}")
        self.log(f"steps: {self.config.steps_raw}")
        self.log(
            "profile: "
            f"{self.config.app_profile}; web: {self.config.web_server}; "
            f"PHP: {self.config.php_version}/{self.config.php_source}; "
            f"DB: {self.config.database} {self.config.database_version}"
        )
        self.log(
            f"certbot scope: {self.config.certbot_scope}; "
            f"app smoke: {int(self.config.app_smoke)}; pre-clean: {self.config.pre_clean}"
        )

        self.capture_remote("host-baseline", "uname -a; cat /etc/os-release; id")
        self.capture_remote("ubuntu-24-check", '. /etc/os-release && test "${ID}" = ubuntu && test "${VERSION_ID}" = 24.04')

        self.install_binary()
        if self.config.pre_clean_enabled():
            self.cleanup_previous_state()
        else:
            self.log("pre-clean skipped")

        version = self.capture_remote("g7-version", f"{shell_quote(self.config.remote_bin)} --version")
        assert_contains("version", version, self.config.expected_version)

        self.run_install_cycle("cycle1")

        if self.config.verify_reinstall:
            self.log("VERIFY_REINSTALL=1 requires the VPS to be restored to a fresh snapshot before cycle2")
            self.run_install_cycle("cycle2")

        if self.config.cleanup:
            self.log("cleanup reset installer-created resources completed")
        else:
            self.log("cleanup disabled; leaving final server state from last cycle")
        self.log("PASS")


def main() -> int:
    try:
        config = HarnessConfig()
        config.validate()
        OpsHarness(config).run()
        return 0
    except HarnessError as error:
        print(f"[ops-harness] failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
