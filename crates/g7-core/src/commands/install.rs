//! Server install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files before
//! performing server changes. Every applied package/service step must be
//! represented in `plan.rs`, `state.json`, `owned-files.json`, and the report.
//!
//! Current phase rule: package installation and basic service/port verification
//! are implemented. App web-root creation, web vhost activation, database user
//! creation, Redis hardening, and SSH changes belong to later phases.

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::doctor::{self, DoctorCheckStatus};
use crate::commands::plan;
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
use g7_state::state::{InstallerState, STATE_PATH, write_state_file};
use g7_system::SystemProbe;
use g7_system::command::CommandRunner;
use g7_system::package::PackageStatus;
use g7_system::port::PortStatus;
use g7_system::service::ServiceActivity;

const CONFIG_PATH: &str = "/etc/g7-installer/config.toml";
const ETC_DIR: &str = "/etc/g7-installer";
const LIB_DIR: &str = "/var/lib/g7-installer";
const LOG_DIR: &str = "/var/log/g7-installer";
const BACKUP_DIR: &str = "/var/backups/g7-installer";
const LOG_PATH: &str = "/var/log/g7-installer/install.log";
const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
const ROLLBACK_PATH: &str = "/var/lib/g7-installer/rollback.json";
const LOCAL_HOSTS_PATH: &str = "/etc/g7-installer/local-hosts.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub domain: String,
    pub deployment_mode: String,
    pub web_server: String,
    pub php_version: String,
    pub database_engine: String,
    pub site_user: String,
    pub web_root_mode: String,
    pub web_root: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub security_profile: String,
    pub ssh_policy: String,
    pub phase: String,
    pub state_path: PathBuf,
    pub owned_files_path: PathBuf,
    pub owned_files: Vec<String>,
    pub completed_steps: Vec<String>,
    pub package_checks: Vec<InstallCheck>,
    pub service_checks: Vec<InstallCheck>,
    pub port_checks: Vec<InstallCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallCheck {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl InstallCheck {
    fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "pass".to_string(),
            message: message.into(),
        }
    }

    fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "fail".to_string(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ApplySummary {
    package_checks: Vec<InstallCheck>,
    service_checks: Vec<InstallCheck>,
    port_checks: Vec<InstallCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    root: PathBuf,
}

impl InstallPaths {
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
}

pub fn run(domain: String, options: plan::PlanOptions) -> Result<InstallReport> {
    run_with_probe_and_paths(
        domain,
        options,
        &SystemProbe::real(),
        &InstallPaths::system(),
    )
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    domain: String,
    options: plan::PlanOptions,
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
) -> Result<InstallReport> {
    let install_plan = plan::build_with_options(domain, options)?;
    let doctor_report = doctor::run_with_probe(probe);

    require_root(&doctor_report)?;
    require_install_allowed(&doctor_report)?;

    let mut owned = Vec::new();
    create_owned_dir(paths, ETC_DIR, &mut owned)?;
    create_owned_dir(paths, LIB_DIR, &mut owned)?;
    create_owned_dir(paths, LOG_DIR, &mut owned)?;
    create_owned_dir(paths, BACKUP_DIR, &mut owned)?;

    write_new_file(
        paths,
        CONFIG_PATH,
        &config_content(&install_plan),
        &mut owned,
    )?;
    write_new_file(paths, LOG_PATH, "G7 installer prepared.\n", &mut owned)?;
    write_new_file(paths, ROLLBACK_PATH, &rollback_content(&owned), &mut owned)?;
    write_new_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, "prepared", &ApplySummary::default(), None)?,
        &mut owned,
    )?;
    let mut optional_steps = Vec::new();
    if install_plan.deployment_mode == "local-test" {
        write_new_file(
            paths,
            LOCAL_HOSTS_PATH,
            &local_hosts_content(&install_plan.domain),
            &mut owned,
        )?;
        optional_steps.push("local-hosts-suggestion-written".to_string());
    }

    let mut owned_file_list = owned.clone();
    owned_file_list.push(STATE_PATH.to_string());
    owned_file_list.push(OWNED_FILES_PATH.to_string());
    let owned_files = OwnedFiles {
        version: 1,
        files: owned_file_list,
    };

    let owned_files_path = paths.resolve(OWNED_FILES_PATH);
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;

    let mut completed_steps = vec![
        "preflight-passed".to_string(),
        "directories-created".to_string(),
        "config-written".to_string(),
        "log-created".to_string(),
        "rollback-prepared".to_string(),
        "problem-report-prepared".to_string(),
    ];
    completed_steps.extend(optional_steps);
    completed_steps.push("owned-files-written".to_string());
    let mut state = InstallerState::new(
        install_id(&install_plan.domain),
        install_plan.domain.clone(),
    );
    state.phase = "prepared".to_string();
    state.completed_steps = completed_steps.clone();

    let state_path = paths.resolve(STATE_PATH);
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    completed_steps.push("state-written".to_string());

    let apply_summary = match apply_package_phase(probe, &install_plan) {
        Ok(summary) => summary,
        Err(err) => {
            state.phase = "package-failed".to_string();
            state.completed_steps = completed_steps.clone();
            write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                path: STATE_PATH.to_string(),
                source,
            })?;
            write_existing_file(
                paths,
                REPORT_PATH,
                &report_content(
                    &install_plan,
                    &state.phase,
                    &ApplySummary::default(),
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    };

    completed_steps.push("apt-updated".to_string());
    completed_steps.push("package-candidates-checked".to_string());
    completed_steps.push("packages-installed".to_string());
    completed_steps.push("services-enabled".to_string());
    completed_steps.push("package-verification-passed".to_string());
    completed_steps.push("service-verification-passed".to_string());
    completed_steps.push("port-verification-passed".to_string());
    state.phase = "packages-installed".to_string();
    state.completed_steps = completed_steps.clone();
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, &state.phase, &apply_summary, None)?,
    )?;

    Ok(InstallReport {
        domain: state.domain,
        deployment_mode: install_plan.deployment_mode,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        database_engine: install_plan.database_engine,
        site_user: install_plan.site_user,
        web_root_mode: install_plan.web_root_mode,
        web_root: install_plan.web_root,
        www_mode: install_plan.www_mode,
        redis_mode: install_plan.redis_mode,
        mail_mode: install_plan.mail_mode,
        security_profile: install_plan.security_profile,
        ssh_policy: install_plan.ssh_policy,
        phase: state.phase,
        state_path,
        owned_files_path,
        owned_files: owned_files.files,
        completed_steps,
        package_checks: apply_summary.package_checks,
        service_checks: apply_summary.service_checks,
        port_checks: apply_summary.port_checks,
    })
}

fn require_root(report: &doctor::DoctorReport) -> Result<()> {
    let root = report
        .checks
        .iter()
        .any(|check| check.name == "privilege" && check.status == DoctorCheckStatus::Pass);

    if root {
        Ok(())
    } else {
        Err(Error::PrivilegeRequired)
    }
}

fn require_install_allowed(report: &doctor::DoctorReport) -> Result<()> {
    if report.install_allowed {
        return Ok(());
    }

    let checks = report
        .checks
        .iter()
        .filter(|check| {
            matches!(
                check.status,
                DoctorCheckStatus::Fail | DoctorCheckStatus::Pending
            )
        })
        .map(|check| check.name)
        .collect::<Vec<_>>()
        .join(", ");

    Err(Error::InstallBlocked { checks })
}

fn apply_package_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<ApplySummary> {
    let packages = package_names(plan);
    let services = managed_services(plan);
    let ports = managed_ports(plan);

    let output = probe
        .apt_update()
        .map_err(|err| command_error("apt-update", "apt-get update", err))?;
    require_success("apt-update", "apt-get update", output)?;

    for package in &packages {
        let available = probe.apt_candidate_available(package).map_err(|err| {
            command_error("apt-candidate", format!("apt-cache policy {package}"), err)
        })?;
        if !available {
            return Err(Error::PackageUnavailable {
                package: package.clone(),
            });
        }
    }

    let install_command = format!("apt-get install -y {}", packages.join(" "));
    let output = probe
        .apt_install(&packages)
        .map_err(|err| command_error("apt-install", &install_command, err))?;
    require_success("apt-install", install_command, output)?;

    for service in &services {
        let command = format!("systemctl enable --now {service}");
        let output = probe
            .enable_service_now(service)
            .map_err(|err| command_error("service-enable", &command, err))?;
        require_success("service-enable", command, output)?;
    }

    let package_checks = verify_packages(probe, &packages)?;
    let service_checks = verify_services(probe, &services)?;
    let port_checks = verify_ports(probe, &ports)?;
    require_checks_passed(&package_checks, &service_checks, &port_checks)?;

    Ok(ApplySummary {
        package_checks,
        service_checks,
        port_checks,
    })
}

fn package_names(plan: &plan::InstallPlan) -> Vec<String> {
    plan.packages
        .iter()
        .flat_map(|package| package.name.split_whitespace())
        .map(ToOwned::to_owned)
        .collect()
}

fn managed_services(plan: &plan::InstallPlan) -> Vec<String> {
    plan.services
        .iter()
        .filter(|service| !service.name.starts_with("g7-"))
        .map(|service| service.name.clone())
        .collect()
}

fn managed_ports(plan: &plan::InstallPlan) -> Vec<u16> {
    plan.ports
        .iter()
        .filter_map(|port| match port.port {
            80 | 3306 | 6379 => Some(port.port),
            _ => None,
        })
        .collect()
}

fn verify_packages<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
) -> Result<Vec<InstallCheck>> {
    packages
        .iter()
        .map(|package| match probe.package_status(package) {
            Ok(PackageStatus::Installed) => Ok(InstallCheck::pass(package, "package installed")),
            Ok(PackageStatus::NotInstalled) => {
                Ok(InstallCheck::fail(package, "package is not installed"))
            }
            Ok(PackageStatus::Unknown) => {
                Ok(InstallCheck::fail(package, "package status is unknown"))
            }
            Err(err) => Err(command_error(
                "package-verify",
                format!("dpkg-query {package}"),
                err,
            )),
        })
        .collect()
}

fn verify_services<R: CommandRunner>(
    probe: &SystemProbe<R>,
    services: &[String],
) -> Result<Vec<InstallCheck>> {
    services
        .iter()
        .map(|service| match probe.service_activity(service) {
            Ok(ServiceActivity::Active) => Ok(InstallCheck::pass(service, "service is active")),
            Ok(ServiceActivity::Inactive) => Ok(InstallCheck::fail(service, "service is inactive")),
            Ok(ServiceActivity::NotFound) => {
                Ok(InstallCheck::fail(service, "service was not found"))
            }
            Ok(ServiceActivity::Unknown) => {
                Ok(InstallCheck::fail(service, "service state is unknown"))
            }
            Err(err) => Err(command_error(
                "service-verify",
                format!("systemctl is-active {service}"),
                err,
            )),
        })
        .collect()
}

fn verify_ports<R: CommandRunner>(
    probe: &SystemProbe<R>,
    ports: &[u16],
) -> Result<Vec<InstallCheck>> {
    ports
        .iter()
        .map(|port| match probe.tcp_port_status(*port) {
            Ok(PortStatus::InUse) => Ok(InstallCheck::pass(
                port.to_string(),
                "TCP port is listening",
            )),
            Ok(PortStatus::Free) => Ok(InstallCheck::fail(
                port.to_string(),
                "TCP port is not listening",
            )),
            Ok(PortStatus::Unknown) => Ok(InstallCheck::fail(
                port.to_string(),
                "TCP port status is unknown",
            )),
            Err(err) => Err(command_error(
                "port-verify",
                format!("ss -H -tulpn for port {port}"),
                err,
            )),
        })
        .collect()
}

fn require_checks_passed(
    package_checks: &[InstallCheck],
    service_checks: &[InstallCheck],
    port_checks: &[InstallCheck],
) -> Result<()> {
    let failed = package_checks
        .iter()
        .chain(service_checks)
        .chain(port_checks)
        .filter(|check| check.status == "fail")
        .map(|check| format!("{}: {}", check.name, check.message))
        .collect::<Vec<String>>();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(Error::InstallVerificationFailed {
            checks: failed.join(", "),
        })
    }
}

fn require_success(
    step: &'static str,
    command: impl Into<String>,
    output: g7_system::command::CommandOutput,
) -> Result<()> {
    if output.status == 0 {
        Ok(())
    } else {
        Err(Error::InstallCommandFailed {
            step,
            command: command.into(),
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn command_error(step: &'static str, command: impl Into<String>, err: impl ToString) -> Error {
    Error::InstallCommandFailed {
        step,
        command: command.into(),
        status: 128,
        stdout: String::new(),
        stderr: err.to_string(),
    }
}

fn create_owned_dir(paths: &InstallPaths, path: &str, owned: &mut Vec<String>) -> Result<()> {
    let target = paths.resolve(path);
    fs::create_dir_all(&target).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })?;
    owned.push(path.to_string());
    Ok(())
}

fn write_new_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .map_err(|source| Error::FileWriteFailed {
            path: path.to_string(),
            source,
        })?;

    file.write_all(content.as_bytes())
        .map_err(|source| Error::FileWriteFailed {
            path: path.to_string(),
            source,
        })?;
    owned.push(path.to_string());
    Ok(())
}

fn write_existing_file(paths: &InstallPaths, path: &str, content: &str) -> Result<()> {
    let target = paths.resolve(path);
    fs::write(&target, content).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })
}

fn config_content(plan: &plan::InstallPlan) -> String {
    let mut content = String::new();
    content.push_str(&format!("domain = \"{}\"\n", plan.domain));
    content.push_str(&format!("deployment_mode = \"{}\"\n", plan.deployment_mode));
    content.push_str("phase = \"prepared\"\n");
    content.push_str(&format!("web_server = \"{}\"\n", plan.web_server));
    content.push_str(&format!("php_version = \"{}\"\n", plan.php_version));
    content.push_str(&format!("database = \"{}\"\n", plan.database_engine));
    content.push_str(&format!("database_name = \"{}\"\n", plan.database_name));
    content.push_str(&format!("database_user = \"{}\"\n", plan.database_user));
    content.push_str(&format!(
        "database_password_policy = \"{}\"\n",
        plan.database_password_policy
    ));
    content.push_str(&format!("site_user = \"{}\"\n", plan.site_user));
    content.push_str(&format!("web_root_mode = \"{}\"\n", plan.web_root_mode));
    content.push_str(&format!("web_root = \"{}\"\n", plan.web_root));
    content.push_str(&format!("www_mode = \"{}\"\n", plan.www_mode));
    content.push_str(&format!("redis = \"{}\"\n", plan.redis_mode));
    content.push_str(&format!("mail_mode = \"{}\"\n", plan.mail_mode));
    content.push_str(&format!(
        "security_profile = \"{}\"\n",
        plan.security_profile
    ));
    content.push_str(&format!("ssh_policy = \"{}\"\n", plan.ssh_policy));
    content.push_str(&format!("rollback = {}\n", plan.rollback_enabled));
    content.push_str(&format!("preserve_config = {}\n", plan.preserve_config));
    content.push_str(&format!("dns_check = {}\n", plan.dns_check_required));

    if let Some(host) = &plan.smtp_host {
        content.push_str(&format!("smtp_host = \"{host}\"\n"));
    }
    if let Some(port) = plan.smtp_port {
        content.push_str(&format!("smtp_port = {port}\n"));
    }
    if let Some(from) = &plan.smtp_from {
        content.push_str(&format!("smtp_from = \"{from}\"\n"));
    }
    if let Some(encryption) = &plan.smtp_encryption {
        content.push_str(&format!("smtp_encryption = \"{encryption}\"\n"));
    }

    content
}

fn rollback_content(owned: &[String]) -> String {
    let files = owned
        .iter()
        .map(|path| format!("    \"{path}\""))
        .collect::<Vec<String>>()
        .join(",\n");

    format!("{{\n  \"version\": 1,\n  \"created_paths\": [\n{files}\n  ]\n}}\n")
}

fn report_content(
    plan: &plan::InstallPlan,
    phase: &str,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<String> {
    let value = serde_json::json!({
        "version": 1,
        "domain": &plan.domain,
        "phase": phase,
        "deployment_mode": &plan.deployment_mode,
        "web_server": &plan.web_server,
        "php_version": &plan.php_version,
        "database": &plan.database_engine,
        "site_user": &plan.site_user,
        "web_root": &plan.web_root,
        "security_profile": &plan.security_profile,
        "ssh_policy": &plan.ssh_policy,
        "package_checks": checks_json(&summary.package_checks),
        "service_checks": checks_json(&summary.service_checks),
        "port_checks": checks_json(&summary.port_checks),
        "problem": problem,
    });
    let mut payload =
        serde_json::to_string_pretty(&value).map_err(|source| Error::FileWriteFailed {
            path: REPORT_PATH.to_string(),
            source: io::Error::other(source),
        })?;
    payload.push('\n');
    Ok(payload)
}

fn checks_json(checks: &[InstallCheck]) -> Vec<serde_json::Value> {
    checks
        .iter()
        .map(|check| {
            serde_json::json!({
                "name": &check.name,
                "status": &check.status,
                "message": &check.message,
            })
        })
        .collect()
}

fn local_hosts_content(domain: &str) -> String {
    format!(
        "# Add this on the test client if {domain} is not resolvable yet:\n127.0.0.1 {domain}\n"
    )
}

fn install_id(domain: &str) -> String {
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };

    format!("g7-{domain}-{seconds}")
}

#[cfg(test)]
mod tests {
    use super::{InstallPaths, run_with_probe_and_paths};
    use crate::Error;
    use g7_state::owned_files::OWNED_FILES_PATH;
    use g7_state::state::STATE_PATH;
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn install_writes_prepared_state_and_owned_files()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths(
            "Example.COM.".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        )?;

        assert_eq!(report.domain, "example.com");
        assert_eq!(report.deployment_mode, "public");
        assert_eq!(report.web_server, "nginx");
        assert_eq!(report.php_version, "8.3");
        assert_eq!(report.database_engine, "mysql");
        assert_eq!(report.site_user, "g7");
        assert_eq!(report.web_root_mode, "public-html");
        assert_eq!(report.web_root, "/home/g7/public_html");
        assert_eq!(report.redis_mode, "enable");
        assert_eq!(report.security_profile, "standard");
        assert_eq!(report.ssh_policy, "audit-only");
        assert_eq!(report.phase, "packages-installed");
        assert!(fs_root.join("etc/g7-installer/config.toml").exists());
        let config = fs::read_to_string(fs_root.join("etc/g7-installer/config.toml"))?;
        assert!(config.contains("deployment_mode = \"public\""));
        assert!(config.contains("web_server = \"nginx\""));
        assert!(config.contains("php_version = \"8.3\""));
        assert!(config.contains("database = \"mysql\""));
        assert!(config.contains("database_password_policy = \"generate-random-store-root-only\""));
        assert!(config.contains("site_user = \"g7\""));
        assert!(config.contains("web_root = \"/home/g7/public_html\""));
        assert!(config.contains("www_mode = \"redirect-to-root\""));
        assert!(config.contains("redis = \"enable\""));
        assert!(config.contains("security_profile = \"standard\""));
        assert!(config.contains("ssh_policy = \"audit-only\""));
        assert!(fs_root.join("var/lib/g7-installer/rollback.json").exists());
        assert!(fs_root.join("var/log/g7-installer/report.json").exists());
        assert!(fs_root.join("var/backups/g7-installer").exists());
        assert!(fs_root.join(strip_root(STATE_PATH)).exists());
        assert!(fs_root.join(strip_root(OWNED_FILES_PATH)).exists());
        assert!(!fs_root.join("home/g7/public_html").exists());
        assert!(
            !report
                .owned_files
                .contains(&"/home/g7/public_html".to_string())
        );
        assert!(
            report
                .completed_steps
                .contains(&"packages-installed".to_string())
        );
        assert!(
            report
                .package_checks
                .iter()
                .any(|check| { check.name == "nginx" && check.status == "pass" })
        );
        assert!(
            report
                .service_checks
                .iter()
                .any(|check| { check.name == "nginx" && check.status == "pass" })
        );
        assert!(
            report
                .port_checks
                .iter()
                .any(|check| { check.name == "80" && check.status == "pass" })
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_requires_root() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let probe = clean_probe_with_uid(&os_release_path, &fs_root, "1000\n")?;
        let paths = InstallPaths::with_root(&fs_root);

        let err = match run_with_probe_and_paths(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        ) {
            Ok(_) => return Err(std::io::Error::other("install should require root").into()),
            Err(err) => err,
        };

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(matches!(err, Error::PrivilegeRequired));
        Ok(())
    }

    #[test]
    fn install_blocks_when_fresh_server_gate_fails()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);

        let err = match run_with_probe_and_paths(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        ) {
            Ok(_) => return Err(std::io::Error::other("install should be blocked").into()),
            Err(err) => err,
        };

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(matches!(err, Error::InstallBlocked { .. }));
        Ok(())
    }

    #[test]
    fn install_writes_local_hosts_hint_for_local_test()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            local_test: true,
            dns_check: true,
            www_mode: "none".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "g7-test.local", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report =
            run_with_probe_and_paths("g7-test.local".to_string(), options, &probe, &paths)?;

        let local_hosts = fs::read_to_string(fs_root.join("etc/g7-installer/local-hosts.txt"))?;
        assert_eq!(report.deployment_mode, "local-test");
        assert!(local_hosts.contains("127.0.0.1 g7-test.local"));
        assert!(
            report
                .completed_steps
                .contains(&"local-hosts-suggestion-written".to_string())
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_fails_before_install_when_package_candidate_is_missing()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("apt update ok\n"));
        runner.push_output(CommandOutput::success("nginx:\n  Candidate: 1\n"));
        runner.push_output(CommandOutput::success("php8.5-fpm:\n  Candidate: (none)\n"));
        let probe = SystemProbe::new(runner)
            .with_os_release_path(&os_release_path)
            .with_fs_root(&fs_root);
        let paths = InstallPaths::with_root(&fs_root);
        let options = super::plan::PlanOptions {
            php_version: "8.5".to_string(),
            ..super::plan::PlanOptions::default()
        };

        let err = match run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)
        {
            Ok(_) => {
                return Err(std::io::Error::other("missing package should fail").into());
            }
            Err(err) => err,
        };

        let report = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;
        let state = fs::read_to_string(fs_root.join(strip_root(STATE_PATH)))?;

        assert!(matches!(err, Error::PackageUnavailable { package } if package == "php8.5-fpm"));
        assert!(report.contains("\"phase\": \"package-failed\""));
        assert!(report.contains("php8.5-fpm"));
        assert!(state.contains("\"phase\": \"package-failed\""));

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    fn clean_root_probe(
        os_release_path: &Path,
        fs_root: &Path,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid(os_release_path, fs_root, "0\n")
    }

    fn clean_probe_with_uid(
        os_release_path: &Path,
        fs_root: &Path,
        uid: &str,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid_for_options(
            os_release_path,
            fs_root,
            uid,
            "example.com",
            &super::plan::PlanOptions::default(),
        )
    }

    fn clean_root_probe_for_options(
        os_release_path: &Path,
        fs_root: &Path,
        domain: &str,
        options: &super::plan::PlanOptions,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid_for_options(os_release_path, fs_root, "0\n", domain, options)
    }

    fn clean_probe_with_uid_for_options(
        os_release_path: &Path,
        fs_root: &Path,
        uid: &str,
        domain: &str,
        options: &super::plan::PlanOptions,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(uid));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let plan = super::plan::build_with_options(domain.to_string(), options.clone())?;
        push_successful_apply_outputs(&runner, &plan);

        Ok(SystemProbe::new(runner)
            .with_os_release_path(os_release_path)
            .with_fs_root(fs_root))
    }

    fn push_successful_apply_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        let packages = super::package_names(install_plan);
        let services = super::managed_services(install_plan);
        let ports = super::managed_ports(install_plan);

        runner.push_output(CommandOutput::success("apt update ok\n"));
        for package in &packages {
            runner.push_output(CommandOutput::success(format!(
                "{package}:\n  Candidate: 1\n"
            )));
        }
        runner.push_output(CommandOutput::success("apt install ok\n"));
        for _service in &services {
            runner.push_output(CommandOutput::success(""));
        }
        for _package in &packages {
            runner.push_output(CommandOutput::success("install ok installed"));
        }
        for _service in &services {
            runner.push_output(CommandOutput::success("active\n"));
        }
        for port in &ports {
            runner.push_output(CommandOutput::success(format!(
                "tcp LISTEN 0 4096 127.0.0.1:{port} 0.0.0.0:*\n"
            )));
        }
    }

    fn write_temp_os_release() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut path = std::env::temp_dir();
        path.push(format!("g7-install-os-release-{}", unique_temp_suffix()?));
        fs::write(
            &path,
            "ID=ubuntu\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\n",
        )?;
        Ok(path)
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        root.push(format!("g7-install-fs-root-{}", unique_temp_suffix()?));
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
