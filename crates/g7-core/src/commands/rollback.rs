//! Early package rollback for G7 Installer.
//!
//! Safety boundary: this command only reverses the package-install phase on a
//! fresh setup. It refuses to run when app content, vhost/database/certificate
//! steps, non-empty web roots, missing package baseline data, or pre-existing
//! packages are detected. Operating sites must be restored from normal backups,
//! not by this installer rollback.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::commands::reset::{self, ResetPaths, ResetReport};
use crate::{Error, Result};
use g7_state::state::{STATE_PATH, read_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandOutput, CommandRunner};
use g7_system::package::PackageStatus;

const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
const REQUIRED_PHASE: &str = "packages-installed";
const SAFE_BASELINE_STATUS: &str = "not-installed";
const APP_MUTATION_STEPS: [&str; 11] = [
    "web-root-created",
    "web-root-populated",
    "vhost-written",
    "vhost-enabled",
    "php-fpm-config-written",
    "database-created",
    "database-user-created",
    "redis-config-written",
    "g7-release-installed",
    "certbot-issued",
    "ssh-hardened",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackReport {
    pub dry_run: bool,
    pub phase: String,
    pub package_actions: Vec<RollbackAction>,
    pub service_actions: Vec<RollbackAction>,
    pub metadata_reset: ResetReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackAction {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl RollbackAction {
    fn new(name: impl Into<String>, status: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPaths {
    root: PathBuf,
}

impl RollbackPaths {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);

        if self.root == Path::new("/") {
            return path.to_path_buf();
        }

        match path.strip_prefix("/") {
            Ok(stripped) => self.root.join(stripped),
            Err(_) => self.root.join(path),
        }
    }

    fn reset_paths(&self) -> ResetPaths {
        ResetPaths::with_root(self.root.clone())
    }
}

pub fn run(yes: bool, dry_run: bool) -> Result<RollbackReport> {
    run_with_probe_and_paths(yes, dry_run, &SystemProbe::real(), &RollbackPaths::system())
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    yes: bool,
    dry_run: bool,
    probe: &SystemProbe<R>,
    paths: &RollbackPaths,
) -> Result<RollbackReport> {
    if !yes && !dry_run {
        return Err(Error::RollbackConfirmationRequired);
    }

    require_root(probe)?;
    let state_path = paths.resolve(STATE_PATH);
    let state = read_state_file(&state_path).map_err(|source| Error::FileReadFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    require_safe_phase(&state.phase)?;
    require_no_app_mutation_steps(&state.completed_steps)?;

    let report = read_report(paths)?;
    require_report_phase(&report)?;
    let baseline = baseline_package_checks(&report)?;
    require_known_package_baseline(&baseline)?;
    let packages = rollback_packages(&baseline);
    let services = check_names(&report, "service_checks");
    let service_plan = planned_service_actions(&services, &baseline, &report)?;
    require_web_root_unused(probe, report_string(&report, "web_root").as_deref())?;

    let metadata_reset = reset::run_with_probe_and_paths(true, true, probe, &paths.reset_paths())?;

    if dry_run {
        return Ok(RollbackReport {
            dry_run,
            phase: state.phase,
            package_actions: planned_package_actions(&baseline),
            service_actions: service_plan,
            metadata_reset,
        });
    }

    let services_to_disable = service_plan
        .iter()
        .filter(|action| action.status == "would-disable")
        .map(|action| action.name.clone())
        .collect::<Vec<_>>();
    let mut service_actions = disable_services(probe, &services_to_disable)?;
    service_actions.extend(
        service_plan
            .into_iter()
            .filter(|action| action.status == "skipped"),
    );
    if !packages.is_empty() {
        purge_packages(probe, &packages)?;
    }
    let mut package_actions = verify_packages_removed(probe, &packages)?;
    package_actions.extend(
        baseline
            .iter()
            .filter(|check| check.status == "installed")
            .map(|check| {
                RollbackAction::new(
                    &check.name,
                    "skipped",
                    "package was already installed before G7 installer ran",
                )
            }),
    );
    let failed_packages = package_actions
        .iter()
        .filter(|action| action.status == "fail")
        .map(|action| format!("{}: {}", action.name, action.message))
        .collect::<Vec<_>>();
    if !failed_packages.is_empty() {
        return Err(Error::RollbackVerificationFailed {
            checks: failed_packages.join(", "),
        });
    }

    let metadata_reset = reset::run_with_probe_and_paths(true, false, probe, &paths.reset_paths())?;

    Ok(RollbackReport {
        dry_run,
        phase: state.phase,
        package_actions,
        service_actions,
        metadata_reset,
    })
}

fn require_root<R: CommandRunner>(probe: &SystemProbe<R>) -> Result<()> {
    match probe.current_privilege() {
        Ok(g7_system::privilege::Privilege::Root) => Ok(()),
        _ => Err(Error::PrivilegeRequired),
    }
}

fn require_safe_phase(phase: &str) -> Result<()> {
    if phase == REQUIRED_PHASE {
        Ok(())
    } else {
        Err(Error::RollbackBlocked {
            reason: format!("state phase is `{phase}`, expected `{REQUIRED_PHASE}`"),
        })
    }
}

fn require_no_app_mutation_steps(steps: &[String]) -> Result<()> {
    let matched = steps
        .iter()
        .filter(|step| APP_MUTATION_STEPS.contains(&step.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if matched.is_empty() {
        Ok(())
    } else {
        Err(Error::RollbackBlocked {
            reason: format!(
                "app/site mutation steps are present: {}",
                matched.join(", ")
            ),
        })
    }
}

fn read_report(paths: &RollbackPaths) -> Result<serde_json::Value> {
    let payload = fs::read(paths.resolve(REPORT_PATH)).map_err(|source| Error::FileReadFailed {
        path: REPORT_PATH.to_string(),
        source,
    })?;

    serde_json::from_slice(&payload).map_err(|source| Error::FileReadFailed {
        path: REPORT_PATH.to_string(),
        source: io::Error::other(source),
    })
}

fn require_report_phase(report: &serde_json::Value) -> Result<()> {
    match report_string(report, "phase").as_deref() {
        Some(REQUIRED_PHASE) => Ok(()),
        Some(phase) => Err(Error::RollbackBlocked {
            reason: format!("report phase is `{phase}`, expected `{REQUIRED_PHASE}`"),
        }),
        None => Err(Error::RollbackBlocked {
            reason: "report phase is missing".to_string(),
        }),
    }
}

fn baseline_package_checks(report: &serde_json::Value) -> Result<Vec<ReportCheck>> {
    let checks = report_checks(report, "preinstall_package_checks");
    if checks.is_empty() {
        return Err(Error::RollbackBlocked {
            reason: "missing preinstall package baseline; use reset for metadata only".to_string(),
        });
    }
    Ok(checks)
}

fn require_known_package_baseline(baseline: &[ReportCheck]) -> Result<()> {
    let unknown_packages = baseline
        .iter()
        .filter(|check| check.status != SAFE_BASELINE_STATUS && check.status != "installed")
        .map(|check| format!("{}({})", check.name, check.status))
        .collect::<Vec<_>>();
    if unknown_packages.is_empty() {
        Ok(())
    } else {
        Err(Error::RollbackBlocked {
            reason: format!(
                "some packages have unknown preinstall state: {}",
                unknown_packages.join(", ")
            ),
        })
    }
}

fn rollback_packages(baseline: &[ReportCheck]) -> Vec<String> {
    unique_names(
        baseline
            .iter()
            .filter(|check| check.status == SAFE_BASELINE_STATUS)
            .map(|check| check.name.as_str()),
    )
}

fn planned_package_actions(baseline: &[ReportCheck]) -> Vec<RollbackAction> {
    baseline
        .iter()
        .map(|check| {
            if check.status == SAFE_BASELINE_STATUS {
                RollbackAction::new(&check.name, "would-purge", "package would be purged")
            } else {
                RollbackAction::new(
                    &check.name,
                    "skipped",
                    "package was already installed before G7 installer ran",
                )
            }
        })
        .collect()
}

fn planned_service_actions(
    services: &[String],
    baseline: &[ReportCheck],
    report: &serde_json::Value,
) -> Result<Vec<RollbackAction>> {
    services
        .iter()
        .map(|service| {
            let owners = service_owner_packages(service, report);
            let owner_statuses = owners
                .iter()
                .map(|package| {
                    package_baseline_status(baseline, package).ok_or_else(|| {
                        Error::RollbackBlocked {
                            reason: format!(
                                "missing package baseline for service `{service}` owner `{package}`"
                            ),
                        }
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            if owner_statuses
                .iter()
                .all(|status| *status == SAFE_BASELINE_STATUS)
            {
                Ok(RollbackAction::new(
                    service,
                    "would-disable",
                    "service would be disabled",
                ))
            } else {
                Ok(RollbackAction::new(
                    service,
                    "skipped",
                    "service owner package existed before G7 installer ran",
                ))
            }
        })
        .collect()
}

fn service_owner_packages(service: &str, report: &serde_json::Value) -> Vec<String> {
    let php_version = report_string(report, "php_version").unwrap_or_else(|| "8.3".to_string());
    let web_server = report_string(report, "web_server").unwrap_or_else(|| "nginx".to_string());
    let database = report_string(report, "database").unwrap_or_else(|| "mysql".to_string());

    if service == "nginx" || (service == "apache2" && web_server == "apache") {
        return vec![service.to_string()];
    }
    if service == format!("php{php_version}-fpm") {
        return vec![service.to_string()];
    }
    if service == "mysql" {
        return vec![if database == "mariadb" {
            "mariadb-server".to_string()
        } else {
            "mysql-server".to_string()
        }];
    }
    if service == "mariadb" {
        return vec!["mariadb-server".to_string()];
    }
    if service == "redis-server" {
        return vec!["redis-server".to_string()];
    }
    if service == "postfix" {
        return vec!["postfix".to_string()];
    }
    if service == "certbot.timer" {
        return vec!["certbot".to_string()];
    }

    vec![service.to_string()]
}

fn package_baseline_status<'a>(baseline: &'a [ReportCheck], package: &str) -> Option<&'a str> {
    baseline
        .iter()
        .find(|check| check.name == package)
        .map(|check| check.status.as_str())
}

fn require_web_root_unused<R: CommandRunner>(
    probe: &SystemProbe<R>,
    web_root: Option<&str>,
) -> Result<()> {
    let Some(web_root) = web_root else {
        return Ok(());
    };
    let path = Path::new(web_root);
    if !path.is_absolute() || path == Path::new("/") {
        return Err(Error::RollbackBlocked {
            reason: format!("unsafe web root path in report: {web_root}"),
        });
    }
    if !probe.path_exists(path) {
        return Ok(());
    }

    let entries = probe
        .directory_entries(path)
        .map_err(|err| Error::RollbackBlocked {
            reason: format!("failed to inspect web root `{web_root}`: {err}"),
        })?;
    if entries.is_empty() {
        Ok(())
    } else {
        Err(Error::RollbackBlocked {
            reason: format!("web root is not empty: {web_root}"),
        })
    }
}

fn disable_services<R: CommandRunner>(
    probe: &SystemProbe<R>,
    services: &[String],
) -> Result<Vec<RollbackAction>> {
    services
        .iter()
        .map(|service| {
            let command = format!("systemctl disable --now {service}");
            let output = probe
                .disable_service_now(service)
                .map_err(|err| command_error("service-disable", command.clone(), err))?;
            require_success("service-disable", command, output)?;
            Ok(RollbackAction::new(service, "disabled", "service disabled"))
        })
        .collect()
}

fn purge_packages<R: CommandRunner>(probe: &SystemProbe<R>, packages: &[String]) -> Result<()> {
    let command = format!("apt-get purge -y --auto-remove {}", packages.join(" "));
    let output = probe
        .apt_purge(packages)
        .map_err(|err| command_error("apt-purge", command.clone(), err))?;
    require_success("apt-purge", command, output)
}

fn verify_packages_removed<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
) -> Result<Vec<RollbackAction>> {
    packages
        .iter()
        .map(|package| match probe.package_status(package) {
            Ok(PackageStatus::NotInstalled) => Ok(RollbackAction::new(
                package,
                "removed",
                "package is no longer installed",
            )),
            Ok(PackageStatus::Installed) => Ok(RollbackAction::new(
                package,
                "fail",
                "package is still installed",
            )),
            Ok(PackageStatus::Unknown) => Ok(RollbackAction::new(
                package,
                "fail",
                "package status is unknown",
            )),
            Err(err) => Err(command_error(
                "package-verify",
                format!("dpkg-query {package}"),
                err,
            )),
        })
        .collect()
}

fn require_success(step: &'static str, command: String, output: CommandOutput) -> Result<()> {
    if output.status == 0 {
        Ok(())
    } else {
        Err(Error::RollbackCommandFailed {
            step,
            command,
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn command_error(step: &'static str, command: impl Into<String>, err: impl ToString) -> Error {
    Error::RollbackCommandFailed {
        step,
        command: command.into(),
        status: 128,
        stdout: String::new(),
        stderr: err.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportCheck {
    name: String,
    status: String,
}

fn report_checks(report: &serde_json::Value, key: &str) -> Vec<ReportCheck> {
    report
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(ReportCheck {
                name: item.get("name")?.as_str()?.trim().to_string(),
                status: item.get("status")?.as_str()?.trim().to_string(),
            })
        })
        .filter(|check| !check.name.is_empty())
        .collect()
}

fn check_names(report: &serde_json::Value, key: &str) -> Vec<String> {
    unique_names(
        report_checks(report, key)
            .iter()
            .map(|check| check.name.as_str()),
    )
}

fn unique_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut unique = Vec::new();
    for name in names {
        if !unique.iter().any(|item: &String| item == name) {
            unique.push(name.to_string());
        }
    }
    unique
}

fn report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report.get(key)?.as_str().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{RollbackPaths, run_with_probe_and_paths};
    use crate::Error;
    use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
    use g7_state::state::{InstallerState, STATE_PATH, write_state_file};
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn rollback_dry_run_lists_package_service_and_metadata_changes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = rollback_fs_root(false, true)?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let report =
            run_with_probe_and_paths(false, true, &probe, &RollbackPaths::with_root(&fs_root))?;

        assert!(report.dry_run);
        assert_eq!(report.phase, "packages-installed");
        assert!(
            report
                .package_actions
                .iter()
                .any(|action| action.name == "nginx" && action.status == "would-purge")
        );
        assert!(
            report
                .service_actions
                .iter()
                .any(|action| action.name == "nginx" && action.status == "would-disable")
        );
        assert!(
            report
                .metadata_reset
                .removed
                .contains(&"/etc/g7-installer/config.toml".to_string())
        );

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn rollback_purges_packages_and_resets_metadata()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = rollback_fs_root(false, true)?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("apt purge ok"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let report =
            run_with_probe_and_paths(true, false, &probe, &RollbackPaths::with_root(&fs_root))?;

        assert!(!report.dry_run);
        assert!(
            report
                .package_actions
                .iter()
                .all(|action| action.status == "removed")
        );
        assert!(!fs_root.join("etc/g7-installer/config.toml").exists());
        assert!(!fs_root.join(strip_root(STATE_PATH)).exists());

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn rollback_skips_packages_and_services_that_existed_before_install()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = rollback_fs_root(false, true)?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
  "version": 1,
  "domain": "example.com",
  "phase": "packages-installed",
  "web_server": "nginx",
  "php_version": "8.3",
  "database": "mysql",
  "web_root": "/home/g7/public_html",
  "preinstall_package_checks": [
    { "name": "nginx", "status": "installed", "message": "already installed" },
    { "name": "php8.3-fpm", "status": "not-installed", "message": "absent" }
  ],
  "package_checks": [
    { "name": "nginx", "status": "pass", "message": "package installed" },
    { "name": "php8.3-fpm", "status": "pass", "message": "package installed" }
  ],
  "service_checks": [
    { "name": "nginx", "status": "pass", "message": "service is active" },
    { "name": "php8.3-fpm", "status": "pass", "message": "service is active" }
  ],
  "port_checks": []
}
"#,
        )?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("apt purge ok"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let report =
            run_with_probe_and_paths(true, false, &probe, &RollbackPaths::with_root(&fs_root))?;

        assert!(
            report
                .package_actions
                .iter()
                .any(|action| action.name == "nginx" && action.status == "skipped")
        );
        assert!(
            report
                .service_actions
                .iter()
                .any(|action| action.name == "nginx" && action.status == "skipped")
        );
        assert!(
            report
                .package_actions
                .iter()
                .any(|action| action.name == "php8.3-fpm" && action.status == "removed")
        );

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn rollback_blocks_when_web_root_has_content()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = rollback_fs_root(true, true)?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let err = match run_with_probe_and_paths(
            true,
            false,
            &probe,
            &RollbackPaths::with_root(&fs_root),
        ) {
            Ok(_) => return Err(std::io::Error::other("rollback should be blocked").into()),
            Err(err) => err,
        };

        fs::remove_dir_all(fs_root)?;
        assert!(
            matches!(err, Error::RollbackBlocked { reason } if reason.contains("web root is not empty"))
        );
        Ok(())
    }

    #[test]
    fn rollback_blocks_when_package_baseline_is_missing()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = rollback_fs_root(false, false)?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let err = match run_with_probe_and_paths(
            true,
            false,
            &probe,
            &RollbackPaths::with_root(&fs_root),
        ) {
            Ok(_) => return Err(std::io::Error::other("rollback should be blocked").into()),
            Err(err) => err,
        };

        fs::remove_dir_all(fs_root)?;
        assert!(
            matches!(err, Error::RollbackBlocked { reason } if reason.contains("missing preinstall package baseline"))
        );
        Ok(())
    }

    fn rollback_fs_root(
        web_root_has_content: bool,
        include_baseline: bool,
    ) -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html"))?;
        if web_root_has_content {
            fs::write(fs_root.join("home/g7/public_html/index.php"), "<?php")?;
        }
        fs::write(
            fs_root.join("etc/g7-installer/config.toml"),
            "domain = \"example.com\"\n",
        )?;
        let mut state = InstallerState::new("test-id".to_string(), "example.com".to_string());
        state.phase = "packages-installed".to_string();
        state.completed_steps = vec![
            "preflight-passed".to_string(),
            "packages-installed".to_string(),
            "service-verification-passed".to_string(),
        ];
        write_state_file(&fs_root.join(strip_root(STATE_PATH)), &state)?;
        write_owned_files(
            &fs_root.join(strip_root(OWNED_FILES_PATH)),
            &OwnedFiles {
                version: 1,
                files: vec![
                    "/etc/g7-installer/config.toml".to_string(),
                    STATE_PATH.to_string(),
                    OWNED_FILES_PATH.to_string(),
                ],
            },
        )?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            rollback_report_content(include_baseline),
        )?;
        Ok(fs_root)
    }

    fn rollback_report_content(include_baseline: bool) -> String {
        let baseline = if include_baseline {
            r#"
  "preinstall_package_checks": [
    { "name": "nginx", "status": "not-installed", "message": "absent" },
    { "name": "php8.3-fpm", "status": "not-installed", "message": "absent" }
  ],
"#
        } else {
            ""
        };

        format!(
            r#"{{
  "version": 1,
  "domain": "example.com",
  "phase": "packages-installed",
  "web_root": "/home/g7/public_html",
{baseline}
  "package_checks": [
    {{ "name": "nginx", "status": "pass", "message": "package installed" }},
    {{ "name": "php8.3-fpm", "status": "pass", "message": "package installed" }}
  ],
  "service_checks": [
    {{ "name": "nginx", "status": "pass", "message": "service is active" }},
    {{ "name": "php8.3-fpm", "status": "pass", "message": "service is active" }}
  ],
  "port_checks": []
}}
"#
        )
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        root.push(format!("g7-rollback-fs-root-{}", unique_temp_suffix()?));
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn unique_temp_suffix() -> std::result::Result<String, Box<dyn std::error::Error>> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{}-{nanos}-{count}", std::process::id()))
    }

    fn strip_root(path: &str) -> &str {
        match path.strip_prefix('/') {
            Some(stripped) => stripped,
            None => path,
        }
    }
}
