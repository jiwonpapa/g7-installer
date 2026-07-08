//! Server install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files before
//! performing server changes. Every applied package/service step must be
//! represented in `plan.rs`, `state.json`, `owned-files.json`, and the report.
//!
//! Current phase rule: package installation, basic service/port verification,
//! and the Nginx HTTP vhost are implemented. Database user creation, app file
//! deployment, Redis hardening, TLS issuance, and SSH changes belong to later
//! phases.

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::net::IpAddr;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::doctor::{self, DoctorCheckStatus};
use crate::commands::plan;
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
use g7_state::state::{InstallerPhase, InstallerState, STATE_PATH, write_state_file};
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
const PHP_READY_FILENAME: &str = "g7inst-ready.php";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub domain: String,
    pub deployment_mode: String,
    pub app_profile: String,
    pub app_profile_label: &'static str,
    pub app_document_root: String,
    pub web_server: String,
    pub php_version: String,
    pub database_engine: String,
    pub site_user: String,
    pub web_root_mode: String,
    pub web_root: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_from: Option<String>,
    pub smtp_encryption: Option<String>,
    pub dns_check: bool,
    pub security_profile: String,
    pub ssh_policy: String,
    pub phase: String,
    pub state_path: PathBuf,
    pub owned_files_path: PathBuf,
    pub owned_files: Vec<String>,
    pub completed_steps: Vec<String>,
    pub safety_checks: Vec<InstallCheck>,
    pub preinstall_package_checks: Vec<InstallCheck>,
    pub package_checks: Vec<InstallCheck>,
    pub service_checks: Vec<InstallCheck>,
    pub port_checks: Vec<InstallCheck>,
    pub network_checks: Vec<InstallCheck>,
    pub mail_checks: Vec<InstallCheck>,
    pub certbot_checks: Vec<InstallCheck>,
    pub vhost_checks: Vec<InstallCheck>,
    pub app_requirements: Vec<InstallCheck>,
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
    safety_checks: Vec<InstallCheck>,
    preinstall_package_checks: Vec<InstallCheck>,
    package_checks: Vec<InstallCheck>,
    service_checks: Vec<InstallCheck>,
    port_checks: Vec<InstallCheck>,
    network_checks: Vec<InstallCheck>,
    mail_checks: Vec<InstallCheck>,
    certbot_checks: Vec<InstallCheck>,
    vhost_checks: Vec<InstallCheck>,
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
    let site_user_password = options.site_user_password.clone();
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
    let mut owned_files = OwnedFiles {
        version: 1,
        files: owned_file_list.clone(),
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
    state.set_phase(InstallerPhase::Prepared);
    state.completed_steps = completed_steps.clone();

    let state_path = paths.resolve(STATE_PATH);
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    completed_steps.push("state-written".to_string());

    let mut apply_summary = match apply_package_phase(probe, &install_plan) {
        Ok(summary) => summary,
        Err(err) => {
            state.set_phase(InstallerPhase::PackageFailed);
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
    completed_steps.push("network-readiness-checked".to_string());
    completed_steps.push("mail-readiness-checked".to_string());
    completed_steps.push("certbot-readiness-checked".to_string());
    apply_summary.safety_checks = safety_checks(&install_plan, "packages-installed");
    state.set_phase(InstallerPhase::PackagesInstalled);
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

    let site_checks = match apply_site_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        site_user_password.as_deref(),
    ) {
        Ok(site_checks) => site_checks,
        Err(err) => {
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "site-provision",
                format!("Site account and web root setup failed: {err}"),
            )];
            state.set_phase(InstallerPhase::VhostFailed);
            state.completed_steps = completed_steps.clone();
            owned_files.files = owned_file_list.clone();
            write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                Error::FileWriteFailed {
                    path: OWNED_FILES_PATH.to_string(),
                    source,
                }
            })?;
            write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
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
                    &apply_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    };

    apply_summary.vhost_checks = site_checks;
    completed_steps.push("site-user-verified".to_string());
    if site_user_password.is_some() {
        completed_steps.push("site-user-password-set".to_string());
    }
    completed_steps.push("web-root-created".to_string());
    state.completed_steps = completed_steps.clone();
    owned_files.files = owned_file_list.clone();
    write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        paths,
        REPORT_PATH,
        &report_content(&install_plan, &state.phase, &apply_summary, None)?,
    )?;

    match apply_vhost_phase(probe, paths, &install_plan, &mut owned_file_list) {
        Ok(vhost_checks) => {
            if !vhost_checks.is_empty() {
                apply_summary.vhost_checks.extend(vhost_checks);
                completed_steps.push("vhost-written".to_string());
                completed_steps.push("vhost-enabled".to_string());
                completed_steps.push("nginx-config-tested".to_string());
                completed_steps.push("nginx-reloaded".to_string());
                completed_steps.push("http-smoke-passed".to_string());
                apply_summary.safety_checks = safety_checks(&install_plan, "vhost-enabled");
                state.set_phase(InstallerPhase::VhostEnabled);
                state.completed_steps = completed_steps.clone();
                owned_files.files = owned_file_list.clone();
                write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                    Error::FileWriteFailed {
                        path: OWNED_FILES_PATH.to_string(),
                        source,
                    }
                })?;
                write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
                write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
                    path: STATE_PATH.to_string(),
                    source,
                })?;
                write_existing_file(
                    paths,
                    REPORT_PATH,
                    &report_content(&install_plan, &state.phase, &apply_summary, None)?,
                )?;
            } else {
                apply_summary.safety_checks = safety_checks(&install_plan, "packages-installed");
                apply_summary
                    .vhost_checks
                    .extend(deferred_vhost_checks(&install_plan));
                write_existing_file(
                    paths,
                    REPORT_PATH,
                    &report_content(&install_plan, &state.phase, &apply_summary, None)?,
                )?;
            }
        }
        Err(err) => {
            apply_summary.safety_checks = safety_checks(&install_plan, "vhost-failed");
            apply_summary.vhost_checks = vec![InstallCheck::fail(
                "nginx-vhost",
                format!("Nginx vhost setup failed: {err}"),
            )];
            state.set_phase(InstallerPhase::VhostFailed);
            state.completed_steps = completed_steps.clone();
            owned_files.files = owned_file_list.clone();
            write_owned_files(&owned_files_path, &owned_files).map_err(|source| {
                Error::FileWriteFailed {
                    path: OWNED_FILES_PATH.to_string(),
                    source,
                }
            })?;
            write_existing_file(paths, ROLLBACK_PATH, &rollback_content(&owned_file_list))?;
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
                    &apply_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    }

    Ok(InstallReport {
        domain: state.domain,
        deployment_mode: install_plan.deployment_mode,
        app_profile: install_plan.app_profile,
        app_profile_label: install_plan.app_profile_label,
        app_document_root: install_plan.app_document_root,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        database_engine: install_plan.database_engine,
        site_user: install_plan.site_user,
        web_root_mode: install_plan.web_root_mode,
        web_root: install_plan.web_root,
        www_mode: install_plan.www_mode,
        redis_mode: install_plan.redis_mode,
        mail_mode: install_plan.mail_mode,
        smtp_host: install_plan.smtp_host,
        smtp_port: install_plan.smtp_port,
        smtp_from: install_plan.smtp_from,
        smtp_encryption: install_plan.smtp_encryption,
        dns_check: install_plan.dns_check_required,
        security_profile: install_plan.security_profile,
        ssh_policy: install_plan.ssh_policy,
        phase: state.phase,
        state_path,
        owned_files_path,
        owned_files: owned_files.files,
        completed_steps,
        safety_checks: apply_summary.safety_checks,
        preinstall_package_checks: apply_summary.preinstall_package_checks,
        package_checks: apply_summary.package_checks,
        service_checks: apply_summary.service_checks,
        port_checks: apply_summary.port_checks,
        network_checks: apply_summary.network_checks,
        mail_checks: apply_summary.mail_checks,
        certbot_checks: apply_summary.certbot_checks,
        vhost_checks: apply_summary.vhost_checks,
        app_requirements: app_requirements_to_checks(install_plan.app_requirements),
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
    let preinstall_package_checks = inspect_preinstall_packages(probe, &packages)?;

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
    let network_checks = verify_network_readiness(probe, plan);
    let mail_checks = verify_mail_readiness(probe, plan);
    let certbot_checks = verify_certbot_readiness(probe, plan, &service_checks);

    Ok(ApplySummary {
        safety_checks: Vec::new(),
        preinstall_package_checks,
        package_checks,
        service_checks,
        port_checks,
        network_checks,
        mail_checks,
        certbot_checks,
        vhost_checks: Vec::new(),
    })
}

fn apply_site_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    site_user_password: Option<&str>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    ensure_supported_web_root(plan)?;

    let user_exists = probe.user_exists(&plan.site_user).map_err(|err| {
        command_error("site-user-check", format!("id -u {}", plan.site_user), err)
    })?;
    if user_exists {
        checks.push(InstallCheck::pass(
            "site-user",
            format!("Linux account `{}` already exists.", plan.site_user),
        ));
    } else {
        let command = format!("useradd --create-home --shell /bin/bash {}", plan.site_user);
        let output = probe
            .create_login_user(&plan.site_user)
            .map_err(|err| command_error("site-user-create", &command, err))?;
        require_success("site-user-create", command, output)?;
        checks.push(InstallCheck::pass(
            "site-user",
            format!("Linux account `{}` was created.", plan.site_user),
        ));
    }

    if let Some(password) = site_user_password {
        let output = probe
            .set_login_password(&plan.site_user, password)
            .map_err(|err| command_error("site-user-password", "chpasswd", err))?;
        require_success("site-user-password", "chpasswd", output)?;
        checks.push(InstallCheck::pass(
            "site-user-password",
            format!(
                "Password was set for Linux account `{}` for SFTP/login use.",
                plan.site_user
            ),
        ));
    }

    require_empty_or_absent_dir(paths, &plan.web_root)?;
    require_empty_or_absent_dir(paths, &plan.app_document_root)?;
    create_owned_dir_if_absent(paths, &plan.web_root, owned)?;
    create_owned_dir_if_absent(paths, &plan.app_document_root, owned)?;
    checks.push(InstallCheck::pass(
        "web-root",
        format!("Created or verified {}.", plan.app_document_root),
    ));

    let ready_path = ready_probe_path(plan);
    write_new_file(paths, &ready_path, ready_probe_content(), owned)?;
    checks.push(InstallCheck::pass(
        "php-ready-probe",
        format!("Wrote temporary PHP smoke file {}.", ready_path),
    ));

    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {}", plan.web_root);
    let output = probe
        .chown_recursive(&owner_group, &plan.web_root)
        .map_err(|err| command_error("web-root-owner", &command, err))?;
    require_success("web-root-owner", command, output)?;
    let command = format!("chmod -R 0755 {}", plan.web_root);
    let output = probe
        .chmod_recursive("0755", &plan.web_root)
        .map_err(|err| command_error("web-root-permissions", &command, err))?;
    require_success("web-root-permissions", command, output)?;
    checks.push(InstallCheck::pass(
        "web-root-permissions",
        format!(
            "Set {} owner to {} and mode 0755.",
            plan.web_root, owner_group
        ),
    ));

    Ok(checks)
}

fn apply_vhost_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    if plan.web_server != "nginx" {
        return Ok(Vec::new());
    }

    let mut checks = Vec::new();
    write_new_file(
        paths,
        g7_system::nginx::G7_SITE_AVAILABLE,
        &nginx_vhost_content(plan),
        owned,
    )?;
    create_owned_symlink(
        paths,
        g7_system::nginx::G7_SITE_AVAILABLE,
        g7_system::nginx::G7_SITE_ENABLED,
        owned,
    )?;
    checks.push(InstallCheck::pass(
        "nginx-vhost",
        format!(
            "Wrote {} and enabled it at {}.",
            g7_system::nginx::G7_SITE_AVAILABLE,
            g7_system::nginx::G7_SITE_ENABLED
        ),
    ));

    let output = probe
        .nginx_config_test()
        .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
    require_success("nginx-configtest", "nginx -t", output)?;
    checks.push(InstallCheck::pass(
        "nginx-configtest",
        "nginx -t completed successfully.",
    ));

    let output = probe
        .reload_service(g7_system::nginx::SERVICE_NAME)
        .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
    require_success("nginx-reload", "systemctl reload nginx", output)?;
    checks.push(InstallCheck::pass(
        "nginx-reload",
        "Nginx was reloaded after vhost enable.",
    ));

    let smoke_host = primary_http_host(plan);
    match probe.http_host_smoke(&smoke_host) {
        Ok(true) => checks.push(InstallCheck::pass(
            "http-smoke",
            format!("HTTP smoke passed for Host: {smoke_host}."),
        )),
        Ok(false) => {
            return Err(Error::InstallVerificationFailed {
                checks: format!("HTTP smoke failed for Host: {smoke_host}"),
            });
        }
        Err(err) => {
            return Err(command_error(
                "http-smoke",
                format!("curl -H 'Host: {smoke_host}' http://127.0.0.1/"),
                err,
            ));
        }
    }

    Ok(checks)
}

fn safety_checks(plan: &plan::InstallPlan, phase: &str) -> Vec<InstallCheck> {
    let mut checks = Vec::new();
    if plan.deployment_mode == "public" {
        checks.push(InstallCheck {
            name: "provider-snapshot".to_string(),
            status: "warn".to_string(),
            message: "Vhost/DB/app/SSL 단계 전에는 Lightsail 스냅샷을 먼저 찍는 것을 권장합니다. 설치기는 provider snapshot 존재 여부를 API로 확인하지 않습니다.".to_string(),
        });
    } else {
        checks.push(InstallCheck {
            name: "provider-snapshot".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode does not require a provider snapshot.".to_string(),
        });
    }

    checks.push(InstallCheck {
        name: "rollback-boundary".to_string(),
        status: "info".to_string(),
        message: if phase == "packages-installed" {
            "Rollback is allowed before app/database/certificate content is created.".to_string()
        } else {
            "Rollback is allowed while web-root contents are installer-owned; it is blocked after app/database/certificate content appears. Restore the VPS snapshot for full-server recovery.".to_string()
        },
    });
    checks.push(InstallCheck {
        name: "resume-policy".to_string(),
        status: "info".to_string(),
        message: "Existing installer state blocks a fresh run. Retry through report/recovery, or reset only installer-owned paths before starting over.".to_string(),
    });
    checks
}

fn deferred_vhost_checks(plan: &plan::InstallPlan) -> Vec<InstallCheck> {
    vec![InstallCheck {
        name: "vhost-apply".to_string(),
        status: "deferred".to_string(),
        message: format!(
            "{} vhost apply is not implemented in this batch; package install report remains available.",
            plan.web_server
        ),
    }]
}

fn package_names(plan: &plan::InstallPlan) -> Vec<String> {
    plan.packages
        .iter()
        .flat_map(|package| package.name.split_whitespace())
        .map(ToOwned::to_owned)
        .collect()
}

fn ensure_supported_web_root(plan: &plan::InstallPlan) -> Result<()> {
    let web_root = Path::new(&plan.web_root);
    let app_root = Path::new(&plan.app_document_root);
    if !web_root.is_absolute()
        || web_root == Path::new("/")
        || !app_root.starts_with(web_root)
        || !reset_safe_web_root(&plan.web_root)
    {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "web root is outside the current reset safety policy: {}",
                plan.web_root
            ),
        });
    }
    Ok(())
}

fn reset_safe_web_root(path: &str) -> bool {
    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() == 4 && parts[1] == "home" && (parts[3] == "public_html" || parts[3] == "www") {
        return valid_path_segment(&parts[2]);
    }

    parts.len() == 4 && parts[1] == "var" && parts[2] == "www" && valid_path_segment(&parts[3])
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
}

fn require_empty_or_absent_dir(paths: &InstallPaths, path: &str) -> Result<()> {
    let target = paths.resolve(path);
    let metadata = match fs::metadata(&target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: path.to_string(),
                source,
            });
        }
    };

    if !metadata.is_dir() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{path} exists but is not a directory"),
        });
    }

    let mut entries = fs::read_dir(&target).map_err(|source| Error::FileReadFailed {
        path: path.to_string(),
        source,
    })?;
    if entries.next().is_some() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{path} exists but is not empty"),
        });
    }

    Ok(())
}

fn ready_probe_path(plan: &plan::InstallPlan) -> String {
    format!("{}/{}", plan.app_document_root, PHP_READY_FILENAME)
}

fn ready_probe_content() -> &'static str {
    "<?php\nheader('Content-Type: text/plain; charset=utf-8');\necho \"G7inst vhost ready\\n\";\n"
}

fn nginx_vhost_content(plan: &plan::InstallPlan) -> String {
    let app_hosts = nginx_app_hosts(plan);
    let redirect_blocks = nginx_redirect_blocks(plan);
    let php_socket = format!("/run/php/php{}-fpm.sock", plan.php_version);

    format!(
        "{redirect_blocks}server {{\n    listen 80;\n    listen [::]:80;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    access_log /var/log/nginx/g7-access.log;\n    error_log /var/log/nginx/g7-error.log;\n\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{php_socket};\n    }}\n\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

fn nginx_app_hosts(plan: &plan::InstallPlan) -> String {
    match plan.www_mode.as_str() {
        "redirect-to-www" if !plan.domain.starts_with("www.") => format!("www.{}", plan.domain),
        "redirect-to-root" | "none" => plan.domain.clone(),
        _ if !plan.domain.starts_with("www.") => format!("{} www.{}", plan.domain, plan.domain),
        _ => plan.domain.clone(),
    }
}

fn nginx_redirect_blocks(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name www.{domain};\n    return 301 http://{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        "redirect-to-www" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {domain};\n    return 301 http://www.{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        _ => String::new(),
    }
}

fn primary_http_host(plan: &plan::InstallPlan) -> String {
    if plan.www_mode == "redirect-to-www" && !plan.domain.starts_with("www.") {
        format!("www.{}", plan.domain)
    } else {
        plan.domain.clone()
    }
}

fn managed_services(plan: &plan::InstallPlan) -> Vec<String> {
    plan.services
        .iter()
        .filter(|service| package_phase_manages_service(&service.name, plan))
        .map(|service| service.name.clone())
        .collect()
}

fn package_phase_manages_service(service: &str, plan: &plan::InstallPlan) -> bool {
    service == plan.web_server
        || service == format!("php{}-fpm", plan.php_version)
        || service
            == if plan.database_engine == "mysql" {
                "mysql"
            } else {
                "mariadb"
            }
        || service == "certbot.timer"
        || service == "redis-server"
        || service == "postfix"
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

fn verify_network_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Vec<InstallCheck> {
    if !plan.dns_check_required {
        return vec![InstallCheck {
            name: "dns-public-ip".to_string(),
            status: "skipped".to_string(),
            message: "DNS/IP check is skipped for local-test mode or disabled dns_check."
                .to_string(),
        }];
    }

    let public_v4 = match probe.public_ipv4() {
        Ok(Some(address)) => {
            let mut checks = vec![InstallCheck::pass(
                "server-public-ipv4",
                format!("Detected server public IPv4: {address}"),
            )];
            checks.extend(verify_dns_hosts_v4(probe, plan, Some(address)));
            return checks;
        }
        Ok(None) => None,
        Err(err) => {
            return vec![InstallCheck::fail(
                "server-public-ipv4",
                format!("Could not detect server public IPv4: {err}"),
            )];
        }
    };

    verify_dns_hosts_v4(probe, plan, public_v4)
}

fn verify_dns_hosts_v4<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
    public_v4: Option<IpAddr>,
) -> Vec<InstallCheck> {
    let mut checks = Vec::new();

    for host in certificate_hosts(plan) {
        let name = if host == plan.domain {
            "dns-a".to_string()
        } else {
            format!("dns-a-{host}")
        };

        let Some(public_v4) = public_v4 else {
            checks.push(InstallCheck::fail(
                name,
                "Server public IPv4 is unavailable, so DNS A record cannot be compared.",
            ));
            continue;
        };

        match probe.dns_ipv4_records(&host) {
            Ok(records) if records.contains(&public_v4) => checks.push(InstallCheck::pass(
                name,
                format!(
                    "{} A record matches server public IPv4 {}.",
                    host, public_v4
                ),
            )),
            Ok(records) if records.is_empty() => checks.push(InstallCheck::fail(
                name,
                format!("{host} has no A record from system resolver."),
            )),
            Ok(records) => checks.push(InstallCheck::fail(
                name,
                format!(
                    "{host} A records {:?} do not include server public IPv4 {}.",
                    records, public_v4
                ),
            )),
            Err(err) => checks.push(InstallCheck::fail(
                name,
                format!("Could not resolve {host} A record: {err}"),
            )),
        }
    }

    checks
}

fn verify_mail_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Vec<InstallCheck> {
    match plan.mail_mode.as_str() {
        "none" => vec![InstallCheck {
            name: "mail-delivery".to_string(),
            status: "skipped".to_string(),
            message: "Mail delivery is disabled for this install.".to_string(),
        }],
        "smtp-relay" => {
            let host = plan.smtp_host.as_deref().unwrap_or("");
            let port = plan.smtp_port.unwrap_or(587);
            let status = probe.tcp_connect(host, port);
            vec![match status {
                Ok(true) => InstallCheck::pass(
                    "smtp-relay",
                    format!("SMTP relay {host}:{port} is reachable from this server."),
                ),
                Ok(false) => InstallCheck::fail(
                    "smtp-relay",
                    format!("SMTP relay {host}:{port} is not reachable from this server."),
                ),
                Err(err) => InstallCheck::fail(
                    "smtp-relay",
                    format!("Could not check SMTP relay {host}:{port}: {err}"),
                ),
            }]
        }
        "local-postfix" => vec![match probe.service_activity("postfix") {
            Ok(ServiceActivity::Active) => InstallCheck::pass(
                "local-postfix",
                "Postfix service is active for outbound-only local mail delivery.",
            ),
            Ok(ServiceActivity::Inactive) => {
                InstallCheck::fail("local-postfix", "Postfix service is inactive.")
            }
            Ok(ServiceActivity::NotFound) => {
                InstallCheck::fail("local-postfix", "Postfix service was not found.")
            }
            Ok(ServiceActivity::Unknown) => {
                InstallCheck::fail("local-postfix", "Postfix service state is unknown.")
            }
            Err(err) => InstallCheck::fail(
                "local-postfix",
                format!("Could not inspect Postfix service: {err}"),
            ),
        }],
        other => vec![InstallCheck::fail(
            "mail-delivery",
            format!("Unsupported mail mode in install report: {other}"),
        )],
    }
}

fn verify_certbot_readiness<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
    service_checks: &[InstallCheck],
) -> Vec<InstallCheck> {
    if plan.deployment_mode == "local-test" {
        return vec![InstallCheck {
            name: "certbot".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode skips Let's Encrypt certificates.".to_string(),
        }];
    }

    let mut checks = Vec::new();
    let timer_check = service_checks
        .iter()
        .find(|check| check.name == "certbot.timer")
        .cloned()
        .unwrap_or_else(|| {
            InstallCheck::fail(
                "certbot.timer",
                "certbot.timer was not checked during install.",
            )
        });
    checks.push(timer_check);

    let cert_path = Path::new("/etc/letsencrypt/live").join(&plan.domain);
    if !probe.path_exists(&cert_path) {
        checks.push(InstallCheck {
            name: "certbot-certificate".to_string(),
            status: "deferred".to_string(),
            message: "Certificate issuance waits for the vhost/app phase so HTTP-01 can be served safely.".to_string(),
        });
        checks.push(InstallCheck {
            name: "certbot-renew-dry-run".to_string(),
            status: "deferred".to_string(),
            message: "Renewal dry-run will run after a certificate exists.".to_string(),
        });
        return checks;
    }

    checks.push(InstallCheck::pass(
        "certbot-certificate",
        format!("Existing certificate directory found for {}.", plan.domain),
    ));

    match probe.certbot_renew_dry_run(&plan.domain) {
        Ok(output) if output.status == 0 => checks.push(InstallCheck::pass(
            "certbot-renew-dry-run",
            "certbot renew --dry-run completed successfully.",
        )),
        Ok(output) => checks.push(InstallCheck::fail(
            "certbot-renew-dry-run",
            format!(
                "certbot renew --dry-run failed with status {}: {}",
                output.status,
                output.stderr.trim()
            ),
        )),
        Err(err) => checks.push(InstallCheck::fail(
            "certbot-renew-dry-run",
            format!("Could not run certbot renew --dry-run: {err}"),
        )),
    }

    checks
}

fn certificate_hosts(plan: &plan::InstallPlan) -> Vec<String> {
    let mut hosts = vec![plan.domain.clone()];
    if plan.www_mode != "none" && !plan.domain.starts_with("www.") {
        hosts.push(format!("www.{}", plan.domain));
    }
    hosts
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

fn inspect_preinstall_packages<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
) -> Result<Vec<InstallCheck>> {
    packages
        .iter()
        .map(|package| match probe.package_status(package) {
            Ok(PackageStatus::Installed) => Ok(InstallCheck {
                name: package.clone(),
                status: "installed".to_string(),
                message: "package was already installed before G7 installer ran".to_string(),
            }),
            Ok(PackageStatus::NotInstalled) => Ok(InstallCheck {
                name: package.clone(),
                status: "not-installed".to_string(),
                message: "package was absent before G7 installer ran".to_string(),
            }),
            Ok(PackageStatus::Unknown) => Ok(InstallCheck {
                name: package.clone(),
                status: "unknown".to_string(),
                message: "package preinstall state is unknown".to_string(),
            }),
            Err(err) => Err(command_error(
                "package-baseline",
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

fn create_owned_dir_if_absent(
    paths: &InstallPaths,
    path: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    let existed = target.exists();
    fs::create_dir_all(&target).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })?;
    if !existed {
        owned.push(path.to_string());
    }
    Ok(())
}

fn create_owned_symlink(
    paths: &InstallPaths,
    source: &str,
    link: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let source_path = paths.resolve(source);
    let link_path = paths.resolve(link);
    #[cfg(unix)]
    {
        unix_fs::symlink(&source_path, &link_path).map_err(|source| Error::FileWriteFailed {
            path: link.to_string(),
            source,
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = source_path;
        return Err(Error::InstallVerificationFailed {
            checks: "symlink creation is supported only on unix platforms".to_string(),
        });
    }
    owned.push(link.to_string());
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
    content.push_str(&format!("app_profile = \"{}\"\n", plan.app_profile));
    content.push_str(&format!(
        "app_document_root = \"{}\"\n",
        plan.app_document_root
    ));
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
        "app_package": &plan.app_profile,
        "app_profile": &plan.app_profile,
        "app_profile_label": &plan.app_profile_label,
        "app_summary": &plan.app_summary,
        "app_document_root": &plan.app_document_root,
        "web_server": &plan.web_server,
        "php_version": &plan.php_version,
        "database": &plan.database_engine,
        "database_name": &plan.database_name,
        "database_user": &plan.database_user,
        "site_user": &plan.site_user,
        "web_root_mode": &plan.web_root_mode,
        "web_root": &plan.web_root,
        "www_mode": &plan.www_mode,
        "redis": &plan.redis_mode,
        "mail_mode": &plan.mail_mode,
        "smtp_host": &plan.smtp_host,
        "smtp_port": &plan.smtp_port,
        "smtp_from": &plan.smtp_from,
        "smtp_encryption": &plan.smtp_encryption,
        "dns_check": plan.dns_check_required,
        "security_profile": &plan.security_profile,
        "ssh_policy": &plan.ssh_policy,
        "safety_checks": checks_json(&summary.safety_checks),
        "preinstall_package_checks": checks_json(&summary.preinstall_package_checks),
        "package_checks": checks_json(&summary.package_checks),
        "service_checks": checks_json(&summary.service_checks),
        "port_checks": checks_json(&summary.port_checks),
        "network_checks": checks_json(&summary.network_checks),
        "mail_checks": checks_json(&summary.mail_checks),
        "certbot_checks": checks_json(&summary.certbot_checks),
        "vhost_checks": checks_json(&summary.vhost_checks),
        "app_requirements": requirements_json(&plan.app_requirements),
        "app_followup_steps": followup_steps_json(&plan.app_followup_steps),
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

fn requirements_json(
    requirements: &[crate::app_profile::AppRequirement],
) -> Vec<serde_json::Value> {
    requirements
        .iter()
        .map(|requirement| {
            serde_json::json!({
                "name": &requirement.name,
                "status": requirement.status,
                "message": &requirement.message,
            })
        })
        .collect()
}

fn followup_steps_json(steps: &[crate::app_profile::AppFollowupStep]) -> Vec<serde_json::Value> {
    steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "name": step.name,
                "description": step.description,
            })
        })
        .collect()
}

fn app_requirements_to_checks(
    requirements: Vec<crate::app_profile::AppRequirement>,
) -> Vec<InstallCheck> {
    requirements
        .into_iter()
        .map(|requirement| InstallCheck {
            name: requirement.name,
            status: requirement.status.to_string(),
            message: requirement.message,
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
        assert_eq!(report.phase, "vhost-enabled");
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
        assert!(fs_root.join("home/g7/public_html").exists());
        assert!(fs_root.join("home/g7/public_html/public").exists());
        assert!(
            fs_root
                .join("home/g7/public_html/public/g7inst-ready.php")
                .exists()
        );
        assert!(fs_root.join("etc/nginx/sites-available/g7.conf").exists());
        assert!(fs_root.join("etc/nginx/sites-enabled/g7.conf").exists());
        assert!(
            report
                .owned_files
                .contains(&"/home/g7/public_html".to_string())
        );
        assert!(
            report
                .completed_steps
                .contains(&"vhost-enabled".to_string())
        );
        assert!(
            report
                .package_checks
                .iter()
                .any(|check| { check.name == "nginx" && check.status == "pass" })
        );
        let report_json = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;
        assert!(report_json.contains("\"preinstall_package_checks\""));
        assert!(report_json.contains("\"status\": \"not-installed\""));
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
        assert!(
            report
                .network_checks
                .iter()
                .any(|check| { check.name == "server-public-ipv4" && check.status == "pass" })
        );
        assert!(
            report
                .network_checks
                .iter()
                .any(|check| { check.name == "dns-a" && check.status == "pass" })
        );
        assert!(
            report
                .mail_checks
                .iter()
                .any(|check| { check.name == "mail-delivery" && check.status == "skipped" })
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| { check.name == "certbot-certificate" && check.status == "deferred" })
        );
        assert!(
            report
                .safety_checks
                .iter()
                .any(|check| { check.name == "provider-snapshot" && check.status == "warn" })
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| { check.name == "http-smoke" && check.status == "pass" })
        );
        assert!(report_json.contains("\"network_checks\""));
        assert!(report_json.contains("\"mail_checks\""));
        assert!(report_json.contains("\"certbot_checks\""));
        assert!(report_json.contains("\"safety_checks\""));
        assert!(report_json.contains("\"vhost_checks\""));

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
                .network_checks
                .iter()
                .any(|check| { check.name == "dns-public-ip" && check.status == "skipped" })
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| { check.name == "certbot" && check.status == "skipped" })
        );
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
    fn install_reports_smtp_relay_reachability()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_from: Some("no-reply@example.com".to_string()),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.mail_mode, "smtp-relay");
        assert_eq!(report.smtp_host.as_deref(), Some("smtp.example.com"));
        assert_eq!(report.smtp_port, Some(587));
        assert!(
            report
                .mail_checks
                .iter()
                .any(|check| { check.name == "smtp-relay" && check.status == "pass" })
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_sets_site_account_password_when_requested()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            site_user_password: Some("0808dong!!".to_string()),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert!(
            report
                .completed_steps
                .contains(&"site-user-password-set".to_string())
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| check.name == "site-user-password" && check.status == "pass")
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
        fs::create_dir_all(fs_root.join("etc/nginx/sites-available"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let options = super::plan::PlanOptions {
            php_version: "8.5".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let install_plan =
            super::plan::build_with_options("example.com".to_string(), options.clone())?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _package in super::package_names(&install_plan) {
            runner.push_output(CommandOutput::failure(1, "no packages found"));
        }
        runner.push_output(CommandOutput::success("apt update ok\n"));
        runner.push_output(CommandOutput::success("nginx:\n  Candidate: 1\n"));
        runner.push_output(CommandOutput::success("php8.5-fpm:\n  Candidate: (none)\n"));
        let probe = SystemProbe::new(runner)
            .with_os_release_path(&os_release_path)
            .with_fs_root(&fs_root);
        let paths = InstallPaths::with_root(&fs_root);

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
        fs::create_dir_all(fs_root.join("etc/nginx/sites-available"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(uid));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let plan = super::plan::build_with_options(domain.to_string(), options.clone())?;
        push_successful_apply_outputs(&runner, &plan, options.site_user_password.is_some());

        Ok(SystemProbe::new(runner)
            .with_os_release_path(os_release_path)
            .with_fs_root(fs_root))
    }

    fn push_successful_apply_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
        site_password_set: bool,
    ) {
        let packages = super::package_names(install_plan);
        let services = super::managed_services(install_plan);
        let ports = super::managed_ports(install_plan);

        for _package in &packages {
            runner.push_output(CommandOutput::failure(1, "no packages found"));
        }
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
        push_successful_network_outputs(runner, install_plan);
        push_successful_mail_outputs(runner, install_plan);
        push_successful_site_and_vhost_outputs(runner, install_plan, site_password_set);
    }

    fn push_successful_network_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        if !install_plan.dns_check_required {
            return;
        }

        runner.push_output(CommandOutput::success("203.0.113.10\n"));
        for host in super::certificate_hosts(install_plan) {
            runner.push_output(CommandOutput::success(format!(
                "203.0.113.10 STREAM {host}\n203.0.113.10 DGRAM {host}\n"
            )));
        }
    }

    fn push_successful_mail_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        match install_plan.mail_mode.as_str() {
            "smtp-relay" => runner.push_output(CommandOutput::success("")),
            "local-postfix" => runner.push_output(CommandOutput::success("active\n")),
            _ => {}
        }
    }

    fn push_successful_site_and_vhost_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
        site_password_set: bool,
    ) {
        runner.push_output(CommandOutput::failure(1, "no such user"));
        runner.push_output(CommandOutput::success(""));
        if site_password_set {
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        if install_plan.web_server != "nginx" {
            return;
        }

        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
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
