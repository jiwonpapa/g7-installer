//! Server install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files before
//! performing server changes. Every applied package/service step must be
//! represented in `plan.rs`, `state.json`, `owned-files.json`, and the report.
//!
//! Current phase rule: package installation, site account/web root creation,
//! Nginx/Apache vhost setup, PHP-FPM/DB tuning, DB user creation, TLS vhost
//! mutation, app source handoff, and setup reporting are implemented. Riskier
//! shared-server mutations such as firewall changes remain deferred until their
//! rollback surface is explicit.

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::net::IpAddr;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::doctor::{self, DoctorCheckStatus};
use crate::commands::plan;
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
use g7_state::state::{InstallerPhase, InstallerState, STATE_PATH, write_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandRunner, CommandSpec};
use g7_system::database::DatabaseEngine;
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
const BACKUP_MANIFEST_PATH: &str = "/var/backups/g7-installer/manifest.json";
const LOCAL_HOSTS_PATH: &str = "/etc/g7-installer/local-hosts.txt";
const PHP_READY_FILENAME: &str = "g7inst-ready.php";
const SECRETS_PATH: &str = "/etc/g7-installer/secrets.toml";
const SETUP_GUIDE_PATH: &str = "/var/log/g7-installer/setup-guide.md";
const GNUBOARD7_REPO_URL: &str = "https://github.com/gnuboard/g7.git";
const GNUBOARD7_RELEASE_REF: &str = "7.0.1";
const LARAVEL_REPO_URL: &str = "https://github.com/laravel/laravel.git";
const LARAVEL_RELEASE_REF: &str = "12.x";
const APP_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source";
const GNUBOARD7_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source/gnuboard7";
const LARAVEL_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source/laravel";
const WORDPRESS_DOWNLOAD_URL: &str = "https://wordpress.org/latest.zip";
const WORDPRESS_ARCHIVE_PATH: &str = "/var/lib/g7-installer/app-source/wordpress.zip";
const WORDPRESS_EXTRACT_DIR: &str = "/var/lib/g7-installer/app-source/wordpress-extract";
const WORDPRESS_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source/wordpress-extract/wordpress";
const CERTBOT_HTTP01_CHALLENGE_DIR: &str = ".well-known/acme-challenge";
const CERTBOT_HTTP01_SMOKE_FILENAME: &str = "g7inst-certbot-http01-smoke.txt";
const CERTBOT_HTTP01_SMOKE_CONTENT: &str = "g7-installer-certbot-http01-ok\n";
const SWAP_FILE_PATH: &str = "/swapfile";
const SWAP_UNIT_PATH: &str = "/etc/systemd/system/swapfile.swap";
const SWAP_SYSCTL_PATH: &str = "/etc/sysctl.d/99-g7-installer-swap.conf";
const GNUBOARD7_DRIVER_SETTINGS_PATH: &str = "storage/app/settings/drivers.json";
const GNUBOARD7_REQUIRED_FILES: &[&str] = &[
    "artisan",
    "composer.json",
    "public/index.php",
    "public/build/core/template-engine.min.js",
];
const LARAVEL_REQUIRED_FILES: &[&str] = &["artisan", "composer.json", "public/index.php"];
const WORDPRESS_REQUIRED_FILES: &[&str] = &["wp-settings.php", "wp-admin/install.php"];
const WORDPRESS_REQUIRED_DIRS: &[&str] = &["wp-content"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub domain: String,
    pub deployment_mode: String,
    pub app_profile: String,
    pub app_profile_label: &'static str,
    pub app_document_root: String,
    pub web_server: String,
    pub php_version: String,
    pub php_source: String,
    pub database_engine: String,
    pub database_name: String,
    pub database_user: String,
    pub database_password_policy: &'static str,
    pub site_user: String,
    pub web_root_mode: String,
    pub web_root: String,
    pub app_url: String,
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
    pub runtime_checks: Vec<InstallCheck>,
    pub database_checks: Vec<InstallCheck>,
    pub firewall_checks: Vec<InstallCheck>,
    pub mail_checks: Vec<InstallCheck>,
    pub certbot_checks: Vec<InstallCheck>,
    pub vhost_checks: Vec<InstallCheck>,
    pub app_checks: Vec<InstallCheck>,
    pub setup_guide_path: PathBuf,
    pub backup_manifest_path: PathBuf,
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

    fn manual(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "manual".to_string(),
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
    runtime_checks: Vec<InstallCheck>,
    database_checks: Vec<InstallCheck>,
    firewall_checks: Vec<InstallCheck>,
    mail_checks: Vec<InstallCheck>,
    certbot_checks: Vec<InstallCheck>,
    vhost_checks: Vec<InstallCheck>,
    app_checks: Vec<InstallCheck>,
}

#[derive(Debug)]
struct PackagePhaseFailure {
    error: Error,
    summary: ApplySummary,
    completed_steps: Vec<String>,
}

struct ProgressContext<'a> {
    paths: &'a InstallPaths,
    state_path: &'a Path,
    owned_files_path: &'a Path,
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
    let database_password = options.database_password.clone();
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
    let progress = ProgressContext {
        paths,
        state_path: &state_path,
        owned_files_path: &owned_files_path,
    };
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    completed_steps.push("state-written".to_string());

    let mut apply_summary = match apply_package_phase(probe, &install_plan) {
        Ok(summary) => summary,
        Err(failure) => {
            let failure = *failure;
            let err = failure.error;
            let mut failed_summary = failure.summary;
            failed_summary.safety_checks = safety_checks(&install_plan, "package-failed");
            completed_steps.extend(failure.completed_steps);
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
                    &failed_summary,
                    Some(&err.to_string()),
                )?,
            )?;
            return Err(err);
        }
    };

    completed_steps.push("apt-updated".to_string());
    if install_plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
        completed_steps.push("php-apt-source-added".to_string());
        completed_steps.push("apt-updated-after-php-source".to_string());
    }
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
                completed_steps.push(format!("{}-config-tested", web_service_name(&install_plan)));
                completed_steps.push(format!("{}-reloaded", web_service_name(&install_plan)));
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
                "webserver-vhost",
                format!("Web server vhost setup failed: {err}"),
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

    match apply_runtime_phase(probe, paths, &install_plan, &mut owned_file_list) {
        Ok(runtime_checks) => {
            apply_summary.runtime_checks = runtime_checks;
            if let Some(message) = blocking_runtime_failure(&apply_summary.runtime_checks) {
                state.completed_steps = completed_steps.clone();
                persist_progress(
                    &progress,
                    &mut owned_files,
                    &owned_file_list,
                    &state,
                    &install_plan,
                    &apply_summary,
                    Some(&message),
                )?;
                return Err(Error::InstallVerificationFailed { checks: message });
            }
            completed_steps.push("php-fpm-config-written".to_string());
            completed_steps.push("php-runtime-config-written".to_string());
            completed_steps.push("php-runtime-diagnostics-passed".to_string());
            completed_steps.push(format!(
                "{}-runtime-config-written",
                web_service_name(&install_plan)
            ));
            completed_steps.push(format!(
                "{}-runtime-reloaded",
                web_service_name(&install_plan)
            ));
            apply_summary.safety_checks = safety_checks(&install_plan, "runtime-configured");
            state.set_phase(InstallerPhase::RuntimeConfigured);
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.runtime_checks = vec![InstallCheck::fail(
                "runtime-config",
                format!("Runtime configuration failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    match apply_database_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        database_password.as_deref(),
    ) {
        Ok(database_checks) => {
            apply_summary.database_checks = database_checks;
            completed_steps.push("database-runtime-configured".to_string());
            completed_steps.push("database-secret-written".to_string());
            completed_steps.push("database-created".to_string());
            completed_steps.push("database-user-created".to_string());
            apply_summary.safety_checks = safety_checks(&install_plan, "database-configured");
            state.set_phase(InstallerPhase::DatabaseConfigured);
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.database_checks = vec![InstallCheck::fail(
                "database-config",
                format!("Database configuration failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    let (firewall_checks, mail_checks, certbot_checks, app_checks) =
        apply_post_database_guidance(&install_plan);
    apply_summary.firewall_checks = firewall_checks;
    apply_summary.mail_checks.extend(mail_checks);
    apply_summary.certbot_checks = certbot_checks;
    apply_summary.app_checks = app_checks;

    match apply_tls_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary.network_checks,
    ) {
        Ok(certbot_checks) => {
            let tls_passed = certbot_checks
                .iter()
                .any(|check| check.name == "tls-certificate" && check.status == "pass");
            let tls_skipped = certbot_checks
                .iter()
                .any(|check| check.name == "tls" && check.status == "skipped");
            apply_summary.certbot_checks = certbot_checks;
            if tls_passed {
                completed_steps.push("certbot-issued".to_string());
                completed_steps.push("https-vhost-written".to_string());
                completed_steps.push("certbot-renew-dry-run".to_string());
                state.set_phase(InstallerPhase::TlsEnabled);
            } else if tls_skipped {
                completed_steps.push("tls-skipped".to_string());
            } else {
                state.completed_steps = completed_steps.clone();
                persist_progress(
                    &progress,
                    &mut owned_files,
                    &owned_file_list,
                    &state,
                    &install_plan,
                    &apply_summary,
                    Some("TLS checks did not pass; app placement was not started."),
                )?;
                return Err(Error::InstallVerificationFailed {
                    checks: "TLS checks did not pass; app placement was not started.".to_string(),
                });
            }
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.certbot_checks = vec![InstallCheck::fail(
                "tls-config",
                format!("TLS configuration failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    match apply_app_phase(
        probe,
        paths,
        &install_plan,
        &mut owned_file_list,
        &apply_summary,
    ) {
        Ok(app_checks) => {
            let app_source_ready = app_checks
                .iter()
                .any(|check| check.name == "app-source" && check.status == "pass");
            apply_summary.app_checks = app_checks;
            completed_steps.push(if app_source_ready {
                "app-source-prepared".to_string()
            } else {
                "app-source-deferred".to_string()
            });
            completed_steps.push("app-link-ready".to_string());
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                None,
            )?;
        }
        Err(err) => {
            apply_summary.app_checks = vec![InstallCheck::fail(
                "app-source",
                format!("Application source setup failed: {err}"),
            )];
            state.completed_steps = completed_steps.clone();
            persist_progress(
                &progress,
                &mut owned_files,
                &owned_file_list,
                &state,
                &install_plan,
                &apply_summary,
                Some(&err.to_string()),
            )?;
            return Err(err);
        }
    }

    completed_steps.push("setup-guide-written".to_string());
    if state.phase == InstallerPhase::TlsEnabled.as_str()
        || completed_steps.iter().any(|step| step == "tls-skipped")
    {
        state.set_phase(InstallerPhase::Completed);
    }
    state.completed_steps = completed_steps.clone();
    write_new_file(
        paths,
        SETUP_GUIDE_PATH,
        &setup_guide_content(
            &install_plan,
            &state.phase,
            &apply_summary,
            &completed_steps,
        ),
        &mut owned_file_list,
    )?;
    write_tracked_file(
        paths,
        BACKUP_MANIFEST_PATH,
        &backup_manifest_content(
            &install_plan,
            &state.phase,
            &owned_file_list,
            &completed_steps,
        )?,
        &mut owned_file_list,
    )?;
    completed_steps.push("backup-manifest-written".to_string());
    state.completed_steps = completed_steps.clone();
    persist_progress(
        &progress,
        &mut owned_files,
        &owned_file_list,
        &state,
        &install_plan,
        &apply_summary,
        None,
    )?;

    let app_url = app_access_url(&install_plan, &apply_summary);

    Ok(InstallReport {
        domain: state.domain,
        deployment_mode: install_plan.deployment_mode,
        app_profile: install_plan.app_profile,
        app_profile_label: install_plan.app_profile_label,
        app_document_root: install_plan.app_document_root,
        web_server: install_plan.web_server,
        php_version: install_plan.php_version,
        php_source: install_plan.php_source,
        database_engine: install_plan.database_engine,
        database_name: install_plan.database_name,
        database_user: install_plan.database_user,
        database_password_policy: install_plan.database_password_policy,
        site_user: install_plan.site_user,
        web_root_mode: install_plan.web_root_mode,
        web_root: install_plan.web_root.clone(),
        app_url,
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
        runtime_checks: apply_summary.runtime_checks,
        database_checks: apply_summary.database_checks,
        firewall_checks: apply_summary.firewall_checks,
        mail_checks: apply_summary.mail_checks,
        certbot_checks: apply_summary.certbot_checks,
        vhost_checks: apply_summary.vhost_checks,
        app_checks: apply_summary.app_checks,
        setup_guide_path: paths.resolve(SETUP_GUIDE_PATH),
        backup_manifest_path: paths.resolve(BACKUP_MANIFEST_PATH),
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
) -> std::result::Result<ApplySummary, Box<PackagePhaseFailure>> {
    let packages = package_names(plan);
    let services = managed_services(plan);
    let ports = managed_ports(plan);
    let preinstall_package_checks =
        inspect_preinstall_packages(probe, &packages).map_err(|error| PackagePhaseFailure {
            error,
            summary: ApplySummary::default(),
            completed_steps: Vec::new(),
        })?;
    let mut summary = ApplySummary {
        preinstall_package_checks,
        ..ApplySummary::default()
    };
    let mut completed_steps = Vec::new();

    let output = match probe.apt_update() {
        Ok(output) => output,
        Err(err) => {
            return Err(package_phase_failure(
                command_error("apt-update", "apt-get update", err),
                &summary,
                &completed_steps,
            ));
        }
    };
    if let Err(error) = require_success("apt-update", "apt-get update", output) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }
    completed_steps.push("apt-updated".to_string());

    if plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
        let source_packages = php_source_prerequisite_packages();
        let install_command = format!("apt-get install -y {}", source_packages.join(" "));
        let output = match probe.apt_install(&source_packages) {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("php-source-prerequisites", &install_command, err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) =
            require_success("php-source-prerequisites", install_command.clone(), output)
        {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }

        let output = match probe.apt_add_repository("ppa:ondrej/php") {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error(
                        "php-source-add",
                        "add-apt-repository -y ppa:ondrej/php",
                        err,
                    ),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success(
            "php-source-add",
            "add-apt-repository -y ppa:ondrej/php",
            output,
        ) {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("php-apt-source-added".to_string());

        let output = match probe.apt_update() {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("apt-update-after-php-source", "apt-get update", err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success("apt-update-after-php-source", "apt-get update", output)
        {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
        completed_steps.push("apt-updated-after-php-source".to_string());
    }

    let mut candidate_checks = Vec::new();
    for package in &packages {
        let available = match probe.apt_candidate_available(package) {
            Ok(available) => available,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("apt-candidate", format!("apt-cache policy {package}"), err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if !available {
            candidate_checks.push(InstallCheck::fail(
                package,
                "현재 apt 저장소에서 설치 후보를 찾지 못했습니다.",
            ));
            summary.package_checks = candidate_checks;
            completed_steps.push("package-candidates-checked".to_string());
            return Err(package_phase_failure(
                Error::PackageUnavailable {
                    package: package.clone(),
                },
                &summary,
                &completed_steps,
            ));
        }
        candidate_checks.push(InstallCheck::pass(
            package,
            "apt 저장소에서 설치 후보를 확인했습니다.",
        ));
    }
    summary.package_checks = candidate_checks;
    completed_steps.push("package-candidates-checked".to_string());

    let install_command = format!("apt-get install -y {}", packages.join(" "));
    let output = match probe.apt_install(&packages) {
        Ok(output) => output,
        Err(err) => {
            return Err(package_phase_failure(
                command_error("apt-install", &install_command, err),
                &summary,
                &completed_steps,
            ));
        }
    };
    if let Err(error) = require_success("apt-install", install_command.clone(), output) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }

    for service in &services {
        let command = format!("systemctl enable --now {service}");
        let output = match probe.enable_service_now(service) {
            Ok(output) => output,
            Err(err) => {
                return Err(package_phase_failure(
                    command_error("service-enable", &command, err),
                    &summary,
                    &completed_steps,
                ));
            }
        };
        if let Err(error) = require_success("service-enable", command.clone(), output) {
            return Err(package_phase_failure(error, &summary, &completed_steps));
        }
    }

    let package_checks = verify_packages(probe, &packages)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.package_checks = package_checks;
    let service_checks = verify_services(probe, &services)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.service_checks = service_checks;
    let port_checks = verify_ports(probe, &ports)
        .map_err(|error| package_phase_failure(error, &summary, &completed_steps))?;
    summary.port_checks = port_checks;
    if let Err(error) = require_checks_passed(
        &summary.package_checks,
        &summary.service_checks,
        &summary.port_checks,
    ) {
        return Err(package_phase_failure(error, &summary, &completed_steps));
    }
    let network_checks = verify_network_readiness(probe, plan);
    let mail_checks = verify_mail_readiness(probe, plan);
    let certbot_checks = verify_certbot_readiness(probe, plan, &summary.service_checks);

    summary.network_checks = network_checks;
    summary.mail_checks = mail_checks;
    summary.certbot_checks = certbot_checks;
    Ok(summary)
}

fn package_phase_failure(
    error: Error,
    summary: &ApplySummary,
    completed_steps: &[String],
) -> Box<PackagePhaseFailure> {
    Box::new(PackagePhaseFailure {
        error,
        summary: summary.clone(),
        completed_steps: completed_steps.to_vec(),
    })
}

fn php_source_prerequisite_packages() -> Vec<String> {
    vec![
        "software-properties-common".to_string(),
        "ca-certificates".to_string(),
        "lsb-release".to_string(),
    ]
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
    let site_home = site_home_path(plan);
    let command = format!("chmod 0711 {site_home}");
    let output = probe
        .chmod_path("0711", &site_home)
        .map_err(|err| command_error("site-home-traverse", &command, err))?;
    require_success("site-home-traverse", command, output)?;
    checks.push(InstallCheck::pass(
        "web-root-permissions",
        format!(
            "Set {} owner to {} and mode 0755; set {} to 0711 so the web server can traverse without listing the home directory.",
            plan.web_root, owner_group, site_home
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
    let mut checks = Vec::new();

    match plan.web_server.as_str() {
        "nginx" => {
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
        }
        "apache" => {
            enable_apache_modules(probe, apache_http_modules())?;
            write_new_file(
                paths,
                g7_system::apache::G7_SITE_AVAILABLE,
                &apache_vhost_content(plan),
                owned,
            )?;
            create_owned_symlink(
                paths,
                g7_system::apache::G7_SITE_AVAILABLE,
                g7_system::apache::G7_SITE_ENABLED,
                owned,
            )?;
            checks.push(InstallCheck::pass(
                "apache-vhost",
                format!(
                    "Wrote {} and enabled it at {}.",
                    g7_system::apache::G7_SITE_AVAILABLE,
                    g7_system::apache::G7_SITE_ENABLED
                ),
            ));

            let output = probe
                .apache_config_test()
                .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
            require_success("apache-configtest", "apache2ctl configtest", output)?;
            checks.push(InstallCheck::pass(
                "apache-configtest",
                "apache2ctl configtest completed successfully.",
            ));

            let output = probe
                .reload_service(g7_system::apache::SERVICE_NAME)
                .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
            require_success("apache-reload", "systemctl reload apache2", output)?;
            checks.push(InstallCheck::pass(
                "apache-reload",
                "Apache was reloaded after vhost enable.",
            ));
        }
        _ => return Ok(Vec::new()),
    }

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

fn apache_http_modules() -> &'static [&'static str] {
    &[
        "proxy",
        "proxy_http",
        "proxy_wstunnel",
        "proxy_fcgi",
        "setenvif",
        "rewrite",
        "headers",
    ]
}

fn apache_tls_modules() -> &'static [&'static str] {
    &[
        "ssl",
        "http2",
        "proxy",
        "proxy_http",
        "proxy_wstunnel",
        "proxy_fcgi",
        "setenvif",
        "rewrite",
        "headers",
    ]
}

fn enable_apache_modules<R: CommandRunner>(probe: &SystemProbe<R>, modules: &[&str]) -> Result<()> {
    for module in modules {
        let command = format!("a2enmod {module}");
        let output = probe
            .apache_enable_module(module)
            .map_err(|err| command_error("apache-enable-module", &command, err))?;
        require_success("apache-enable-module", command, output)?;
    }
    Ok(())
}

fn apply_runtime_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let sizing = detected_memory_sizing(probe);
    let pool_path = php_pool_path(plan);
    let ini_path = php_ini_override_path(plan);
    let nginx_tuning_path = "/etc/nginx/conf.d/g7-runtime-tuning.conf";

    checks.extend(apply_swap_configuration(probe, paths, &sizing, owned)?);

    checks.push(InstallCheck::pass(
        "server-sizing",
        format!(
            "Detected {} MiB RAM, {} vCPU; selected {} sizing preset with {} swap.",
            sizing.total_memory_kib / 1024,
            sizing.vcpu_count,
            sizing.tier_label,
            sizing.swap_size
        ),
    ));

    write_new_file(paths, &pool_path, &php_pool_content(plan, &sizing), owned)?;
    checks.push(InstallCheck::pass(
        "php-fpm-pool",
        format!(
            "Created PHP-FPM pool config at {pool_path}; max_children={}, memory_limit={}.",
            sizing.php_max_children, sizing.php_memory_limit
        ),
    ));

    write_new_file(paths, &ini_path, &php_ini_override_content(&sizing), owned)?;
    checks.push(InstallCheck::pass(
        "php-runtime-ini",
        format!("Created PHP runtime override at {ini_path}."),
    ));

    if plan.web_server == "nginx" {
        write_new_file(
            paths,
            nginx_tuning_path,
            &nginx_runtime_tuning_content(&sizing),
            owned,
        )?;
        write_existing_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_vhost_content_with_socket(plan, &php_fpm_site_socket(plan)),
        )?;
        checks.push(InstallCheck::pass(
            "nginx-fastcgi-runtime",
            format!(
                "Updated Nginx vhost to use site PHP-FPM socket {}.",
                php_fpm_site_socket(plan)
            ),
        ));
        checks.push(InstallCheck {
            name: "nginx-worker-mode".to_string(),
            status: "info".to_string(),
            message: format!(
                "Recommended nginx.conf values: worker_processes={}, worker_connections={}, rlimit_nofile={}. These are reported but not rewritten until nginx.conf backup ownership is implemented.",
                sizing.nginx_worker_processes,
                sizing.nginx_worker_connections,
                sizing.nginx_worker_rlimit_nofile
            ),
        });
    } else {
        write_existing_file(
            paths,
            g7_system::apache::G7_SITE_AVAILABLE,
            &apache_vhost_content_with_socket(plan, &php_fpm_site_socket(plan)),
        )?;
        checks.push(InstallCheck::pass(
            "apache-proxy-fcgi-runtime",
            format!(
                "Updated Apache vhost to use site PHP-FPM socket {}.",
                php_fpm_site_socket(plan)
            ),
        ));
        checks.push(InstallCheck {
            name: "apache-worker-mode".to_string(),
            status: "info".to_string(),
            message: format!(
                "Apache mpm_event target: MaxRequestWorkers={} with PHP-FPM pool max_children={}. Keep MPM tuning in apache2.conf/mpm_event.conf after manual backup ownership is implemented.",
                sizing.apache_max_request_workers, sizing.php_max_children
            ),
        });
    }

    let fpm_service = format!("php{}-fpm", plan.php_version);
    let output = probe.reload_service(&fpm_service).map_err(|err| {
        command_error(
            "php-fpm-reload",
            format!("systemctl reload {fpm_service}"),
            err,
        )
    })?;
    require_success(
        "php-fpm-reload",
        format!("systemctl reload {fpm_service}"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        "php-fpm-reload",
        format!("Reloaded {fpm_service}."),
    ));

    if plan.web_server == "nginx" {
        let output = probe
            .nginx_config_test()
            .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
        require_success("nginx-configtest", "nginx -t", output)?;
        let output = probe
            .reload_service(g7_system::nginx::SERVICE_NAME)
            .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
        require_success("nginx-reload", "systemctl reload nginx", output)?;
        checks.push(InstallCheck::pass(
            "nginx-runtime-reload",
            "Validated and reloaded Nginx after runtime tuning.",
        ));
    } else {
        let output = probe
            .apache_config_test()
            .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
        require_success("apache-configtest", "apache2ctl configtest", output)?;
        let output = probe
            .reload_service(g7_system::apache::SERVICE_NAME)
            .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
        require_success("apache-reload", "systemctl reload apache2", output)?;
        checks.push(InstallCheck::pass(
            "apache-runtime-reload",
            "Validated and reloaded Apache after runtime tuning.",
        ));
    }

    checks.extend(php_runtime_diagnostic_checks(probe, paths, plan, &sizing));

    Ok(checks)
}

fn php_runtime_diagnostic_checks<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> Vec<InstallCheck> {
    let mut checks = Vec::new();
    let output = match probe.runner().run(&php_runtime_probe_command(plan)) {
        Ok(output) => output,
        Err(error) => {
            checks.push(InstallCheck::fail(
                "php-runtime-probe",
                format!("PHP 런타임 정보를 실행하지 못했습니다: {error}"),
            ));
            return checks;
        }
    };

    if output.status != 0 {
        checks.push(InstallCheck::fail(
            "php-runtime-probe",
            format!(
                "PHP 런타임 정보 수집 실패: status={} stdout={} stderr={}",
                output.status,
                short_text(&output.stdout),
                short_text(&output.stderr)
            ),
        ));
        return checks;
    }

    let facts = parse_key_value_lines(&output.stdout);
    checks.push(InstallCheck::pass(
        "phpinfo-summary",
        format!(
            "FPM ini 기준 PHP 정보를 파싱했습니다: PHP {}, SAPI={}, ini={}, scan_dir={}.",
            fact(&facts, "php_version"),
            fact(&facts, "sapi"),
            fact(&facts, "loaded_ini"),
            fact(&facts, "scan_dir")
        ),
    ));

    let limits = [
        ("memory_limit", sizing.php_memory_limit.as_str()),
        ("upload_max_filesize", sizing.php_upload_limit.as_str()),
        ("post_max_size", sizing.php_upload_limit.as_str()),
        (
            "opcache.memory_consumption",
            sizing.opcache_memory.trim_end_matches('M'),
        ),
        ("opcache.validate_timestamps", "0"),
        ("opcache.enable_file_override", "1"),
    ];
    let mismatches = limits
        .iter()
        .filter_map(|(key, expected)| {
            let actual = fact(&facts, key);
            if normalize_php_value(&actual) == normalize_php_value(expected) {
                None
            } else {
                Some(format!("{key}: expected {expected}, actual {actual}"))
            }
        })
        .collect::<Vec<_>>();
    checks.push(if mismatches.is_empty() {
        InstallCheck::pass(
            "php-runtime-limits",
            format!(
                "PHP 한도 적용 확인: memory_limit={}, upload_max_filesize={}, post_max_size={}, max_execution_time={}, max_input_vars={}, opcache.memory_consumption={}.",
                fact(&facts, "memory_limit"),
                fact(&facts, "upload_max_filesize"),
                fact(&facts, "post_max_size"),
                fact(&facts, "max_execution_time"),
                fact(&facts, "max_input_vars"),
                fact(&facts, "opcache.memory_consumption")
            ),
        )
    } else {
        InstallCheck::fail(
            "php-runtime-limits",
            format!("PHP 설정값이 설치 계획과 다릅니다: {}.", mismatches.join("; ")),
        )
    });

    let loaded_extensions = fact(&facts, "extensions")
        .split(',')
        .map(|extension| extension.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    for extension in required_php_extensions(plan) {
        let present = loaded_extensions.iter().any(|loaded| loaded == extension);
        checks.push(if present {
            InstallCheck::pass(
                format!("php-extension:{extension}"),
                format!("PHP 확장 {extension} 로드 확인."),
            )
        } else {
            InstallCheck::fail(
                format!("php-extension:{extension}"),
                format!("PHP 확장 {extension} 이 로드되지 않았습니다. 앱 설치 전에 패키지/ini 설정을 확인하세요."),
            )
        });
    }

    checks.push(php_fpm_pool_value_check(paths, plan, sizing));
    checks
}

fn php_runtime_probe_command(plan: &plan::InstallPlan) -> CommandSpec {
    CommandSpec::new("env")
        .arg(format!(
            "PHP_INI_SCAN_DIR=/etc/php/{}/fpm/conf.d",
            plan.php_version
        ))
        .arg(format!("php{}", plan.php_version))
        .arg("-c")
        .arg(format!("/etc/php/{}/fpm/php.ini", plan.php_version))
        .arg("-r")
        .arg(php_runtime_probe_script())
}

fn php_runtime_probe_script() -> &'static str {
    r#"
echo "php_version=".PHP_VERSION."\n";
echo "sapi=".PHP_SAPI."\n";
echo "loaded_ini=".(php_ini_loaded_file() ?: "-")."\n";
echo "scan_dir=".(getenv("PHP_INI_SCAN_DIR") ?: "-")."\n";
foreach (["memory_limit","upload_max_filesize","post_max_size","max_execution_time","max_input_vars","date.timezone","realpath_cache_size","realpath_cache_ttl","opcache.enable","opcache.memory_consumption","opcache.validate_timestamps","opcache.enable_file_override"] as $key) {
    $value = ini_get($key);
    echo $key."=".($value === false ? "-" : $value)."\n";
}
echo "extensions=".implode(",", array_map("strtolower", get_loaded_extensions()))."\n";
"#
}

fn php_fpm_pool_value_check(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    sizing: &plan::ResolvedMemorySizing,
) -> InstallCheck {
    let path = php_pool_path(plan);
    let content = match fs::read_to_string(paths.resolve(&path)) {
        Ok(content) => content,
        Err(error) => {
            return InstallCheck::fail(
                "php-fpm-pool-values",
                format!("{path} 파일을 읽지 못했습니다: {error}"),
            );
        }
    };

    let expected = [
        ("user", plan.site_user.clone()),
        ("group", "www-data".to_string()),
        ("pm", "dynamic".to_string()),
        ("pm.max_children", sizing.php_max_children.to_string()),
        ("pm.max_requests", "500".to_string()),
    ];
    let mismatches = expected
        .iter()
        .filter_map(|(key, expected)| {
            let actual = pool_value(&content, key).unwrap_or_else(|| "-".to_string());
            if actual == *expected {
                None
            } else {
                Some(format!("{key}: expected {expected}, actual {actual}"))
            }
        })
        .collect::<Vec<_>>();

    if mismatches.is_empty() {
        InstallCheck::pass(
            "php-fpm-pool-values",
            format!(
                "PHP-FPM pool 확인: user={}, group=www-data, pm=dynamic, max_children={}, max_requests=500.",
                plan.site_user, sizing.php_max_children
            ),
        )
    } else {
        InstallCheck::fail(
            "php-fpm-pool-values",
            format!(
                "PHP-FPM pool 설정값이 설치 계획과 다릅니다: {}.",
                mismatches.join("; ")
            ),
        )
    }
}

fn required_php_extensions(plan: &plan::InstallPlan) -> Vec<&'static str> {
    let mut extensions = match crate::app_profile::resolve_app_profile(&plan.app_profile) {
        Ok(profile) => profile.php_extensions.to_vec(),
        Err(_) => vec![
            "curl",
            "fileinfo",
            "mbstring",
            "openssl",
            "pdo_mysql",
            "xml",
            "zip",
        ],
    };
    if plan.redis_mode == "enable" {
        extensions.push("redis");
    }
    extensions.sort_unstable();
    extensions.dedup();
    extensions
}

fn parse_key_value_lines(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn fact(facts: &[(String, String)], key: &str) -> String {
    facts
        .iter()
        .find(|(name, _value)| name == key)
        .map(|(_name, value)| value.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "-".to_string())
}

fn normalize_php_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn short_text(value: &str) -> String {
    let text = value.trim().replace('\n', " ");
    if text.chars().count() > 240 {
        format!("{}...", text.chars().take(240).collect::<String>())
    } else {
        text
    }
}

fn pool_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('[') {
            return None;
        }
        let (name, value) = line.split_once('=')?;
        if name.trim() == key {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn blocking_runtime_failure(checks: &[InstallCheck]) -> Option<String> {
    let failures = checks
        .iter()
        .filter(|check| {
            check.status == "fail"
                && (check.name == "php-runtime-probe"
                    || check.name == "php-runtime-limits"
                    || check.name == "php-fpm-pool-values"
                    || check.name.starts_with("php-extension:"))
        })
        .map(|check| format!("{} - {}", check.name, check.message))
        .collect::<Vec<_>>();

    if failures.is_empty() {
        None
    } else {
        Some(format!(
            "PHP 런타임 진단 실패. 웹앱 설치를 시작하지 않습니다: {}",
            failures.join("; ")
        ))
    }
}

fn apply_database_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    database_password: Option<&str>,
) -> Result<Vec<InstallCheck>> {
    let sizing = detected_memory_sizing(probe);
    let db_config_path = database_config_path(plan);
    write_new_file(
        paths,
        db_config_path,
        &database_runtime_content(&sizing),
        owned,
    )?;

    let db_service = database_service_name(plan);
    let command = format!("systemctl restart {db_service}");
    let output = probe
        .restart_service(db_service)
        .map_err(|err| command_error("database-restart", &command, err))?;
    require_success("database-restart", command, output)?;

    let password = match database_password {
        Some(value) => value.to_string(),
        None => random_hex_secret()?,
    };
    write_secret_file(
        paths,
        SECRETS_PATH,
        &secrets_content(plan, &password),
        owned,
    )?;

    let sql = database_sql(plan, &password);
    let engine = DatabaseEngine::from_id(&plan.database_engine);
    let output = probe.database_apply_sql(engine, &sql).map_err(|err| {
        command_error("database-provision", "mysql --protocol=socket -uroot", err)
    })?;
    require_success(
        "database-provision",
        "mysql --protocol=socket -uroot",
        output,
    )?;

    Ok(vec![
        InstallCheck::pass(
            "database-runtime",
            format!(
                "Created {db_config_path}; innodb_buffer_pool_size={}, max_connections={}.",
                sizing.db_buffer_pool, sizing.db_max_connections
            ),
        ),
        InstallCheck::pass(
            "database-restart",
            format!("Restarted {db_service} after DB runtime tuning."),
        ),
        InstallCheck::pass(
            "database-secret",
            format!(
                "{} DB password and stored it root-only at {SECRETS_PATH}.",
                if database_password.is_some() {
                    "Stored user-provided"
                } else {
                    "Generated"
                }
            ),
        ),
        InstallCheck::pass(
            "database-created",
            format!("Ensured database `{}` exists.", plan.database_name),
        ),
        InstallCheck::pass(
            "database-user-created",
            format!(
                "Ensured app DB user `{}`@`localhost` has privileges only for `{}`.",
                plan.database_user, plan.database_name
            ),
        ),
    ])
}

fn apply_post_database_guidance(
    plan: &plan::InstallPlan,
) -> (
    Vec<InstallCheck>,
    Vec<InstallCheck>,
    Vec<InstallCheck>,
    Vec<InstallCheck>,
) {
    let firewall_checks = vec![InstallCheck {
        name: "ufw-policy".to_string(),
        status: "deferred".to_string(),
        message: "UFW apply is deferred until active SSH port detection is implemented; provider firewall should allow SSH, 80, and 443 only.".to_string(),
    }];
    let mail_checks = if plan.mail_mode == "none" {
        vec![InstallCheck {
            name: "mail-delivery".to_string(),
            status: "skipped".to_string(),
            message: "Mail delivery is disabled for this install.".to_string(),
        }]
    } else {
        vec![InstallCheck {
            name: "mail-config".to_string(),
            status: "deferred".to_string(),
            message: format!(
                "{} mail settings will be written into the app .env during app configuration.",
                plan.mail_mode
            ),
        }]
    };
    let certbot_checks = if plan.deployment_mode == "local-test" {
        vec![InstallCheck {
            name: "tls".to_string(),
            status: "skipped".to_string(),
            message: "Local test mode skips Let's Encrypt.".to_string(),
        }]
    } else {
        vec![InstallCheck {
            name: "tls".to_string(),
            status: "deferred".to_string(),
            message: "Let's Encrypt issuance will run after DNS and HTTP challenge checks in the TLS batch.".to_string(),
        }]
    };
    let app_checks = vec![InstallCheck {
        name: "app-fetch".to_string(),
        status: "deferred".to_string(),
        message: "Selected web app source fetch and .env generation will run after runtime, database, and TLS are stable.".to_string(),
    }];
    (firewall_checks, mail_checks, certbot_checks, app_checks)
}

fn apply_tls_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    network_checks: &[InstallCheck],
) -> Result<Vec<InstallCheck>> {
    if plan.deployment_mode == "local-test" {
        return Ok(vec![
            InstallCheck {
                name: "certbot".to_string(),
                status: "skipped".to_string(),
                message: "Local test mode skips Let's Encrypt certificates.".to_string(),
            },
            InstallCheck {
                name: "tls".to_string(),
                status: "skipped".to_string(),
                message: "Local test mode skips HTTPS vhost mutation.".to_string(),
            },
        ]);
    }

    let failed_dns = network_checks
        .iter()
        .filter(|check| check.status == "fail")
        .map(|check| format!("{}: {}", check.name, check.message))
        .collect::<Vec<_>>();
    if !failed_dns.is_empty() {
        return Ok(vec![
            InstallCheck::fail(
                "tls-dns",
                format!(
                    "Let's Encrypt was not attempted because DNS/IP checks failed: {}",
                    failed_dns.join("; ")
                ),
            ),
            InstallCheck {
                name: "tls-certificate".to_string(),
                status: "deferred".to_string(),
                message: "Fix DNS A records, confirm HTTP access, then resume the TLS phase."
                    .to_string(),
            },
        ]);
    }

    let domains = certificate_hosts(plan);
    let cert_name = plan.domain.clone();
    let email = certificate_email(plan);
    let certbot_challenge_dir = certbot_http01_challenge_dir(plan);
    let certbot_http01_smoke_path = certbot_http01_smoke_path(plan);
    let existing_certificate = certificate_files_exist(paths, &cert_name);
    create_owned_dir_if_absent(
        paths,
        &format!("{}/.well-known", plan.app_document_root),
        owned,
    )?;
    create_owned_dir_if_absent(paths, &certbot_challenge_dir, owned)?;
    if paths.resolve(&certbot_http01_smoke_path).exists() {
        write_existing_file(
            paths,
            &certbot_http01_smoke_path,
            CERTBOT_HTTP01_SMOKE_CONTENT,
        )?;
    } else {
        write_new_file(
            paths,
            &certbot_http01_smoke_path,
            CERTBOT_HTTP01_SMOKE_CONTENT,
            owned,
        )?;
    }
    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {certbot_challenge_dir}");
    let output = probe
        .chown_recursive(&owner_group, &certbot_challenge_dir)
        .map_err(|err| command_error("certbot-http01-webroot-owner", &command, err))?;
    require_success("certbot-http01-webroot-owner", command, output)?;
    let command = format!("chmod -R 0755 {certbot_challenge_dir}");
    let output = probe
        .chmod_recursive("0755", &certbot_challenge_dir)
        .map_err(|err| command_error("certbot-http01-webroot-permissions", &command, err))?;
    require_success("certbot-http01-webroot-permissions", command, output)?;
    let certbot_http01_uri = certbot_http01_smoke_uri();
    for host in &domains {
        match probe.http_host_path_smoke(host, &certbot_http01_uri) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Error::InstallVerificationFailed {
                    checks: format!(
                        "Certbot HTTP-01 challenge smoke failed for Host: {host} path: {certbot_http01_uri}"
                    ),
                });
            }
            Err(err) => {
                return Err(command_error(
                    "certbot-http01-smoke",
                    format!("curl -H 'Host: {host}' http://127.0.0.1{certbot_http01_uri}"),
                    err,
                ));
            }
        }
    }

    let certificate_check = if existing_certificate {
        InstallCheck::pass(
            "tls-certificate",
            format!(
                "기존 Let's Encrypt 인증서 `{cert_name}`를 확인했습니다. 중복 발급 제한을 피하기 위해 새 발급은 실행하지 않았습니다."
            ),
        )
    } else {
        let output = probe
            .certbot_certonly_webroot(&plan.app_document_root, &cert_name, &domains, &email)
            .map_err(|err| {
                command_error(
                    "certbot-certonly",
                    format!(
                        "certbot certonly --webroot -w {} --cert-name {}",
                        plan.app_document_root, cert_name
                    ),
                    err,
                )
            })?;
        require_success(
            "certbot-certonly",
            format!(
                "certbot certonly --webroot -w {} --cert-name {}",
                plan.app_document_root, cert_name
            ),
            output,
        )?;
        InstallCheck::pass(
            "tls-certificate",
            format!(
                "Issued Let's Encrypt certificate `{cert_name}` for {} with Certbot webroot.",
                domains.join(", ")
            ),
        )
    };

    let vhost_check = if plan.web_server == "nginx" {
        write_existing_file(
            paths,
            g7_system::nginx::G7_SITE_AVAILABLE,
            &nginx_tls_vhost_content(plan, &php_fpm_site_socket(plan)),
        )?;

        let output = probe
            .nginx_config_test()
            .map_err(|err| command_error("nginx-configtest", "nginx -t", err))?;
        require_success("nginx-configtest", "nginx -t", output)?;

        let output = probe
            .reload_service(g7_system::nginx::SERVICE_NAME)
            .map_err(|err| command_error("nginx-reload", "systemctl reload nginx", err))?;
        require_success("nginx-reload", "systemctl reload nginx", output)?;

        InstallCheck::pass(
            "nginx-https-vhost",
            format!(
                "Rewrote {} with HTTPS server blocks for {}.",
                g7_system::nginx::G7_SITE_AVAILABLE,
                domains.join(", ")
            ),
        )
    } else {
        enable_apache_modules(probe, apache_tls_modules())?;
        write_existing_file(
            paths,
            g7_system::apache::G7_SITE_AVAILABLE,
            &apache_tls_vhost_content(plan, &php_fpm_site_socket(plan)),
        )?;

        let output = probe
            .apache_config_test()
            .map_err(|err| command_error("apache-configtest", "apache2ctl configtest", err))?;
        require_success("apache-configtest", "apache2ctl configtest", output)?;

        let output = probe
            .reload_service(g7_system::apache::SERVICE_NAME)
            .map_err(|err| command_error("apache-reload", "systemctl reload apache2", err))?;
        require_success("apache-reload", "systemctl reload apache2", output)?;

        InstallCheck::pass(
            "apache-https-vhost",
            format!(
                "Rewrote {} with HTTPS VirtualHost blocks for {}.",
                g7_system::apache::G7_SITE_AVAILABLE,
                domains.join(", ")
            ),
        )
    };

    let output = probe
        .certbot_renew_dry_run(&cert_name)
        .map_err(|err| command_error("certbot-renew-dry-run", "certbot renew --dry-run", err))?;
    require_success("certbot-renew-dry-run", "certbot renew --dry-run", output)?;

    let _ = owned;
    Ok(vec![
        InstallCheck::pass(
            "certbot-http01-smoke",
            format!(
                "Verified HTTP-01 challenge path {certbot_http01_uri} for {} before running Certbot.",
                domains.join(", ")
            ),
        ),
        certificate_check,
        vhost_check,
        InstallCheck::pass(
            "certbot-renew-dry-run",
            "certbot renew --dry-run completed successfully.",
        ),
    ])
}

fn apply_app_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    summary: &ApplySummary,
) -> Result<Vec<InstallCheck>> {
    fs::create_dir_all(paths.resolve(APP_SOURCE_DIR)).map_err(|source| Error::FileWriteFailed {
        path: APP_SOURCE_DIR.to_string(),
        source,
    })?;

    let app_url = app_access_url(plan, summary);
    let mut checks = match plan.app_profile.as_str() {
        "gnuboard7" => install_gnuboard7_app(probe, paths, plan, owned, &app_url)?,
        "wordpress" => install_wordpress_app(probe, paths, plan, owned)?,
        "laravel" => install_laravel_app(probe, paths, plan, owned, &app_url)?,
        _ => {
            let mut checks = install_placeholder_app(paths, plan, owned)?;
            checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
            checks
        }
    };

    checks.push(InstallCheck::pass(
        "app-url",
        format!("Open {app_url} to continue or verify the selected app install."),
    ));
    Ok(checks)
}

fn apply_app_permissions<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    ensure_app_writable_dirs(paths, plan, owned)?;
    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {}", plan.web_root);
    let output = probe
        .chown_recursive(&owner_group, &plan.web_root)
        .map_err(|err| command_error("app-web-root-owner", &command, err))?;
    require_success("app-web-root-owner", command, output)?;
    let command = format!("chmod -R 0755 {}", plan.web_root);
    let output = probe
        .chmod_recursive("0755", &plan.web_root)
        .map_err(|err| command_error("app-web-root-permissions", &command, err))?;
    require_success("app-web-root-permissions", command, output)?;
    checks.push(InstallCheck::pass(
        "app-file-permissions",
        format!(
            "Applied {} ownership and 0755 mode to {} after app placement.",
            owner_group, plan.web_root
        ),
    ));

    for writable_path in app_writable_paths(plan) {
        let target = format!("{}/{}", plan.web_root, writable_path);
        let command = format!("chmod -R 0775 {target}");
        let output = probe
            .chmod_recursive("0775", &target)
            .map_err(|err| command_error("app-writable-permissions", &command, err))?;
        require_success("app-writable-permissions", command, output)?;
        checks.push(InstallCheck::pass(
            format!("app-writable:{writable_path}"),
            format!("Set writable runtime path `{target}` to mode 0775."),
        ));
    }
    if let Some(check) = apply_app_env_permissions(probe, paths, plan)? {
        checks.push(check);
    }

    Ok(checks)
}

fn apply_app_env_permissions<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
) -> Result<Option<InstallCheck>> {
    let env_path = format!("{}/.env", plan.web_root);
    if !paths.resolve(&env_path).exists() {
        return Ok(None);
    }

    let command = format!("chmod 0640 {env_path}");
    let output = probe
        .chmod_path("0640", &env_path)
        .map_err(|err| command_error("app-env-permissions", &command, err))?;
    require_success("app-env-permissions", command, output)?;
    Ok(Some(InstallCheck::pass(
        "app-env-permissions",
        format!("Set `{env_path}` to mode 0640 after web-root permission normalization."),
    )))
}

fn verify_git_checkout<R: CommandRunner>(
    probe: &SystemProbe<R>,
    app_key: &str,
    source_dir: &str,
    required_files: &[&str],
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let error_step = git_verify_error_step(app_key);
    let head_output = probe.git_rev_parse_head(source_dir).map_err(|err| {
        command_error(
            error_step,
            format!("git -C {source_dir} rev-parse --verify HEAD"),
            err,
        )
    })?;
    let commit = head_output.stdout.trim().to_string();
    require_success(
        error_step,
        format!("git -C {source_dir} rev-parse --verify HEAD"),
        head_output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-head"),
        if commit.is_empty() {
            format!("{app_key} Git HEAD를 확인했습니다.")
        } else {
            format!("{app_key} Git HEAD `{commit}`를 확인했습니다.")
        },
    ));

    let output = probe.git_fsck_full(source_dir).map_err(|err| {
        command_error(error_step, format!("git -C {source_dir} fsck --full"), err)
    })?;
    require_success(
        error_step,
        format!("git -C {source_dir} fsck --full"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-fsck"),
        format!("{app_key} Git object 무결성을 확인했습니다."),
    ));

    let output = probe.git_diff_index_clean(source_dir).map_err(|err| {
        command_error(
            error_step,
            format!("git -C {source_dir} diff-index --quiet HEAD --"),
            err,
        )
    })?;
    require_success(
        error_step,
        format!("git -C {source_dir} diff-index --quiet HEAD --"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-clean"),
        format!("{app_key} checkout 작업트리가 HEAD와 일치합니다."),
    ));

    for required_file in required_files {
        let output = probe
            .git_ls_files_error_unmatch(source_dir, required_file)
            .map_err(|err| {
                command_error(
                    error_step,
                    format!("git -C {source_dir} ls-files --error-unmatch {required_file}"),
                    err,
                )
            })?;
        require_success(
            error_step,
            format!("git -C {source_dir} ls-files --error-unmatch {required_file}"),
            output,
        )?;
        checks.push(InstallCheck::pass(
            format!("{app_key}-git-tracked-{}", check_key(required_file)),
            format!("{app_key} Git index에서 `{required_file}` 파일을 확인했습니다."),
        ));
    }

    Ok(checks)
}

fn verify_zip_archive<R: CommandRunner>(
    probe: &SystemProbe<R>,
    app_key: &str,
    archive_path: &str,
) -> Result<InstallCheck> {
    let error_step = archive_verify_error_step(app_key);
    let output = probe
        .unzip_test(archive_path)
        .map_err(|err| command_error(error_step, format!("unzip -tq {archive_path}"), err))?;
    require_success(error_step, format!("unzip -tq {archive_path}"), output)?;
    Ok(InstallCheck::pass(
        format!("{app_key}-archive-test"),
        format!("{app_key} zip archive 무결성을 확인했습니다."),
    ))
}

fn verify_required_app_paths<R: CommandRunner>(
    probe: &SystemProbe<R>,
    check_prefix: &str,
    base_dir: &str,
    files: &[&str],
    dirs: &[&str],
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let error_step = app_path_verify_error_step(check_prefix);
    for file in files {
        let target = join_unix_path(base_dir, file);
        let output = probe
            .test_file(&target)
            .map_err(|err| command_error(error_step, format!("test -f {target}"), err))?;
        require_success(error_step, format!("test -f {target}"), output)?;
        checks.push(InstallCheck::pass(
            format!("{check_prefix}-file-{}", check_key(file)),
            format!("`{target}` 파일을 확인했습니다."),
        ));
    }
    for dir in dirs {
        let target = join_unix_path(base_dir, dir);
        let output = probe
            .test_dir(&target)
            .map_err(|err| command_error(error_step, format!("test -d {target}"), err))?;
        require_success(error_step, format!("test -d {target}"), output)?;
        checks.push(InstallCheck::pass(
            format!("{check_prefix}-dir-{}", check_key(dir)),
            format!("`{target}` 디렉터리를 확인했습니다."),
        ));
    }
    Ok(checks)
}

fn join_unix_path(base_dir: &str, relative: &str) -> String {
    format!(
        "{}/{}",
        base_dir.trim_end_matches('/'),
        relative.trim_start_matches('/')
    )
}

fn check_key(path: &str) -> String {
    path.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn git_verify_error_step(app_key: &str) -> &'static str {
    match app_key {
        "gnuboard7" => "gnuboard7-source-verify",
        "laravel" => "laravel-source-verify",
        _ => "app-source-verify",
    }
}

fn archive_verify_error_step(app_key: &str) -> &'static str {
    match app_key {
        "wordpress" => "wordpress-archive-verify",
        _ => "app-archive-verify",
    }
}

fn app_path_verify_error_step(check_prefix: &str) -> &'static str {
    if check_prefix.starts_with("gnuboard7") {
        "gnuboard7-path-verify"
    } else if check_prefix.starts_with("laravel") {
        "laravel-path-verify"
    } else if check_prefix.starts_with("wordpress") {
        "wordpress-path-verify"
    } else {
        "app-path-verify"
    }
}

fn install_gnuboard7_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    app_url: &str,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, GNUBOARD7_SOURCE_DIR)?;
    let output = probe
        .git_clone(GNUBOARD7_REPO_URL, GNUBOARD7_RELEASE_REF, GNUBOARD7_SOURCE_DIR)
        .map_err(|err| {
            command_error(
                "gnuboard7-source",
                format!(
                    "git clone --depth 1 --branch {GNUBOARD7_RELEASE_REF} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
                ),
                err,
            )
        })?;
    require_success(
        "gnuboard7-source",
        format!(
            "git clone --depth 1 --branch {GNUBOARD7_RELEASE_REF} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
        ),
        output,
    )?;
    let source_checks = verify_git_checkout(
        probe,
        "gnuboard7",
        GNUBOARD7_SOURCE_DIR,
        GNUBOARD7_REQUIRED_FILES,
    )?;

    let output = probe
        .copy_dir_contents(GNUBOARD7_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "gnuboard7-copy",
                format!("cp -a {GNUBOARD7_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "gnuboard7-copy",
        format!("cp -a {GNUBOARD7_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "gnuboard7-deployed",
        &plan.web_root,
        GNUBOARD7_REQUIRED_FILES,
        &[],
    )?;

    let db_password =
        read_database_password(paths)?.ok_or_else(|| Error::InstallVerificationFailed {
            checks: format!("database password was not found at {SECRETS_PATH}"),
        })?;
    write_existing_file(
        paths,
        &format!("{}/.env", plan.web_root),
        &laravel_env_content(plan, &db_password, app_url, LaravelRuntimeKind::Gnuboard7)?,
    )?;

    let mut checks = vec![
        InstallCheck::pass(
            "app-source",
            format!(
                "Checked out Gnuboard7 {GNUBOARD7_RELEASE_REF} from GitHub into {}.",
                plan.web_root
            ),
        ),
        InstallCheck::pass(
            "app-env",
            format!(
                "Wrote application .env with DB name `{}` and user `{}`; password remains in {SECRETS_PATH}.",
                plan.database_name, plan.database_user
            ),
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(write_gnuboard7_driver_settings(paths, plan, owned)?);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend(configure_laravel_runtime(
        probe,
        paths,
        plan,
        owned,
        LaravelRuntimeKind::Gnuboard7,
        LaravelRuntimeOptions::browser_installer(),
    )?);
    checks.push(InstallCheck::pass(
        "app-install-screen",
        format!("그누보드7 브라우저 설치 화면을 {app_url} 에 준비했습니다."),
    ));
    checks.push(InstallCheck::manual(
        "app-post-install",
        "브라우저 설치를 끝낸 뒤 마이그레이션, 최적화, queue/scheduler/Reverb 서비스 시작 여부를 후속 점검하세요.",
    ));

    Ok(checks)
}

fn write_gnuboard7_driver_settings(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let settings_dir = format!("{}/storage/app/settings", plan.web_root);
    create_owned_dir_if_absent(paths, &settings_dir, owned)?;
    let path = format!("{}/{}", plan.web_root, GNUBOARD7_DRIVER_SETTINGS_PATH);
    write_tracked_file(
        paths,
        &path,
        &gnuboard7_driver_settings_content(plan)?,
        owned,
    )?;

    let (cache_driver, session_driver) = gnuboard7_runtime_drivers(plan);
    Ok(vec![InstallCheck::pass(
        "gnuboard7-driver-settings",
        format!(
            "Preseeded Gnuboard7 driver settings at {path}; cache={cache_driver}, session={session_driver}, queue=sync."
        ),
    )])
}

fn gnuboard7_runtime_drivers(plan: &plan::InstallPlan) -> (&'static str, &'static str) {
    if plan.redis_mode == "enable" {
        ("redis", "redis")
    } else {
        ("file", "file")
    }
}

fn gnuboard7_driver_settings_content(plan: &plan::InstallPlan) -> Result<String> {
    let (cache_driver, session_driver) = gnuboard7_runtime_drivers(plan);
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };
    let value = serde_json::json!({
        "_meta": {
            "version": "1.0.0",
            "updated_at": format!("g7inst-{seconds}")
        },
        "storage_driver": "local",
        "s3_bucket": "",
        "s3_region": "ap-northeast-2",
        "s3_access_key": "",
        "s3_secret_key": "",
        "s3_url": "",
        "cache_driver": cache_driver,
        "redis_host": "127.0.0.1",
        "redis_port": 6379,
        "redis_password": "",
        "redis_database": 0,
        "memcached_host": "127.0.0.1",
        "memcached_port": 11211,
        "session_driver": session_driver,
        "session_lifetime": 120,
        "queue_driver": "sync",
        "log_driver": "daily",
        "log_level": "error",
        "log_days": 14,
        "websocket_enabled": false,
        "websocket_app_id": "",
        "websocket_app_key": "",
        "websocket_app_secret": "",
        "websocket_host": "localhost",
        "websocket_port": 8080,
        "websocket_scheme": "https",
        "websocket_verify_ssl": true,
        "websocket_server_host": "127.0.0.1",
        "websocket_server_port": 8080,
        "websocket_server_scheme": "http",
        "search_engine_driver": "mysql-fulltext"
    });

    let mut content = serde_json::to_string_pretty(&value).map_err(|source| {
        Error::InstallVerificationFailed {
            checks: format!("failed to render Gnuboard7 driver settings: {source}"),
        }
    })?;
    content.push('\n');
    Ok(content)
}

fn install_laravel_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    app_url: &str,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, LARAVEL_SOURCE_DIR)?;
    let output = probe
        .git_clone(LARAVEL_REPO_URL, LARAVEL_RELEASE_REF, LARAVEL_SOURCE_DIR)
        .map_err(|err| {
            command_error(
                "laravel-source",
                format!(
                    "git clone --depth 1 --branch {LARAVEL_RELEASE_REF} {LARAVEL_REPO_URL} {LARAVEL_SOURCE_DIR}"
                ),
                err,
            )
        })?;
    require_success(
        "laravel-source",
        format!(
            "git clone --depth 1 --branch {LARAVEL_RELEASE_REF} {LARAVEL_REPO_URL} {LARAVEL_SOURCE_DIR}"
        ),
        output,
    )?;
    let source_checks =
        verify_git_checkout(probe, "laravel", LARAVEL_SOURCE_DIR, LARAVEL_REQUIRED_FILES)?;

    let output = probe
        .copy_dir_contents(LARAVEL_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "laravel-copy",
                format!("cp -a {LARAVEL_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "laravel-copy",
        format!("cp -a {LARAVEL_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "laravel-deployed",
        &plan.web_root,
        LARAVEL_REQUIRED_FILES,
        &[],
    )?;

    let db_password =
        read_database_password(paths)?.ok_or_else(|| Error::InstallVerificationFailed {
            checks: format!("database password was not found at {SECRETS_PATH}"),
        })?;
    write_existing_file(
        paths,
        &format!("{}/.env", plan.web_root),
        &laravel_env_content(plan, &db_password, app_url, LaravelRuntimeKind::Laravel)?,
    )?;

    let mut checks = vec![
        InstallCheck::pass(
            "app-source",
            format!(
                "Checked out Laravel skeleton {LARAVEL_RELEASE_REF} into {}.",
                plan.web_root
            ),
        ),
        InstallCheck::pass(
            "app-env",
            format!(
                "Wrote Laravel .env with DB name `{}` and user `{}`; password remains in {SECRETS_PATH}.",
                plan.database_name, plan.database_user
            ),
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend(configure_laravel_runtime(
        probe,
        paths,
        plan,
        owned,
        LaravelRuntimeKind::Laravel,
        LaravelRuntimeOptions::full(),
    )?);
    checks.push(InstallCheck::pass(
        "app-install-screen",
        format!("Laravel should be available at {app_url}."),
    ));

    Ok(checks)
}

fn install_wordpress_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, WORDPRESS_EXTRACT_DIR)?;
    let output = probe
        .download_file(WORDPRESS_DOWNLOAD_URL, WORDPRESS_ARCHIVE_PATH)
        .map_err(|err| {
            command_error(
                "wordpress-download",
                format!("curl -fsSL -o {WORDPRESS_ARCHIVE_PATH} {WORDPRESS_DOWNLOAD_URL}"),
                err,
            )
        })?;
    require_success(
        "wordpress-download",
        format!("curl -fsSL -o {WORDPRESS_ARCHIVE_PATH} {WORDPRESS_DOWNLOAD_URL}"),
        output,
    )?;
    let archive_check = verify_zip_archive(probe, "wordpress", WORDPRESS_ARCHIVE_PATH)?;

    let output = probe
        .unzip_archive(WORDPRESS_ARCHIVE_PATH, WORDPRESS_EXTRACT_DIR)
        .map_err(|err| {
            command_error(
                "wordpress-unzip",
                format!("unzip -q {WORDPRESS_ARCHIVE_PATH} -d {WORDPRESS_EXTRACT_DIR}"),
                err,
            )
        })?;
    require_success(
        "wordpress-unzip",
        format!("unzip -q {WORDPRESS_ARCHIVE_PATH} -d {WORDPRESS_EXTRACT_DIR}"),
        output,
    )?;
    let source_checks = verify_required_app_paths(
        probe,
        "wordpress-source",
        WORDPRESS_SOURCE_DIR,
        WORDPRESS_REQUIRED_FILES,
        WORDPRESS_REQUIRED_DIRS,
    )?;

    let output = probe
        .copy_dir_contents(WORDPRESS_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "wordpress-copy",
                format!("cp -a {WORDPRESS_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "wordpress-copy",
        format!("cp -a {WORDPRESS_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "wordpress-deployed",
        &plan.web_root,
        WORDPRESS_REQUIRED_FILES,
        WORDPRESS_REQUIRED_DIRS,
    )?;

    let mut checks = vec![InstallCheck::pass(
        "app-source",
        format!(
            "Downloaded WordPress latest.zip and copied it into {}.",
            plan.web_root
        ),
    )];
    checks.push(archive_check);
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend([
        InstallCheck::pass(
            "app-install-screen",
            format!(
                "WordPress browser installer should be available at {}.",
                app_entry_url(plan)
            ),
        ),
        InstallCheck {
            name: "app-db-handoff".to_string(),
            status: "info".to_string(),
            message: format!(
                "Use DB `{}` and user `{}` from {SECRETS_PATH} in the WordPress install screen.",
                plan.database_name, plan.database_user
            ),
        },
    ]);
    Ok(checks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaravelRuntimeKind {
    Gnuboard7,
    Laravel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaravelRuntimeOptions {
    run_migrations: bool,
    run_optimize: bool,
    verify_about: bool,
    write_services: bool,
    enable_services: bool,
}

impl LaravelRuntimeOptions {
    fn full() -> Self {
        Self {
            run_migrations: true,
            run_optimize: true,
            verify_about: true,
            write_services: true,
            enable_services: true,
        }
    }

    fn browser_installer() -> Self {
        Self {
            run_migrations: false,
            run_optimize: false,
            verify_about: false,
            write_services: true,
            enable_services: false,
        }
    }
}

struct AppSystemdUnit {
    name: &'static str,
    content: String,
    enable_now: bool,
}

fn configure_laravel_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    kind: LaravelRuntimeKind,
    options: LaravelRuntimeOptions,
) -> Result<Vec<InstallCheck>> {
    let cwd = paths.resolve(&plan.web_root);
    let mut checks = Vec::new();

    let output = probe.composer_install(&cwd).map_err(|err| {
        command_error(
            "composer-install",
            "composer install --no-dev --prefer-dist --optimize-autoloader --no-interaction",
            err,
        )
    })?;
    require_success(
        "composer-install",
        "composer install --no-dev --prefer-dist --optimize-autoloader --no-interaction",
        output,
    )?;
    checks.push(InstallCheck::pass(
        "composer-install",
        format!("Installed PHP dependencies in {}.", plan.web_root),
    ));

    let output = probe
        .npm_install(&cwd)
        .map_err(|err| command_error("npm-install", "npm install", err))?;
    require_success("npm-install", "npm install", output)?;
    checks.push(InstallCheck::pass(
        "npm-install",
        format!("Installed frontend dependencies in {}.", plan.web_root),
    ));

    let output = probe
        .npm_run_build(&cwd)
        .map_err(|err| command_error("npm-build", "npm run build", err))?;
    require_success("npm-build", "npm run build", output)?;
    checks.push(InstallCheck::pass(
        "npm-build",
        "Built frontend assets with npm run build.",
    ));

    run_artisan_step(
        probe,
        &cwd,
        "artisan-key-generate",
        ["key:generate", "--force"],
        &mut checks,
        "Generated Laravel APP_KEY.",
    )?;
    run_artisan_step(
        probe,
        &cwd,
        "artisan-storage-link",
        ["storage:link"],
        &mut checks,
        "Linked public storage.",
    )?;
    if options.run_migrations {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-migrate",
            ["migrate", "--force"],
            &mut checks,
            "Applied database migrations.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-migrate",
            "브라우저 설치 화면에서 앱 설치를 완료한 뒤 필요 시 `php artisan migrate --force`를 실행하세요.",
        ));
    }

    if options.run_optimize {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-optimize",
            ["optimize"],
            &mut checks,
            "Cached Laravel runtime metadata.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-optimize",
            "브라우저 설치 완료 후 `php artisan optimize`로 캐시를 갱신하세요.",
        ));
    }

    if options.verify_about {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-about",
            ["about"],
            &mut checks,
            "Verified Laravel artisan runtime.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-about",
            "브라우저 설치 완료 후 `php artisan about`으로 앱 런타임을 확인하세요.",
        ));
    }

    if options.write_services {
        let units = app_systemd_units(plan, kind);
        for unit in &units {
            let unit_path = systemd_unit_path(unit.name);
            write_new_file(paths, &unit_path, &unit.content, owned)?;
            checks.push(InstallCheck::pass(
                format!("app-service-file:{}", unit.name),
                format!("Wrote systemd unit `{unit_path}`."),
            ));
        }

        let output = probe.systemd_daemon_reload().map_err(|err| {
            command_error("systemd-daemon-reload", "systemctl daemon-reload", err)
        })?;
        require_success("systemd-daemon-reload", "systemctl daemon-reload", output)?;
        checks.push(InstallCheck::pass(
            "systemd-daemon-reload",
            "Reloaded systemd units after app service creation.",
        ));

        for unit in units
            .into_iter()
            .filter(|unit| unit.enable_now && options.enable_services)
        {
            let command = format!("systemctl enable --now {}", unit.name);
            let output = probe
                .enable_service_now(unit.name)
                .map_err(|err| command_error("app-service-enable", &command, err))?;
            require_success("app-service-enable", command, output)?;
            checks.push(InstallCheck::pass(
                format!("app-service:{}", unit.name),
                format!("Enabled and started `{}`.", unit.name),
            ));
        }

        if !options.enable_services {
            checks.push(InstallCheck::manual(
                "app-services-enable",
                "앱 브라우저 설치를 끝낸 뒤 필요한 queue/scheduler/Reverb 서비스를 `systemctl enable --now`로 시작하세요.",
            ));
        }
    }

    Ok(checks)
}

fn run_artisan_step<R: CommandRunner, const N: usize>(
    probe: &SystemProbe<R>,
    cwd: &Path,
    step: &'static str,
    args: [&'static str; N],
    checks: &mut Vec<InstallCheck>,
    message: &'static str,
) -> Result<()> {
    let command = format!("php artisan {}", args.join(" "));
    let output = probe
        .artisan(cwd, args)
        .map_err(|err| command_error(step, &command, err))?;
    require_success(step, command, output)?;
    checks.push(InstallCheck::pass(step, message));
    Ok(())
}

fn app_systemd_units(plan: &plan::InstallPlan, kind: LaravelRuntimeKind) -> Vec<AppSystemdUnit> {
    let prefix = match kind {
        LaravelRuntimeKind::Gnuboard7 => "g7",
        LaravelRuntimeKind::Laravel => "laravel",
    };
    let mut units = vec![
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-queue.service",
                LaravelRuntimeKind::Laravel => "laravel-queue.service",
            },
            content: queue_service_content(plan),
            enable_now: true,
        },
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-scheduler.service",
                LaravelRuntimeKind::Laravel => "laravel-scheduler.service",
            },
            content: scheduler_service_content(plan, prefix),
            enable_now: false,
        },
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-scheduler.timer",
                LaravelRuntimeKind::Laravel => "laravel-scheduler.timer",
            },
            content: scheduler_timer_content(prefix),
            enable_now: true,
        },
    ];

    if kind == LaravelRuntimeKind::Gnuboard7 {
        units.push(AppSystemdUnit {
            name: "g7-reverb.service",
            content: reverb_service_content(plan),
            enable_now: true,
        });
    }

    units
}

fn queue_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription={} queue worker\nAfter=network.target {}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan queue:work --sleep=3 --tries=3 --timeout=90\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n",
        plan.app_profile_label,
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

fn scheduler_service_content(plan: &plan::InstallPlan, prefix: &str) -> String {
    format!(
        "[Unit]\nDescription={prefix} Laravel scheduler\nAfter=network.target {}\n\n[Service]\nType=oneshot\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan schedule:run\n",
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

fn scheduler_timer_content(prefix: &str) -> String {
    format!(
        "[Unit]\nDescription={prefix} Laravel scheduler every minute\n\n[Timer]\nOnCalendar=*:0/1\nAccuracySec=10s\nPersistent=true\nUnit={prefix}-scheduler.service\n\n[Install]\nWantedBy=timers.target\n"
    )
}

fn reverb_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription=Gnuboard7 Reverb websocket server\nAfter=network.target {}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan reverb:start --host=127.0.0.1 --port=8080\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n",
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

fn systemd_unit_path(unit: &str) -> String {
    format!("/etc/systemd/system/{unit}")
}

fn app_runtime_unit_names(plan: &plan::InstallPlan) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "gnuboard7" => &[
            "g7-queue.service",
            "g7-scheduler.service",
            "g7-scheduler.timer",
            "g7-reverb.service",
        ],
        "laravel" => &[
            "laravel-queue.service",
            "laravel-scheduler.service",
            "laravel-scheduler.timer",
        ],
        _ => &[],
    }
}

fn ensure_app_writable_dirs(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<()> {
    for writable_path in app_writable_paths(plan) {
        let target = format!("{}/{}", plan.web_root, writable_path);
        create_owned_dir_if_absent(paths, &target, owned)?;
    }
    Ok(())
}

fn app_writable_paths(plan: &plan::InstallPlan) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "gnuboard7" | "laravel" => &["storage", "bootstrap/cache"],
        "wordpress" => &["wp-content/uploads"],
        _ => &[],
    }
}

fn install_placeholder_app(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let index_path = format!("{}/index.php", plan.app_document_root);
    write_new_file(paths, &index_path, &placeholder_app_content(plan), owned)?;
    Ok(vec![
        InstallCheck {
            name: "app-source".to_string(),
            status: "deferred".to_string(),
            message: format!(
                "{} source URL is not selected yet; wrote a temporary handoff page at {index_path}.",
                plan.app_profile_label
            ),
        },
        InstallCheck::pass(
            "app-install-screen",
            format!(
                "Temporary app handoff page is available at {}.",
                app_entry_url(plan)
            ),
        ),
    ])
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

fn site_home_path(plan: &plan::InstallPlan) -> String {
    format!("/home/{}", plan.site_user)
}

fn ready_probe_content() -> &'static str {
    "<?php\nheader('Content-Type: text/plain; charset=utf-8');\necho \"G7inst vhost ready\\n\";\n"
}

fn certbot_http01_challenge_dir(plan: &plan::InstallPlan) -> String {
    format!(
        "{}/{}",
        plan.app_document_root, CERTBOT_HTTP01_CHALLENGE_DIR
    )
}

fn certbot_http01_smoke_path(plan: &plan::InstallPlan) -> String {
    format!(
        "{}/{}",
        certbot_http01_challenge_dir(plan),
        CERTBOT_HTTP01_SMOKE_FILENAME
    )
}

fn certbot_http01_smoke_uri() -> String {
    format!("/{CERTBOT_HTTP01_CHALLENGE_DIR}/{CERTBOT_HTTP01_SMOKE_FILENAME}")
}

fn certificate_files_exist(paths: &InstallPaths, cert_name: &str) -> bool {
    let cert_dir = format!("/etc/letsencrypt/live/{cert_name}");
    paths.resolve(&format!("{cert_dir}/fullchain.pem")).exists()
        && paths.resolve(&format!("{cert_dir}/privkey.pem")).exists()
}

fn app_entry_url(plan: &plan::InstallPlan) -> String {
    format!("http://{}{}", primary_http_host(plan), app_entry_path(plan))
}

fn app_access_url(plan: &plan::InstallPlan, summary: &ApplySummary) -> String {
    let scheme = if summary
        .certbot_checks
        .iter()
        .any(|check| check.name == "tls-certificate" && check.status == "pass")
    {
        "https"
    } else {
        "http"
    };
    format!(
        "{scheme}://{}{}",
        primary_http_host(plan),
        app_entry_path(plan)
    )
}

fn app_entry_path(plan: &plan::InstallPlan) -> &'static str {
    match plan.app_profile.as_str() {
        "gnuboard7" => "/install",
        "wordpress" => "/wp-admin/install.php",
        _ => "/",
    }
}

fn read_database_password(paths: &InstallPaths) -> Result<Option<String>> {
    let target = paths.resolve(SECRETS_PATH);
    let content = match fs::read_to_string(&target) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: SECRETS_PATH.to_string(),
                source,
            });
        }
    };

    Ok(content.lines().find_map(|line| {
        line.strip_prefix("database_password = ")
            .map(|value| value.trim().trim_matches('"').to_string())
    }))
}

fn laravel_env_content(
    plan: &plan::InstallPlan,
    db_password: &str,
    app_url: &str,
    kind: LaravelRuntimeKind,
) -> Result<String> {
    let app_key = random_laravel_app_key()?;
    let redis_enabled = plan.redis_mode == "enable";
    let mut env = format!(
        "APP_NAME=\"{}\"\nAPP_ENV=production\nAPP_KEY=base64:{app_key}\nAPP_DEBUG=false\nAPP_URL={app_url}\n\nDB_CONNECTION=mysql\nDB_HOST=localhost\nDB_PORT=3306\nDB_DATABASE={}\nDB_USERNAME={}\nDB_PASSWORD=\"{}\"\n\nCACHE_STORE={}\nCACHE_DRIVER={}\nSESSION_DRIVER={}\nQUEUE_CONNECTION={}\nREDIS_CLIENT=phpredis\nREDIS_HOST=127.0.0.1\nREDIS_PORT=6379\nREDIS_PASSWORD=null\n\n",
        plan.app_profile_label,
        plan.database_name,
        plan.database_user,
        db_password.replace('"', "\\\""),
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "database" },
    );
    env.push_str(&mail_env_content(plan));
    if kind == LaravelRuntimeKind::Gnuboard7 {
        let public_reverb_port = if app_url.starts_with("https://") {
            "443"
        } else {
            "80"
        };
        let public_reverb_scheme = if app_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        env.push_str(&format!(
            "\nBROADCAST_CONNECTION=reverb\nREVERB_APP_ID=g7\nREVERB_APP_KEY=g7-local\nREVERB_APP_SECRET=g7-local-secret\nREVERB_SERVER_HOST=127.0.0.1\nREVERB_SERVER_PORT=8080\nREVERB_HOST=127.0.0.1\nREVERB_PORT=8080\nREVERB_SCHEME=http\nVITE_REVERB_APP_KEY=g7-local\nVITE_REVERB_HOST={}\nVITE_REVERB_PORT={public_reverb_port}\nVITE_REVERB_SCHEME={public_reverb_scheme}\n",
            primary_http_host(plan)
        ));
    }
    Ok(env)
}

fn mail_env_content(plan: &plan::InstallPlan) -> String {
    match plan.mail_mode.as_str() {
        "local-postfix" => {
            let from = plan
                .smtp_from
                .clone()
                .unwrap_or_else(|| format!("noreply@{}", plan.domain));
            format!(
                "MAIL_MAILER=smtp\nMAIL_HOST=127.0.0.1\nMAIL_PORT=25\nMAIL_USERNAME=null\nMAIL_PASSWORD=null\nMAIL_ENCRYPTION=null\nMAIL_FROM_ADDRESS=\"{from}\"\nMAIL_FROM_NAME=\"{}\"\n",
                plan.app_profile_label
            )
        }
        "smtp-relay" => {
            let host = plan.smtp_host.clone().unwrap_or_default();
            let port = plan.smtp_port.unwrap_or(587);
            let encryption = plan
                .smtp_encryption
                .clone()
                .unwrap_or_else(|| "tls".to_string());
            let from = plan
                .smtp_from
                .clone()
                .unwrap_or_else(|| format!("noreply@{}", plan.domain));
            format!(
                "MAIL_MAILER=smtp\nMAIL_HOST={host}\nMAIL_PORT={port}\nMAIL_USERNAME=null\nMAIL_PASSWORD=null\nMAIL_ENCRYPTION={encryption}\nMAIL_FROM_ADDRESS=\"{from}\"\nMAIL_FROM_NAME=\"{}\"\n",
                plan.app_profile_label
            )
        }
        _ => format!(
            "MAIL_MAILER=log\nMAIL_FROM_ADDRESS=\"noreply@{}\"\nMAIL_FROM_NAME=\"{}\"\n",
            plan.domain, plan.app_profile_label
        ),
    }
}

fn placeholder_app_content(plan: &plan::InstallPlan) -> String {
    format!(
        "<?php\nheader('Content-Type: text/html; charset=utf-8');\n?><!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><title>{label} 준비됨</title><body><h1>{label} 설치 준비됨</h1><p>도메인, PHP-FPM, DB, SSL 설정이 완료되었습니다.</p><p>앱 소스 URL을 지정한 뒤 다시 설치하거나 수동 배포를 진행하세요.</p></body></html>\n",
        label = plan.app_profile_label
    )
}

fn remove_existing_path(paths: &InstallPaths, path: &str) -> Result<()> {
    let target = paths.resolve(path);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: path.to_string(),
                source,
            });
        }
    };

    if metadata.file_type().is_dir() {
        fs::remove_dir_all(&target).map_err(|source| Error::FileRemoveFailed {
            path: path.to_string(),
            source,
        })
    } else {
        fs::remove_file(&target).map_err(|source| Error::FileRemoveFailed {
            path: path.to_string(),
            source,
        })
    }
}

fn detected_memory_sizing<R: CommandRunner>(probe: &SystemProbe<R>) -> plan::ResolvedMemorySizing {
    let total_memory_kib = probe
        .total_memory_kib()
        .ok()
        .flatten()
        .unwrap_or(1024 * 1024);
    let vcpu_count = probe.vcpu_count().ok().flatten().unwrap_or(1);
    plan::resolve_memory_sizing(total_memory_kib, vcpu_count)
}

fn apply_swap_configuration<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    sizing: &plan::ResolvedMemorySizing,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    write_new_file(paths, SWAP_UNIT_PATH, &swap_unit_content(), owned)?;
    write_new_file(paths, SWAP_SYSCTL_PATH, swap_sysctl_content(), owned)?;

    if paths.resolve("/") != Path::new("/") {
        let swap_path = paths.resolve(SWAP_FILE_PATH);
        fs::write(
            &swap_path,
            format!("g7inst simulated {}\n", sizing.swap_size),
        )
        .map_err(|source| Error::FileWriteFailed {
            path: SWAP_FILE_PATH.to_string(),
            source,
        })?;
        owned.push(SWAP_FILE_PATH.to_string());
        checks.push(InstallCheck::pass(
            "swapfile",
            format!(
                "Prepared managed {} swapfile at {SWAP_FILE_PATH} with systemd unit {SWAP_UNIT_PATH}.",
                sizing.swap_size
            ),
        ));
        checks.push(InstallCheck::pass(
            "swap-sysctl",
            format!("Prepared swap sysctl policy at {SWAP_SYSCTL_PATH}."),
        ));
        return Ok(checks);
    }

    let output = probe
        .runner()
        .run(&swap_apply_command(&sizing.swap_size))
        .map_err(|err| {
            command_error(
                "swapfile",
                format!("create and enable {SWAP_FILE_PATH}"),
                err,
            )
        })?;
    require_success(
        "swapfile",
        format!("create and enable {SWAP_FILE_PATH}"),
        output,
    )?;
    if !owned.iter().any(|path| path == SWAP_FILE_PATH) {
        owned.push(SWAP_FILE_PATH.to_string());
    }

    checks.push(InstallCheck::pass(
        "swapfile",
        format!(
            "Enabled managed {} swapfile through systemd unit {SWAP_UNIT_PATH}.",
            sizing.swap_size
        ),
    ));
    checks.push(InstallCheck::pass(
        "swap-sysctl",
        format!("Applied vm.swappiness=10 and vm.vfs_cache_pressure=50 from {SWAP_SYSCTL_PATH}."),
    ));
    Ok(checks)
}

fn swap_apply_command(swap_size: &str) -> CommandSpec {
    let swap_size = shell_single_quote(swap_size);
    let swap_size_mib = swap_size_to_mib(swap_size.trim_matches('\''));
    CommandSpec::new("sh").arg("-c").arg(format!(
        r#"set -eu
swap_size={swap_size}
if [ ! -f {SWAP_FILE_PATH} ]; then
    fallocate -l "$swap_size" {SWAP_FILE_PATH} || dd if=/dev/zero of={SWAP_FILE_PATH} bs=1M count="{swap_size_mib}" status=none
    chmod 600 {SWAP_FILE_PATH}
    mkswap {SWAP_FILE_PATH} >/dev/null
else
    chmod 600 {SWAP_FILE_PATH}
fi
systemctl daemon-reload
systemctl enable --now swapfile.swap >/dev/null
sysctl --system >/dev/null
swapon --show=NAME | grep -qx {SWAP_FILE_PATH}
"#
    ))
}

fn swap_size_to_mib(swap_size: &str) -> u64 {
    let normalized = swap_size.trim().to_ascii_lowercase();
    let digits = normalized
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or(2);
    if normalized.contains('g') {
        digits.saturating_mul(1024).max(1024)
    } else {
        digits.max(1024)
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn swap_unit_content() -> String {
    format!(
        r#"[Unit]
Description=G7 Installer managed swapfile
After=local-fs.target

[Swap]
What={SWAP_FILE_PATH}

[Install]
WantedBy=swap.target
"#
    )
}

fn swap_sysctl_content() -> &'static str {
    "# Managed by g7inst.\nvm.swappiness = 10\nvm.vfs_cache_pressure = 50\n"
}

fn php_fpm_site_socket(plan: &plan::InstallPlan) -> String {
    format!(
        "/run/php/php{}-fpm-{}.sock",
        plan.php_version, plan.site_user
    )
}

fn php_pool_path(plan: &plan::InstallPlan) -> String {
    format!(
        "/etc/php/{}/fpm/pool.d/g7-{}.conf",
        plan.php_version, plan.site_user
    )
}

fn php_ini_override_path(plan: &plan::InstallPlan) -> String {
    format!(
        "/etc/php/{}/fpm/conf.d/99-g7-installer.ini",
        plan.php_version
    )
}

fn php_pool_content(plan: &plan::InstallPlan, sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"[g7-{site_user}]
user = {site_user}
group = www-data
listen = {socket}
listen.owner = www-data
listen.group = www-data
listen.mode = 0660

pm = dynamic
pm.max_children = {php_max_children}
pm.start_servers = {php_start_servers}
pm.min_spare_servers = {php_min_spare_servers}
pm.max_spare_servers = {php_max_spare_servers}
pm.max_requests = 500

php_admin_value[open_basedir] = {web_root}:/tmp
php_admin_value[session.save_path] = /tmp
request_slowlog_timeout = 2s
slowlog = /var/log/php{php_version}-fpm-{site_user}-slow.log
catch_workers_output = yes
"#,
        site_user = plan.site_user,
        socket = php_fpm_site_socket(plan),
        web_root = plan.web_root,
        php_version = plan.php_version,
        php_max_children = sizing.php_max_children,
        php_start_servers = sizing.php_start_servers,
        php_min_spare_servers = sizing.php_min_spare_servers,
        php_max_spare_servers = sizing.php_max_spare_servers,
    )
}

fn php_ini_override_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"; Managed by g7inst.
memory_limit = {memory_limit}
upload_max_filesize = {upload_limit}
post_max_size = {upload_limit}
max_execution_time = 120
max_input_vars = 3000
realpath_cache_size = 4096K
realpath_cache_ttl = 600
opcache.enable = 1
opcache.memory_consumption = {opcache_memory}
opcache.interned_strings_buffer = 16
opcache.max_accelerated_files = 20000
opcache.validate_timestamps = 0
opcache.revalidate_freq = 60
opcache.save_comments = 1
opcache.enable_file_override = 1
"#,
        memory_limit = sizing.php_memory_limit,
        upload_limit = sizing.php_upload_limit,
        opcache_memory = sizing.opcache_memory.trim_end_matches('M'),
    )
}

fn nginx_runtime_tuning_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"# Managed by g7inst. This file is included inside the nginx http context.
log_format g7_timing '$remote_addr - $remote_user [$time_local] "$request" '
                     '$status $body_bytes_sent "$http_referer" "$http_user_agent" '
                     'rt=$request_time uct=$upstream_connect_time '
                     'uht=$upstream_header_time urt=$upstream_response_time';

client_max_body_size {upload_limit};
keepalive_timeout {keepalive};
fastcgi_buffers {fastcgi_buffers};
fastcgi_buffer_size 32k;
gzip on;
gzip_vary on;
gzip_proxied any;
gzip_comp_level 5;
gzip_min_length 1024;
gzip_types text/plain text/css application/json application/javascript text/xml application/xml application/xml+rss text/javascript image/svg+xml;
"#,
        upload_limit = sizing.php_upload_limit.to_ascii_lowercase(),
        keepalive = sizing.nginx_keepalive_timeout,
        fastcgi_buffers = sizing.nginx_fastcgi_buffers,
    )
}

fn database_config_path(plan: &plan::InstallPlan) -> &'static str {
    if plan.database_engine == "mariadb" {
        "/etc/mysql/mariadb.conf.d/60-g7-installer.cnf"
    } else {
        "/etc/mysql/conf.d/g7-installer.cnf"
    }
}

fn database_service_name(plan: &plan::InstallPlan) -> &'static str {
    if plan.database_engine == "mariadb" {
        "mariadb"
    } else {
        "mysql"
    }
}

fn database_runtime_content(sizing: &plan::ResolvedMemorySizing) -> String {
    format!(
        r#"# Managed by g7inst.
[mysqld]
bind-address = 127.0.0.1
innodb_buffer_pool_size = {buffer_pool}
max_connections = {max_connections}
tmp_table_size = {tmp_table_size}
max_heap_table_size = {tmp_table_size}
slow_query_log = ON
long_query_time = 0.5
"#,
        buffer_pool = sizing.db_buffer_pool,
        max_connections = sizing.db_max_connections,
        tmp_table_size = sizing.db_tmp_table_size,
    )
}

fn apache_vhost_content(plan: &plan::InstallPlan) -> String {
    let php_socket = format!("/run/php/php{}-fpm.sock", plan.php_version);
    apache_vhost_content_with_socket(plan, &php_socket)
}

fn apache_vhost_content_with_socket(plan: &plan::InstallPlan, php_socket: &str) -> String {
    let redirect_blocks = apache_http_redirect_blocks(plan);
    let (server_name, aliases) = apache_app_hosts(plan);
    let server_alias = apache_server_alias_line(&aliases);
    let reverb_proxy = apache_reverb_proxy_block(plan);

    format!(
        "{redirect_blocks}<VirtualHost *:80>\n    ServerName {server_name}\n{server_alias}    DocumentRoot {root}\n\n    ErrorLog ${{APACHE_LOG_DIR}}/g7-error.log\n    CustomLog ${{APACHE_LOG_DIR}}/g7-access.log combined\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride All\n        Require all granted\n    </Directory>\n{reverb_proxy}\n    <FilesMatch \\.php$>\n        SetHandler \"proxy:unix:{php_socket}|fcgi://localhost/\"\n    </FilesMatch>\n\n    <DirectoryMatch \"^/.*/\\.git/\">\n        Require all denied\n    </DirectoryMatch>\n</VirtualHost>\n",
        root = plan.app_document_root,
    )
}

fn nginx_vhost_content(plan: &plan::InstallPlan) -> String {
    let php_socket = format!("/run/php/php{}-fpm.sock", plan.php_version);
    nginx_vhost_content_with_socket(plan, &php_socket)
}

fn nginx_vhost_content_with_socket(plan: &plan::InstallPlan, php_socket: &str) -> String {
    let app_hosts = nginx_app_hosts(plan);
    let redirect_blocks = nginx_redirect_blocks(plan);
    let reverb_proxy = nginx_reverb_proxy_block(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let static_cache_locations = nginx_static_cache_locations();

    format!(
        "{redirect_blocks}server {{\n    listen 80;\n    listen [::]:80;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    access_log /var/log/nginx/g7-access.log g7_timing;\n    error_log /var/log/nginx/g7-error.log;\n\n{certbot_http01_location}{static_cache_locations}\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n{reverb_proxy}\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{php_socket};\n    }}\n\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

fn nginx_static_cache_locations() -> &'static str {
    "    location ^~ /build/ {\n        access_log off;\n        expires 30d;\n        add_header Cache-Control \"public, max-age=2592000, immutable\" always;\n        try_files $uri =404;\n    }\n\n    location ^~ /assets/ {\n        access_log off;\n        expires 30d;\n        add_header Cache-Control \"public, max-age=2592000, immutable\" always;\n        try_files $uri =404;\n    }\n\n    location ~* \\.(?:css|js|mjs|map|jpg|jpeg|png|gif|webp|avif|svg|ico|woff2?|ttf|eot)$ {\n        access_log off;\n        expires 30d;\n        add_header Cache-Control \"public, max-age=2592000, immutable\" always;\n        try_files $uri =404;\n    }\n"
}

fn nginx_certbot_http01_challenge_location() -> &'static str {
    "    location ^~ /.well-known/acme-challenge/ {\n        default_type \"text/plain\";\n        try_files $uri =404;\n    }\n"
}

fn nginx_app_hosts(plan: &plan::InstallPlan) -> String {
    match plan.www_mode.as_str() {
        "redirect-to-www" if !plan.domain.starts_with("www.") => format!("www.{}", plan.domain),
        "redirect-to-root" | "none" => plan.domain.clone(),
        _ if !plan.domain.starts_with("www.") => format!("{} www.{}", plan.domain, plan.domain),
        _ => plan.domain.clone(),
    }
}

fn nginx_reverb_proxy_block(plan: &plan::InstallPlan) -> &'static str {
    if plan.app_profile != "gnuboard7" {
        return "";
    }

    "\n    location /app {\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n        proxy_pass http://127.0.0.1:8080;\n    }\n\n    location /apps {\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_pass http://127.0.0.1:8080;\n    }\n"
}

fn apache_reverb_proxy_block(plan: &plan::InstallPlan) -> &'static str {
    if plan.app_profile != "gnuboard7" {
        return "";
    }

    "\n    ProxyPreserveHost On\n    ProxyPass /app ws://127.0.0.1:8080/app\n    ProxyPassReverse /app ws://127.0.0.1:8080/app\n    ProxyPass /apps http://127.0.0.1:8080/apps\n    ProxyPassReverse /apps http://127.0.0.1:8080/apps\n"
}

fn secrets_content(plan: &plan::InstallPlan, db_password: &str) -> String {
    format!(
        "database_name = \"{}\"\ndatabase_user = \"{}\"\ndatabase_password = \"{}\"\n",
        plan.database_name, plan.database_user, db_password
    )
}

fn database_sql(plan: &plan::InstallPlan, db_password: &str) -> String {
    format!(
        "CREATE DATABASE IF NOT EXISTS `{db}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;\n\
         CREATE USER IF NOT EXISTS '{user}'@'localhost' IDENTIFIED BY '{password}';\n\
         ALTER USER '{user}'@'localhost' IDENTIFIED BY '{password}';\n\
         GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'localhost';\n\
         FLUSH PRIVILEGES;\n",
        db = sql_identifier(&plan.database_name),
        user = sql_string(&plan.database_user),
        password = sql_string(db_password),
    )
}

fn sql_identifier(value: &str) -> String {
    value.replace('`', "``")
}

fn sql_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "''")
}

fn nginx_redirect_blocks(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name www.{domain};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 http://{domain}$request_uri;\n    }}\n}}\n\n",
            domain = plan.domain,
            root = plan.app_document_root,
            certbot_http01_location = nginx_certbot_http01_challenge_location()
        ),
        "redirect-to-www" => format!(
            "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {domain};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 http://www.{domain}$request_uri;\n    }}\n}}\n\n",
            domain = plan.domain,
            root = plan.app_document_root,
            certbot_http01_location = nginx_certbot_http01_challenge_location()
        ),
        _ => String::new(),
    }
}

fn apache_http_redirect_blocks(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "<VirtualHost *:80>\n    ServerName www.{domain}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ http://{domain}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n",
            domain = plan.domain,
            root = plan.app_document_root
        ),
        "redirect-to-www" => format!(
            "<VirtualHost *:80>\n    ServerName {domain}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ http://www.{domain}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n",
            domain = plan.domain,
            root = plan.app_document_root
        ),
        _ => String::new(),
    }
}

fn nginx_tls_vhost_content(plan: &plan::InstallPlan, php_socket: &str) -> String {
    let http_hosts = certificate_hosts(plan).join(" ");
    let cert_name = &plan.domain;
    let app_hosts = nginx_app_hosts(plan);
    let canonical_redirect = nginx_https_canonical_redirect(plan);
    let reverb_proxy = nginx_reverb_proxy_block(plan);
    let certbot_http01_location = nginx_certbot_http01_challenge_location();
    let static_cache_locations = nginx_static_cache_locations();

    format!(
        "server {{\n    listen 80;\n    listen [::]:80;\n    server_name {http_hosts};\n    root {root};\n\n{certbot_http01_location}\n    location / {{\n        return 301 https://$host$request_uri;\n    }}\n}}\n\n{canonical_redirect}server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name {app_hosts};\n    root {root};\n    index index.php index.html index.htm;\n\n    ssl_certificate /etc/letsencrypt/live/{cert_name}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{cert_name}/privkey.pem;\n    ssl_protocols TLSv1.2 TLSv1.3;\n    ssl_prefer_server_ciphers off;\n\n    access_log /var/log/nginx/g7-access.log g7_timing;\n    error_log /var/log/nginx/g7-error.log;\n\n    add_header X-Content-Type-Options nosniff always;\n    add_header X-Frame-Options SAMEORIGIN always;\n    add_header Referrer-Policy strict-origin-when-cross-origin always;\n\n{certbot_http01_location}{static_cache_locations}\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n{reverb_proxy}\n    location ~ \\.php$ {{\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass unix:{php_socket};\n    }}\n\n    location ~ /\\. {{\n        deny all;\n    }}\n}}\n",
        root = plan.app_document_root,
    )
}

fn apache_tls_vhost_content(plan: &plan::InstallPlan, php_socket: &str) -> String {
    let http_hosts = certificate_hosts(plan).join(" ");
    let cert_name = &plan.domain;
    let canonical_redirect = apache_https_canonical_redirect(plan);
    let (server_name, aliases) = apache_app_hosts(plan);
    let server_alias = apache_server_alias_line(&aliases);
    let reverb_proxy = apache_reverb_proxy_block(plan);

    format!(
        "<VirtualHost *:80>\n    ServerName {primary_host}\n    ServerAlias {http_hosts}\n    DocumentRoot {root}\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride None\n        Require all granted\n    </Directory>\n\n    RewriteEngine On\n    RewriteCond %{{REQUEST_URI}} !^/\\.well-known/acme-challenge/\n    RewriteRule ^ https://%{{HTTP_HOST}}%{{REQUEST_URI}} [R=301,L]\n</VirtualHost>\n\n{canonical_redirect}<VirtualHost *:443>\n    ServerName {server_name}\n{server_alias}    DocumentRoot {root}\n\n    ErrorLog ${{APACHE_LOG_DIR}}/g7-error.log\n    CustomLog ${{APACHE_LOG_DIR}}/g7-access.log combined\n\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{cert_name}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{cert_name}/privkey.pem\n    Protocols h2 http/1.1\n\n    Header always set X-Content-Type-Options \"nosniff\"\n    Header always set X-Frame-Options \"SAMEORIGIN\"\n    Header always set Referrer-Policy \"strict-origin-when-cross-origin\"\n\n    <Directory {root}>\n        Options FollowSymLinks\n        AllowOverride All\n        Require all granted\n    </Directory>\n{reverb_proxy}\n    <FilesMatch \\.php$>\n        SetHandler \"proxy:unix:{php_socket}|fcgi://localhost/\"\n    </FilesMatch>\n\n    <DirectoryMatch \"^/.*/\\.git/\">\n        Require all denied\n    </DirectoryMatch>\n</VirtualHost>\n",
        primary_host = primary_http_host(plan),
        root = plan.app_document_root,
    )
}

fn nginx_https_canonical_redirect(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name www.{domain};\n    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;\n    return 301 https://{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        "redirect-to-www" => format!(
            "server {{\n    listen 443 ssl http2;\n    listen [::]:443 ssl http2;\n    server_name {domain};\n    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;\n    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;\n    return 301 https://www.{domain}$request_uri;\n}}\n\n",
            domain = plan.domain
        ),
        _ => String::new(),
    }
}

fn apache_https_canonical_redirect(plan: &plan::InstallPlan) -> String {
    if plan.domain.starts_with("www.") {
        return String::new();
    }

    match plan.www_mode.as_str() {
        "redirect-to-root" => format!(
            "<VirtualHost *:443>\n    ServerName www.{domain}\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{domain}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{domain}/privkey.pem\n    Redirect permanent / https://{domain}/\n</VirtualHost>\n\n",
            domain = plan.domain
        ),
        "redirect-to-www" => format!(
            "<VirtualHost *:443>\n    ServerName {domain}\n    SSLEngine on\n    SSLCertificateFile /etc/letsencrypt/live/{domain}/fullchain.pem\n    SSLCertificateKeyFile /etc/letsencrypt/live/{domain}/privkey.pem\n    Redirect permanent / https://www.{domain}/\n</VirtualHost>\n\n",
            domain = plan.domain
        ),
        _ => String::new(),
    }
}

fn apache_app_hosts(plan: &plan::InstallPlan) -> (String, Vec<String>) {
    match plan.www_mode.as_str() {
        "redirect-to-www" if !plan.domain.starts_with("www.") => {
            (format!("www.{}", plan.domain), Vec::new())
        }
        "redirect-to-root" | "none" => {
            let aliases = if plan.www_mode == "none" && !plan.domain.starts_with("www.") {
                vec![format!("www.{}", plan.domain)]
            } else {
                Vec::new()
            };
            (plan.domain.clone(), aliases)
        }
        _ if !plan.domain.starts_with("www.") => {
            (plan.domain.clone(), vec![format!("www.{}", plan.domain)])
        }
        _ => (plan.domain.clone(), Vec::new()),
    }
}

fn apache_server_alias_line(aliases: &[String]) -> String {
    if aliases.is_empty() {
        String::new()
    } else {
        format!("    ServerAlias {}\n", aliases.join(" "))
    }
}

fn certificate_email(plan: &plan::InstallPlan) -> String {
    plan.smtp_from
        .clone()
        .unwrap_or_else(|| format!("admin@{}", plan.domain.trim_start_matches("www.")))
}

fn primary_http_host(plan: &plan::InstallPlan) -> String {
    if plan.www_mode == "redirect-to-www" && !plan.domain.starts_with("www.") {
        format!("www.{}", plan.domain)
    } else {
        plan.domain.clone()
    }
}

fn web_service_name(plan: &plan::InstallPlan) -> &'static str {
    if plan.web_server == "apache" {
        g7_system::apache::SERVICE_NAME
    } else {
        g7_system::nginx::SERVICE_NAME
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
    service == web_service_name(plan)
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
            Ok(PackageStatus::Installed) => {
                Ok(InstallCheck::pass(package, "패키지 설치 확인 완료"))
            }
            Ok(PackageStatus::NotInstalled) => {
                Ok(InstallCheck::fail(package, "패키지가 설치되지 않았습니다."))
            }
            Ok(PackageStatus::Unknown) => Ok(InstallCheck::fail(
                package,
                "패키지 상태를 확인하지 못했습니다.",
            )),
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
                message: "설치 전부터 있던 패키지입니다. 그대로 사용합니다.".to_string(),
            }),
            Ok(PackageStatus::NotInstalled) => Ok(InstallCheck {
                name: package.clone(),
                status: "not-installed".to_string(),
                message: "설치 전에는 없던 패키지입니다. 이번 설치 대상입니다.".to_string(),
            }),
            Ok(PackageStatus::Unknown) => Ok(InstallCheck {
                name: package.clone(),
                status: "unknown".to_string(),
                message: "설치 전 패키지 상태를 확인하지 못했습니다.".to_string(),
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
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
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
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
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

fn write_tracked_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    write_existing_file(paths, path, content)?;
    if !owned.iter().any(|owned_path| owned_path == path) {
        owned.push(path.to_string());
    }
    Ok(())
}

fn write_existing_file(paths: &InstallPaths, path: &str, content: &str) -> Result<()> {
    let target = paths.resolve(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(&target, content).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })
}

fn write_secret_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    write_new_file(paths, path, content, owned)?;
    #[cfg(unix)]
    {
        let target = paths.resolve(path);
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).map_err(|source| {
            Error::FileWriteFailed {
                path: path.to_string(),
                source,
            }
        })?;
    }
    Ok(())
}

fn persist_progress(
    progress: &ProgressContext<'_>,
    owned_files: &mut OwnedFiles,
    owned_file_list: &[String],
    state: &InstallerState,
    plan: &plan::InstallPlan,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<()> {
    owned_files.files = owned_file_list.to_vec();
    write_owned_files(progress.owned_files_path, owned_files).map_err(|source| {
        Error::FileWriteFailed {
            path: OWNED_FILES_PATH.to_string(),
            source,
        }
    })?;
    write_existing_file(
        progress.paths,
        ROLLBACK_PATH,
        &rollback_content(owned_file_list),
    )?;
    write_state_file(progress.state_path, state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;
    write_existing_file(
        progress.paths,
        REPORT_PATH,
        &report_content(plan, &state.phase, summary, problem)?,
    )?;
    Ok(())
}

fn random_hex_secret() -> Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|source| Error::InstallVerificationFailed {
        checks: format!("failed to generate database password: {source}"),
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn random_laravel_app_key() -> Result<String> {
    use base64::Engine;

    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|source| Error::InstallVerificationFailed {
        checks: format!("failed to generate Laravel APP_KEY: {source}"),
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
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
    content.push_str(&format!("php_source = \"{}\"\n", plan.php_source));
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

fn backup_manifest_content(
    plan: &plan::InstallPlan,
    phase: &str,
    owned: &[String],
    completed_steps: &[String],
) -> Result<String> {
    let value = serde_json::json!({
        "version": 1,
        "kind": "g7-installer-recovery-manifest",
        "domain": &plan.domain,
        "phase": phase,
        "scope": "installer-created configuration/state manifest, not a full site data backup",
        "config_paths": {
            "config": CONFIG_PATH,
            "state": STATE_PATH,
            "owned_files": OWNED_FILES_PATH,
            "rollback": ROLLBACK_PATH,
            "report": REPORT_PATH,
            "setup_guide": SETUP_GUIDE_PATH,
            "secrets": SECRETS_PATH
        },
        "site_paths": {
            "web_root": &plan.web_root,
            "app_document_root": &plan.app_document_root
        },
        "certificate_policy": {
            "path": format!("/etc/letsencrypt/live/{}", plan.domain),
            "reset": "preserve existing Let's Encrypt certificates to avoid duplicate issuance limits"
        },
        "completed_steps": completed_steps,
        "owned_paths": owned
    });

    let mut payload =
        serde_json::to_string_pretty(&value).map_err(|source| Error::FileWriteFailed {
            path: BACKUP_MANIFEST_PATH.to_string(),
            source: io::Error::other(source),
        })?;
    payload.push('\n');
    Ok(payload)
}

fn report_content(
    plan: &plan::InstallPlan,
    phase: &str,
    summary: &ApplySummary,
    problem: Option<&str>,
) -> Result<String> {
    let mut value = serde_json::Map::new();
    value.insert("version".to_string(), serde_json::json!(1));
    value.insert("domain".to_string(), serde_json::json!(&plan.domain));
    value.insert("phase".to_string(), serde_json::json!(phase));
    value.insert(
        "deployment_mode".to_string(),
        serde_json::json!(&plan.deployment_mode),
    );
    value.insert(
        "app_package".to_string(),
        serde_json::json!(&plan.app_profile),
    );
    value.insert(
        "app_profile".to_string(),
        serde_json::json!(&plan.app_profile),
    );
    value.insert(
        "app_profile_label".to_string(),
        serde_json::json!(&plan.app_profile_label),
    );
    value.insert(
        "app_summary".to_string(),
        serde_json::json!(&plan.app_summary),
    );
    value.insert(
        "app_document_root".to_string(),
        serde_json::json!(&plan.app_document_root),
    );
    value.insert(
        "app_url".to_string(),
        serde_json::json!(app_access_url(plan, summary)),
    );
    value.insert(
        "web_server".to_string(),
        serde_json::json!(&plan.web_server),
    );
    value.insert(
        "php_version".to_string(),
        serde_json::json!(&plan.php_version),
    );
    value.insert(
        "php_source".to_string(),
        serde_json::json!(&plan.php_source),
    );
    value.insert(
        "database".to_string(),
        serde_json::json!(&plan.database_engine),
    );
    value.insert(
        "database_name".to_string(),
        serde_json::json!(&plan.database_name),
    );
    value.insert(
        "database_user".to_string(),
        serde_json::json!(&plan.database_user),
    );
    value.insert(
        "database_password_policy".to_string(),
        serde_json::json!(plan.database_password_policy),
    );
    value.insert("site_user".to_string(), serde_json::json!(&plan.site_user));
    value.insert(
        "web_root_mode".to_string(),
        serde_json::json!(&plan.web_root_mode),
    );
    value.insert("web_root".to_string(), serde_json::json!(&plan.web_root));
    value.insert("www_mode".to_string(), serde_json::json!(&plan.www_mode));
    value.insert("redis".to_string(), serde_json::json!(&plan.redis_mode));
    value.insert("mail_mode".to_string(), serde_json::json!(&plan.mail_mode));
    value.insert("smtp_host".to_string(), serde_json::json!(&plan.smtp_host));
    value.insert("smtp_port".to_string(), serde_json::json!(plan.smtp_port));
    value.insert("smtp_from".to_string(), serde_json::json!(&plan.smtp_from));
    value.insert(
        "smtp_encryption".to_string(),
        serde_json::json!(&plan.smtp_encryption),
    );
    value.insert(
        "dns_check".to_string(),
        serde_json::json!(plan.dns_check_required),
    );
    value.insert(
        "security_profile".to_string(),
        serde_json::json!(&plan.security_profile),
    );
    value.insert(
        "ssh_policy".to_string(),
        serde_json::json!(&plan.ssh_policy),
    );
    value.insert(
        "safety_checks".to_string(),
        serde_json::json!(checks_json(&summary.safety_checks)),
    );
    value.insert(
        "preinstall_package_checks".to_string(),
        serde_json::json!(checks_json(&summary.preinstall_package_checks)),
    );
    value.insert(
        "package_checks".to_string(),
        serde_json::json!(checks_json(&summary.package_checks)),
    );
    value.insert(
        "service_checks".to_string(),
        serde_json::json!(checks_json(&summary.service_checks)),
    );
    value.insert(
        "port_checks".to_string(),
        serde_json::json!(checks_json(&summary.port_checks)),
    );
    value.insert(
        "network_checks".to_string(),
        serde_json::json!(checks_json(&summary.network_checks)),
    );
    value.insert(
        "runtime_checks".to_string(),
        serde_json::json!(checks_json(&summary.runtime_checks)),
    );
    value.insert(
        "database_checks".to_string(),
        serde_json::json!(checks_json(&summary.database_checks)),
    );
    value.insert(
        "firewall_checks".to_string(),
        serde_json::json!(checks_json(&summary.firewall_checks)),
    );
    value.insert(
        "mail_checks".to_string(),
        serde_json::json!(checks_json(&summary.mail_checks)),
    );
    value.insert(
        "certbot_checks".to_string(),
        serde_json::json!(checks_json(&summary.certbot_checks)),
    );
    value.insert(
        "vhost_checks".to_string(),
        serde_json::json!(checks_json(&summary.vhost_checks)),
    );
    value.insert(
        "app_checks".to_string(),
        serde_json::json!(checks_json(&summary.app_checks)),
    );
    value.insert(
        "setup_guide_path".to_string(),
        serde_json::json!(SETUP_GUIDE_PATH),
    );
    value.insert(
        "backup_manifest_path".to_string(),
        serde_json::json!(BACKUP_MANIFEST_PATH),
    );
    value.insert(
        "app_requirements".to_string(),
        serde_json::json!(requirements_json(&plan.app_requirements)),
    );
    value.insert(
        "app_followup_steps".to_string(),
        serde_json::json!(followup_steps_json(&plan.app_followup_steps)),
    );
    value.insert("problem".to_string(), serde_json::json!(problem));
    let mut payload =
        serde_json::to_string_pretty(&serde_json::Value::Object(value)).map_err(|source| {
            Error::FileWriteFailed {
                path: REPORT_PATH.to_string(),
                source: io::Error::other(source),
            }
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

fn setup_guide_content(
    plan: &plan::InstallPlan,
    phase: &str,
    summary: &ApplySummary,
    completed_steps: &[String],
) -> String {
    let web_service = if plan.web_server == "apache" {
        "apache2"
    } else {
        "nginx"
    };
    let fpm_service = format!("php{}-fpm", plan.php_version);
    let db_service = database_service_name(plan);
    let access_url = app_access_url(plan, summary);

    let mut content = String::new();
    content.push_str(&format!("# G7 Installer Setup Guide - {}\n\n", plan.domain));
    content.push_str("이 문서는 설치기가 만든 서버 구성을 사람이 확인하기 위한 안내서입니다. DB 비밀번호 같은 민감값은 화면에 쓰지 않고 root 전용 파일 경로만 표시합니다.\n\n");
    content.push_str("## 요약\n\n");
    content.push_str(&format!("- 도메인: `{}`\n", plan.domain));
    content.push_str(&format!("- 웹앱 접속 주소: `{access_url}`\n"));
    content.push_str(&format!("- 현재 단계: `{phase}`\n"));
    content.push_str(&format!(
        "- 앱 프로필: `{}` ({})\n",
        plan.app_profile, plan.app_profile_label
    ));
    content.push_str(&format!(
        "- 웹서버 / PHP: `{}` / `PHP {}` ({})\n",
        plan.web_server, plan.php_version, plan.php_source
    ));
    content.push_str(&format!("- DB: `{}`\n", plan.database_engine));
    content.push_str(&format!("- 사이트 계정: `{}`\n", plan.site_user));
    content.push_str("\n## 주요 경로\n\n");
    content.push_str(&format!("- 웹 루트: `{}`\n", plan.web_root));
    content.push_str(&format!("- 앱 문서 루트: `{}`\n", plan.app_document_root));
    content.push_str(&format!("- 웹앱 링크: `{access_url}`\n"));
    content.push_str(&format!("- 설치 상태: `{STATE_PATH}`\n"));
    content.push_str(&format!("- 설치 리포트(JSON): `{REPORT_PATH}`\n"));
    content.push_str(&format!("- 설정 안내서(MD): `{SETUP_GUIDE_PATH}`\n"));
    content.push_str(&format!(
        "- 복구 매니페스트(JSON): `{BACKUP_MANIFEST_PATH}`\n"
    ));
    content.push_str(&format!("- 기본 설정: `{CONFIG_PATH}`\n"));
    content.push_str(&format!("- 비밀 설정: `{SECRETS_PATH}`\n"));
    content.push_str("\n## 설정 파일\n\n");
    if plan.web_server == "nginx" {
        content.push_str(&format!(
            "- Nginx vhost: `{}`\n",
            g7_system::nginx::G7_SITE_AVAILABLE
        ));
        content.push_str(&format!(
            "- Nginx enabled: `{}`\n",
            g7_system::nginx::G7_SITE_ENABLED
        ));
        content.push_str("- Nginx runtime: `/etc/nginx/conf.d/g7-runtime-tuning.conf`\n");
    } else {
        content.push_str("- Apache vhost: `/etc/apache2/sites-available/g7.conf`\n");
        content.push_str("- Apache enabled: `/etc/apache2/sites-enabled/g7.conf`\n");
    }
    content.push_str(&format!("- PHP-FPM pool: `{}`\n", php_pool_path(plan)));
    content.push_str(&format!(
        "- PHP override: `{}`\n",
        php_ini_override_path(plan)
    ));
    content.push_str(&format!("- DB tuning: `{}`\n", database_config_path(plan)));
    if plan.redis_mode == "enable" {
        content.push_str("- Redis config: `/etc/redis/redis.conf`\n");
    }
    for unit in app_runtime_unit_names(plan) {
        content.push_str(&format!(
            "- 앱 systemd unit: `{}`\n",
            systemd_unit_path(unit)
        ));
    }
    content.push_str("\n## 계정과 DB\n\n");
    content.push_str(&format!("- Linux 사이트 계정: `{}`\n", plan.site_user));
    content.push_str(&format!("- DB 이름: `{}`\n", plan.database_name));
    content.push_str(&format!(
        "- DB 사용자: `{}`@`localhost`\n",
        plan.database_user
    ));
    content.push_str(&format!(
        "- DB 비밀번호 위치: `{SECRETS_PATH}` (root만 읽기)\n"
    ));
    content.push_str("\n## 서비스 명령\n\n");
    content.push_str(&format!(
        "- 웹서버 상태: `sudo systemctl status {web_service}`\n"
    ));
    content.push_str(&format!(
        "- 웹서버 재시작: `sudo systemctl restart {web_service}`\n"
    ));
    content.push_str(&format!(
        "- PHP-FPM 상태: `sudo systemctl status {fpm_service}`\n"
    ));
    content.push_str(&format!(
        "- PHP-FPM 재시작: `sudo systemctl restart {fpm_service}`\n"
    ));
    content.push_str(&format!(
        "- DB 상태: `sudo systemctl status {db_service}`\n"
    ));
    content.push_str(&format!(
        "- DB 재시작: `sudo systemctl restart {db_service}`\n"
    ));
    if plan.redis_mode == "enable" {
        content.push_str("- Redis 상태: `sudo systemctl status redis-server`\n");
        content.push_str("- Redis 재시작: `sudo systemctl restart redis-server`\n");
    }
    if plan.mail_mode == "local-postfix" {
        content.push_str("- Postfix 상태: `sudo systemctl status postfix`\n");
        content.push_str("- Postfix 재시작: `sudo systemctl restart postfix`\n");
    }
    for unit in app_runtime_unit_names(plan) {
        if unit.ends_with(".timer") {
            content.push_str(&format!(
                "- 앱 타이머 상태: `sudo systemctl status {unit}`\n"
            ));
        } else if unit.ends_with("-scheduler.service") {
            content.push_str(&format!(
                "- 앱 스케줄러 수동 실행: `sudo systemctl start {unit}`\n"
            ));
        } else {
            content.push_str(&format!(
                "- 앱 서비스 상태: `sudo systemctl status {unit}`\n"
            ));
            content.push_str(&format!(
                "- 앱 서비스 재시작: `sudo systemctl restart {unit}`\n"
            ));
        }
    }
    content.push_str("- SSL 자동갱신 타이머: `sudo systemctl status certbot.timer`\n");
    content
        .push_str("- SSL 갱신 테스트: `sudo certbot renew --dry-run --no-random-sleep-on-renew`\n");
    content.push_str("\n## 설치 결과\n\n");
    content.push_str(&format!(
        "- 완료 단계: `{}`\n",
        completed_steps.join("`, `")
    ));
    content.push_str("\n### 런타임\n\n");
    content.push_str(&checks_markdown(&summary.runtime_checks));
    content.push_str("\n### DB\n\n");
    content.push_str(&checks_markdown(&summary.database_checks));
    content.push_str("\n### 방화벽\n\n");
    content.push_str(&checks_markdown(&summary.firewall_checks));
    content.push_str("\n### 메일\n\n");
    content.push_str(&checks_markdown(&summary.mail_checks));
    content.push_str("\n### SSL\n\n");
    content.push_str(&checks_markdown(&summary.certbot_checks));
    content.push_str("\n### 앱 설치 준비\n\n");
    content.push_str(&checks_markdown(&summary.app_checks));
    content.push_str("\n## 주의\n\n");
    content.push_str("- VPS 전체 복구는 제공자 스냅샷을 기준으로 처리합니다.\n");
    content.push_str("- 복구 매니페스트는 설치기가 만든 설정/상태 추적용이며 DB 덤프나 웹루트 운영 데이터 백업이 아닙니다.\n");
    content.push_str("- 설치기가 소유하지 않은 기존 서비스/파일은 자동 삭제하지 않습니다.\n");
    content.push_str("- PDF가 필요하면 이 Markdown을 웹 UI에서 표시한 뒤 브라우저 인쇄/PDF 저장으로 내보내는 방식을 권장합니다.\n");
    content
}

fn checks_markdown(checks: &[InstallCheck]) -> String {
    if checks.is_empty() {
        return "- 기록된 항목 없음\n".to_string();
    }

    checks
        .iter()
        .map(|check| format!("- `{}` [{}] {}\n", check.name, check.status, check.message))
        .collect()
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
        assert_eq!(report.phase, "completed");
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
        let nginx_vhost = fs::read_to_string(fs_root.join("etc/nginx/sites-available/g7.conf"))?;
        assert!(nginx_vhost.contains("proxy_pass http://127.0.0.1:8080;"));
        assert!(nginx_vhost.contains("location /app"));
        assert!(nginx_vhost.contains("access_log /var/log/nginx/g7-access.log g7_timing;"));
        assert!(nginx_vhost.contains("location ^~ /build/"));
        assert!(nginx_vhost.contains("public, max-age=2592000, immutable"));
        assert!(
            fs_root
                .join("etc/nginx/conf.d/g7-runtime-tuning.conf")
                .exists()
        );
        let nginx_runtime =
            fs::read_to_string(fs_root.join("etc/nginx/conf.d/g7-runtime-tuning.conf"))?;
        assert!(nginx_runtime.contains("log_format g7_timing"));
        assert!(nginx_runtime.contains("gzip_comp_level 5"));
        assert!(fs_root.join("etc/php/8.3/fpm/pool.d/g7-g7.conf").exists());
        let php_pool = fs::read_to_string(fs_root.join("etc/php/8.3/fpm/pool.d/g7-g7.conf"))?;
        assert!(php_pool.contains("request_slowlog_timeout = 2s"));
        assert!(php_pool.contains("slowlog = /var/log/php8.3-fpm-g7-slow.log"));
        assert!(
            fs_root
                .join("etc/php/8.3/fpm/conf.d/99-g7-installer.ini")
                .exists()
        );
        assert!(fs_root.join("swapfile").exists());
        assert!(fs_root.join("etc/systemd/system/swapfile.swap").exists());
        assert!(
            fs_root
                .join("etc/sysctl.d/99-g7-installer-swap.conf")
                .exists()
        );
        assert!(fs_root.join("etc/mysql/conf.d/g7-installer.cnf").exists());
        let database_runtime =
            fs::read_to_string(fs_root.join("etc/mysql/conf.d/g7-installer.cnf"))?;
        assert!(database_runtime.contains("slow_query_log = ON"));
        assert!(database_runtime.contains("long_query_time = 0.5"));
        assert!(fs_root.join("etc/g7-installer/secrets.toml").exists());
        assert!(fs_root.join("var/log/g7-installer/setup-guide.md").exists());
        assert!(fs_root.join("home/g7/public_html/.env").exists());
        assert!(
            fs_root
                .join("home/g7/public_html/storage/app/settings/drivers.json")
                .exists()
        );
        assert!(fs_root.join("etc/systemd/system/g7-queue.service").exists());
        assert!(
            fs_root
                .join("etc/systemd/system/g7-scheduler.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/g7-scheduler.timer")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/g7-reverb.service")
                .exists()
        );
        let app_env = fs::read_to_string(fs_root.join("home/g7/public_html/.env"))?;
        assert!(app_env.contains("DB_HOST=localhost"));
        assert!(!app_env.contains("DB_HOST=127.0.0.1"));
        assert!(app_env.contains("CACHE_STORE=redis"));
        assert!(app_env.contains("CACHE_DRIVER=redis"));
        assert!(app_env.contains("SESSION_DRIVER=redis"));
        assert!(app_env.contains("QUEUE_CONNECTION=redis"));
        assert!(app_env.contains("BROADCAST_CONNECTION=reverb"));
        assert!(app_env.contains("VITE_REVERB_HOST=example.com"));
        assert!(app_env.contains("VITE_REVERB_PORT=443"));
        let driver_settings = fs::read_to_string(
            fs_root.join("home/g7/public_html/storage/app/settings/drivers.json"),
        )?;
        assert!(driver_settings.contains("\"cache_driver\": \"redis\""));
        assert!(driver_settings.contains("\"session_driver\": \"redis\""));
        assert!(driver_settings.contains("\"queue_driver\": \"sync\""));
        let recorded = probe.runner().recorded();
        let app_copy_index = recorded
            .iter()
            .position(|spec| {
                spec.display()
                    == "cp -a /var/lib/g7-installer/app-source/gnuboard7/. /home/g7/public_html"
            })
            .ok_or_else(|| std::io::Error::other("missing gnuboard7 app copy command"))?;
        let app_chown_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chown -R g7:www-data /home/g7/public_html")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing app chown command after copy"))?;
        let storage_chmod_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chmod -R 0775 /home/g7/public_html/storage")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing storage chmod command after copy"))?;
        let env_chmod_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chmod 0640 /home/g7/public_html/.env")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing .env chmod command after copy"))?;
        let composer_index = recorded
            .iter()
            .position(|spec| spec.display().starts_with("composer install "))
            .ok_or_else(|| std::io::Error::other("missing composer install command"))?;
        assert!(app_copy_index < app_chown_index);
        assert!(app_chown_index < composer_index);
        assert!(storage_chmod_index < composer_index);
        assert!(env_chmod_index < composer_index);
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
                .any(|check| { check.name == "tls-certificate" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "swapfile" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-fpm-pool" && check.status == "pass" })
        );
        assert!(report.runtime_checks.iter().any(|check| {
            check.name == "phpinfo-summary" && check.message.contains("FPM ini 기준")
        }));
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-runtime-limits" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-extension:pdo_mysql" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-fpm-pool-values" && check.status == "pass" })
        );
        assert!(
            report
                .database_checks
                .iter()
                .any(|check| { check.name == "database-user-created" && check.status == "pass" })
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
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "composer-install" && check.status == "pass" })
        );
        assert!(
            report.app_checks.iter().any(|check| {
                check.name == "gnuboard7-driver-settings" && check.status == "pass"
            })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "artisan-migrate" && check.status == "manual" })
        );
        assert!(report.app_checks.iter().any(|check| {
            check.name == "app-service-file:g7-queue.service" && check.status == "pass"
        }));
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "app-services-enable" && check.status == "manual" })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "app-writable:storage" && check.status == "pass" })
        );
        assert!(report_json.contains("\"network_checks\""));
        assert!(report_json.contains("\"mail_checks\""));
        assert!(report_json.contains("\"certbot_checks\""));
        assert!(report_json.contains("\"runtime_checks\""));
        assert!(report_json.contains("\"database_checks\""));
        assert!(report_json.contains("\"setup_guide_path\""));
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
    fn install_applies_apache_vhost_runtime_tls_and_app_link()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            web_server: "apache".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.web_server, "apache");
        assert_eq!(report.app_url, "https://example.com/install");
        assert!(fs_root.join("etc/apache2/sites-available/g7.conf").exists());
        assert!(fs_root.join("etc/apache2/sites-enabled/g7.conf").exists());
        let apache_vhost = fs::read_to_string(fs_root.join("etc/apache2/sites-available/g7.conf"))?;
        assert!(apache_vhost.contains("ProxyPass /app ws://127.0.0.1:8080/app"));
        assert!(
            report
                .service_checks
                .iter()
                .any(|check| check.name == "apache2" && check.status == "pass")
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| check.name == "apache-vhost" && check.status == "pass")
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| check.name == "apache-runtime-reload" && check.status == "pass")
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| check.name == "apache-https-vhost" && check.status == "pass")
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "app-url" && check.status == "pass")
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn php_runtime_failures_block_app_phase() {
        let message = super::blocking_runtime_failure(&[
            super::InstallCheck::pass("phpinfo-summary", "parsed"),
            super::InstallCheck::fail("php-extension:redis", "redis missing"),
        ])
        .expect("php extension failure should block");

        assert!(message.contains("PHP 런타임 진단 실패"));
        assert!(message.contains("php-extension:redis"));
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
    fn install_laravel_runs_runtime_pipeline_and_services()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            app_profile: "laravel".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.app_profile, "laravel");
        assert_eq!(report.app_url, "https://example.com/");
        assert!(fs_root.join("home/g7/public_html/.env").exists());
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-queue.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-scheduler.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-scheduler.timer")
                .exists()
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "composer-install" && check.status == "pass")
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "artisan-migrate" && check.status == "pass")
        );
        assert!(report.app_checks.iter().any(|check| check.name
            == "app-service:laravel-queue.service"
            && check.status == "pass"));

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
        runner.push_output(CommandOutput::success(
            "php source prerequisites installed\n",
        ));
        runner.push_output(CommandOutput::success("ondrej ppa added\n"));
        runner.push_output(CommandOutput::success("apt update after php source ok\n"));
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

    #[test]
    fn install_adds_ondrej_source_for_php_85() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            php_version: "8.5".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;
        let report_json = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;

        assert_eq!(report.php_version, "8.5");
        assert_eq!(report.php_source, "ondrej");
        assert!(
            report
                .completed_steps
                .contains(&"php-apt-source-added".to_string())
        );
        assert!(
            report
                .completed_steps
                .contains(&"apt-updated-after-php-source".to_string())
        );
        assert!(report_json.contains("\"php_source\": \"ondrej\""));

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
        if install_plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
            runner.push_output(CommandOutput::success(
                "php source prerequisites installed\n",
            ));
            runner.push_output(CommandOutput::success("ondrej ppa added\n"));
            runner.push_output(CommandOutput::success("apt update after php source ok\n"));
        }
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
        runner.push_output(CommandOutput::success(""));
        if install_plan.web_server == "apache" {
            for _module in super::apache_http_modules() {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            push_successful_runtime_database_tls_outputs(runner, install_plan);
            return;
        }

        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        push_successful_runtime_database_tls_outputs(runner, install_plan);
    }

    fn push_successful_runtime_database_tls_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        runner.push_output(CommandOutput::success(""));
        if matches!(install_plan.web_server.as_str(), "nginx" | "apache") {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success(successful_php_runtime_probe_output(
            install_plan,
        )));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        if install_plan.deployment_mode == "public" && install_plan.web_server == "nginx" {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            for _host in super::certificate_hosts(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success("cert issued\n"));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("renew ok\n"));
        } else if install_plan.deployment_mode == "public" && install_plan.web_server == "apache" {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            for _host in super::certificate_hosts(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success("cert issued\n"));
            for _module in super::apache_tls_modules() {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("renew ok\n"));
        }
        push_successful_app_outputs(runner, install_plan);
    }

    fn successful_php_runtime_probe_output(install_plan: &super::plan::InstallPlan) -> String {
        let sizing = super::plan::resolve_memory_sizing(1024 * 1024, 1);
        let extensions = super::required_php_extensions(install_plan).join(",");
        format!(
            "php_version={}\n\
             sapi=cli\n\
             loaded_ini=/etc/php/{}/fpm/php.ini\n\
             scan_dir=/etc/php/{}/fpm/conf.d\n\
             memory_limit={}\n\
             upload_max_filesize={}\n\
             post_max_size={}\n\
             max_execution_time=120\n\
             max_input_vars=3000\n\
             date.timezone=UTC\n\
             realpath_cache_size=4096K\n\
             realpath_cache_ttl=600\n\
             opcache.enable=1\n\
             opcache.memory_consumption={}\n\
             opcache.validate_timestamps=0\n\
             opcache.enable_file_override=1\n\
             extensions={}\n",
            install_plan.php_version,
            install_plan.php_version,
            install_plan.php_version,
            sizing.php_memory_limit,
            sizing.php_upload_limit,
            sizing.php_upload_limit,
            sizing.opcache_memory.trim_end_matches('M'),
            extensions
        )
    }

    fn push_successful_app_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        match install_plan.app_profile.as_str() {
            "gnuboard7" => {
                runner.push_output(CommandOutput::success("cloned\n"));
                push_successful_git_validation_outputs(runner, super::GNUBOARD7_REQUIRED_FILES);
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(runner, super::GNUBOARD7_REQUIRED_FILES, &[]);
                push_successful_app_permission_outputs(runner, install_plan);
                runner.push_output(CommandOutput::success("composer ok\n"));
                runner.push_output(CommandOutput::success("npm install ok\n"));
                runner.push_output(CommandOutput::success("npm build ok\n"));
                runner.push_output(CommandOutput::success("key generated\n"));
                runner.push_output(CommandOutput::success("storage linked\n"));
                runner.push_output(CommandOutput::success(""));
            }
            "wordpress" => {
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(
                    runner,
                    super::WORDPRESS_REQUIRED_FILES,
                    super::WORDPRESS_REQUIRED_DIRS,
                );
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(
                    runner,
                    super::WORDPRESS_REQUIRED_FILES,
                    super::WORDPRESS_REQUIRED_DIRS,
                );
                push_successful_app_permission_outputs(runner, install_plan);
            }
            "laravel" => {
                runner.push_output(CommandOutput::success("cloned\n"));
                push_successful_git_validation_outputs(runner, super::LARAVEL_REQUIRED_FILES);
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(runner, super::LARAVEL_REQUIRED_FILES, &[]);
                push_successful_app_permission_outputs(runner, install_plan);
                runner.push_output(CommandOutput::success("composer ok\n"));
                runner.push_output(CommandOutput::success("npm install ok\n"));
                runner.push_output(CommandOutput::success("npm build ok\n"));
                runner.push_output(CommandOutput::success("key generated\n"));
                runner.push_output(CommandOutput::success("storage linked\n"));
                runner.push_output(CommandOutput::success("migrated\n"));
                runner.push_output(CommandOutput::success("optimized\n"));
                runner.push_output(CommandOutput::success("artisan about\n"));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
            }
            _ => {
                push_successful_app_permission_outputs(runner, install_plan);
            }
        }
    }

    fn push_successful_git_validation_outputs(runner: &FakeCommandRunner, required_files: &[&str]) {
        runner.push_output(CommandOutput::success("deadbeef\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        push_successful_required_path_outputs(runner, required_files, &[]);
    }

    fn push_successful_required_path_outputs(
        runner: &FakeCommandRunner,
        files: &[&str],
        dirs: &[&str],
    ) {
        for _file in files {
            runner.push_output(CommandOutput::success(""));
        }
        for _dir in dirs {
            runner.push_output(CommandOutput::success(""));
        }
    }

    fn push_successful_app_permission_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _writable_path in super::app_writable_paths(install_plan) {
            runner.push_output(CommandOutput::success(""));
        }
        if matches!(install_plan.app_profile.as_str(), "gnuboard7" | "laravel") {
            runner.push_output(CommandOutput::success(""));
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
