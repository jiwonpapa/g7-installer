//! Reset installer-owned resources without touching operator-owned server state.
//!
//! This command only removes paths recorded by `owned-files.json` after they pass
//! the explicit reset allowlist below. The allowlist is intentionally narrow:
//! add a regression test whenever a new installer-managed path is introduced.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::installer_paths::{
    BACKUP_DIR, CONFIG_PATH, LETSENCRYPT_LIVE_DIR, MYSQL_CONFIG_CANDIDATE_PATH,
    NGINX_MAIN_BACKUP_PATH, NGINX_MAIN_CONFIG_PATH, REPORT_PATH,
};
use crate::resource_policy::{preserve_package_on_reset, preserve_service_on_reset};
use crate::runtime_resources::G7_RUNTIME_SERVICES;
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, read_owned_files};
use g7_system::SystemProbe;
use g7_system::command::{CommandOutput, CommandRunner};
use g7_system::database::DatabaseEngine;
use g7_system::package::PackageStatus;

const LEGACY_INSTALLER_PATHS: [&str; 2] = ["/usr/local/bin/g7", "/tmp/g7"];
const SWAP_FILE_PATH: &str = "/swapfile";
const SWAP_UNIT_PATH: &str = "/etc/systemd/system/swapfile.swap";
const SWAP_SYSCTL_PATH: &str = "/etc/sysctl.d/99-g7-installer-swap.conf";
const MANAGED_SWAP_MARKERS: [(&str, &str); 2] = [
    (SWAP_UNIT_PATH, "G7 Installer managed swapfile"),
    (SWAP_SYSCTL_PATH, "Managed by g7inst."),
];
const BASELINE_NOT_INSTALLED: &str = "not-installed";
const MYSQL_SERVER_PACKAGE: &str = "mysql-server";
const MYSQL_DATA_DIR: &str = "/var/lib/mysql";
const PHP_SOURCE_ONDREJ: &str = "ondrej";
const ONDREJ_PHP_SOURCE_PATHS: [&str; 3] = [
    "/etc/apt/sources.list.d/ondrej-ubuntu-php-noble.sources",
    "/etc/apt/sources.list.d/ondrej-ubuntu-php-noble.list",
    "/etc/apt/sources.list.d/ondrej-php.list",
];
const APP_SYSTEMD_UNITS: [&str; 7] = [
    "g7-frankenphp.service",
    G7_RUNTIME_SERVICES[0],
    G7_RUNTIME_SERVICES[1],
    G7_RUNTIME_SERVICES[2],
    G7_RUNTIME_SERVICES[3],
    "laravel-queue.service",
    "laravel-scheduler.timer",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    pub dry_run: bool,
    pub actions: Vec<ResetAction>,
    pub removed: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetAction {
    pub name: String,
    pub status: String,
    pub message: String,
}

impl ResetAction {
    fn new(name: impl Into<String>, status: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetPaths {
    root: PathBuf,
}

impl ResetPaths {
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

pub fn run(yes: bool, dry_run: bool) -> Result<ResetReport> {
    run_with_probe_and_paths(yes, dry_run, &SystemProbe::real(), &ResetPaths::system())
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    yes: bool,
    dry_run: bool,
    probe: &SystemProbe<R>,
    paths: &ResetPaths,
) -> Result<ResetReport> {
    if !yes && !dry_run {
        return Err(Error::ResetConfirmationRequired);
    }

    let _operation_lock =
        g7_state::lock::InstallerLock::acquire(&paths.resolve(g7_state::lock::LOCK_PATH), "reset")
            .map_err(|source| Error::OperationLocked {
                operation: "reset",
                source,
            })?;

    require_root(probe)?;
    let metadata = reset_metadata(paths)?;
    let mut actions = Vec::new();
    let certificate_domains = preserved_certificate_domains(paths, &metadata)?;
    for domain in &certificate_domains {
        actions.push(ResetAction::new(
            format!("certificate:{domain}"),
            if dry_run {
                "would-preserve"
            } else {
                "preserved"
            },
            "Let's Encrypt certificate preserved to avoid duplicate issuance limits",
        ));
    }
    if certbot_timer_should_be_preserved(paths, &metadata) {
        actions.push(ResetAction::new(
            "service:certbot.timer",
            if dry_run {
                "would-preserve"
            } else {
                "preserved"
            },
            "인증서 자동 갱신을 보존하기 위해 certbot.timer는 중지하지 않습니다.",
        ));
    }

    if metadata.database_server_installed_by_installer {
        actions.push(ResetAction::new(
            "database",
            if dry_run {
                "would-remove-with-server-data"
            } else {
                "remove-with-server-data"
            },
            "설치기가 새로 구성한 MySQL이므로 개별 DB 삭제 대신 전체 데이터 디렉터리를 초기화합니다.",
        ));
    } else if metadata.database_name.is_some() || metadata.database_user.is_some() {
        let database_name = metadata.database_name.as_deref();
        let database_user = metadata.database_user.as_deref();
        if dry_run {
            actions.push(ResetAction::new(
                "database",
                "would-drop",
                database_reset_message(database_name, database_user),
            ));
        } else {
            let engine =
                DatabaseEngine::from_id(metadata.database_engine.as_deref().unwrap_or("mysql"));
            actions.push(reset_database(probe, engine, database_name, database_user)?);
        }
    }

    let services = reset_services(paths, &metadata);
    if dry_run {
        actions.extend(services.iter().map(|service| {
            ResetAction::new(
                format!("service:{service}"),
                "would-disable",
                "service would be disabled before reset",
            )
        }));
    } else {
        actions.extend(disable_services(probe, &services)?);
    }

    if let Some(site_user) = metadata
        .site_user
        .as_deref()
        .filter(|user| valid_path_segment(user))
    {
        if dry_run {
            actions.push(ResetAction::new(
                format!("account-processes:{site_user}"),
                "would-terminate",
                "사이트 계정으로 실행 중인 프로세스와 SSH/SFTP 세션을 종료한 뒤 계정을 삭제합니다.",
            ));
            actions.push(ResetAction::new(
                format!("account:{site_user}"),
                "would-delete",
                "site Linux account and home directory would be deleted",
            ));
        } else if probe.user_exists(site_user).map_err(command_error)? {
            actions.extend(delete_site_user_account(probe, site_user)?);
        } else {
            actions.push(ResetAction::new(
                format!("account:{site_user}"),
                "missing",
                "site Linux account did not exist",
            ));
        }
    }

    if let Some(action) = restore_nginx_main_config(paths, dry_run)? {
        actions.push(action);
    }

    let packages = metadata.packages_to_purge.clone();
    actions.extend(reset_packages(probe, &packages, dry_run)?);
    if let Some(action) = reset_mysql_data_dir(paths, &metadata, dry_run)? {
        actions.push(action);
    }

    let (removed, missing) = remove_reset_files(paths, &metadata, dry_run)?;

    if !dry_run
        && removed
            .iter()
            .any(|path| path.starts_with("/etc/systemd/system/"))
    {
        let output = probe.systemd_daemon_reload().map_err(command_error)?;
        require_success("systemd-daemon-reload", "systemctl daemon-reload", output)?;
        actions.push(ResetAction::new(
            "systemd:daemon-reload",
            "reloaded",
            "systemd unit cache reloaded after service file removal",
        ));
        if !services.is_empty() {
            let output = probe
                .systemd_reset_failed(&services)
                .map_err(command_error)?;
            if output.status != 0 && !systemd_reset_failed_reports_only_missing(&output) {
                require_success("systemd-reset-failed", "systemctl reset-failed", output)?;
            }
            actions.push(ResetAction::new(
                "systemd:reset-failed",
                "cleared",
                "설치기가 제거한 서비스의 과거 실패 상태를 정리했습니다. 이미 제거된 unit은 건너뛰었습니다.",
            ));
        }
    } else if dry_run && files_contain_systemd_units(&removed) {
        actions.push(ResetAction::new(
            "systemd:daemon-reload",
            "would-reload",
            "systemd unit cache would be reloaded",
        ));
    }

    Ok(ResetReport {
        dry_run,
        actions,
        removed,
        missing,
    })
}

fn delete_site_user_account<R: CommandRunner>(
    probe: &SystemProbe<R>,
    site_user: &str,
) -> Result<Vec<ResetAction>> {
    let mut process_action = terminate_site_user_processes(probe, site_user)?;
    let mut output = probe.delete_login_user(site_user).map_err(command_error)?;
    if output.status == 8 {
        process_action = terminate_site_user_processes(probe, site_user)?;
        output = probe.delete_login_user(site_user).map_err(command_error)?;
    }
    require_success("account-delete", format!("userdel -r {site_user}"), output)?;

    Ok(vec![
        process_action,
        ResetAction::new(
            format!("account:{site_user}"),
            "deleted",
            "site Linux account and home directory deleted",
        ),
    ])
}

fn terminate_site_user_processes<R: CommandRunner>(
    probe: &SystemProbe<R>,
    site_user: &str,
) -> Result<ResetAction> {
    let term = probe
        .signal_login_user_processes("TERM", site_user)
        .map_err(command_error)?;
    let term_matched = term.status == 0;
    if term.status != 0 && term.status != 1 {
        require_success(
            "account-process-stop",
            format!("pkill -TERM -u {site_user}"),
            term,
        )?;
    }

    let kill = probe
        .signal_login_user_processes("KILL", site_user)
        .map_err(command_error)?;
    let kill_matched = kill.status == 0;
    if kill.status != 0 && kill.status != 1 {
        require_success(
            "account-process-kill",
            format!("pkill -KILL -u {site_user}"),
            kill,
        )?;
    }

    let terminated = term_matched || kill_matched;
    Ok(ResetAction::new(
        format!("account-processes:{site_user}"),
        if terminated { "terminated" } else { "none" },
        if terminated {
            "사이트 계정의 잔존 프로세스와 로그인 세션을 종료했습니다."
        } else {
            "사이트 계정으로 실행 중인 프로세스가 없었습니다."
        },
    ))
}

pub fn run_metadata_only_with_probe_and_paths<R: CommandRunner>(
    yes: bool,
    dry_run: bool,
    probe: &SystemProbe<R>,
    paths: &ResetPaths,
) -> Result<ResetReport> {
    if !yes && !dry_run {
        return Err(Error::ResetConfirmationRequired);
    }

    require_root(probe)?;
    let metadata = reset_metadata(paths)?;
    let mut actions = Vec::new();
    if let Some(action) = restore_nginx_main_config(paths, dry_run)? {
        actions.push(action);
    }
    let (removed, missing) = remove_reset_files(paths, &metadata, dry_run)?;

    Ok(ResetReport {
        dry_run,
        actions,
        removed,
        missing,
    })
}

fn restore_nginx_main_config(paths: &ResetPaths, dry_run: bool) -> Result<Option<ResetAction>> {
    let backup = paths.resolve(NGINX_MAIN_BACKUP_PATH);
    if !backup.exists() {
        return Ok(None);
    }
    if dry_run {
        return Ok(Some(ResetAction::new(
            "nginx-main-config",
            "would-restore",
            format!("would restore {NGINX_MAIN_CONFIG_PATH} from {NGINX_MAIN_BACKUP_PATH}"),
        )));
    }
    let target = paths.resolve(NGINX_MAIN_CONFIG_PATH);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::copy(&backup, &target).map_err(|source| Error::FileWriteFailed {
        path: NGINX_MAIN_CONFIG_PATH.to_string(),
        source,
    })?;
    Ok(Some(ResetAction::new(
        "nginx-main-config",
        "restored",
        format!("restored {NGINX_MAIN_CONFIG_PATH} from installer backup"),
    )))
}

fn remove_reset_files(
    paths: &ResetPaths,
    metadata: &ResetMetadata,
    dry_run: bool,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut files = reset_file_list(paths, metadata)?;
    files.sort_by_key(|path| std::cmp::Reverse(path_depth(path)));
    let allow_swapfile_reset = files.iter().any(|path| path == SWAP_UNIT_PATH)
        || managed_marker_file_exists(paths, SWAP_UNIT_PATH, "G7 Installer managed swapfile");

    let mut removed = Vec::new();
    let mut missing = Vec::new();

    for path in files {
        validate_reset_path(&path)?;
        if path == SWAP_FILE_PATH && !allow_swapfile_reset {
            return Err(Error::UnsafeResetPath {
                path: path.to_string(),
            });
        }
        let target = paths.resolve(&path);

        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(path);
                continue;
            }
            Err(source) => {
                return Err(Error::FileReadFailed { path, source });
            }
        };

        if dry_run {
            removed.push(path);
            continue;
        }

        if metadata.file_type().is_dir() {
            fs::remove_dir_all(&target).map_err(|source| Error::FileRemoveFailed {
                path: path.clone(),
                source,
            })?;
        } else {
            fs::remove_file(&target).map_err(|source| Error::FileRemoveFailed {
                path: path.clone(),
                source,
            })?;
        }

        removed.push(path);
    }

    Ok((removed, missing))
}

fn reset_file_list(paths: &ResetPaths, metadata: &ResetMetadata) -> Result<Vec<String>> {
    let metadata_path = paths.resolve(OWNED_FILES_PATH);
    let mut files = match read_owned_files(&metadata_path) {
        Ok(owned) => owned.files,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: OWNED_FILES_PATH.to_string(),
                source,
            });
        }
    };

    for path in LEGACY_INSTALLER_PATHS {
        if !files.iter().any(|owned| owned == path) {
            files.push(path.to_string());
        }
    }

    if metadata.php_source.as_deref() == Some(PHP_SOURCE_ONDREJ) {
        for path in ONDREJ_PHP_SOURCE_PATHS {
            if !files.iter().any(|owned| owned == path) {
                files.push(path.to_string());
            }
        }
    }

    for (path, marker) in MANAGED_SWAP_MARKERS {
        if managed_marker_file_exists(paths, path, marker)
            && !files.iter().any(|owned| owned == path)
        {
            files.push(path.to_string());
        }
    }
    if files.iter().any(|path| path == SWAP_UNIT_PATH)
        && paths.resolve(SWAP_FILE_PATH).exists()
        && !files.iter().any(|owned| owned == SWAP_FILE_PATH)
    {
        files.push(SWAP_FILE_PATH.to_string());
    }

    Ok(files)
}

fn managed_marker_file_exists(paths: &ResetPaths, path: &str, marker: &str) -> bool {
    fs::read_to_string(paths.resolve(path)).is_ok_and(|content| {
        content.contains(marker) || (path == SWAP_SYSCTL_PATH && is_legacy_g7_swap_sysctl(&content))
    })
}

fn is_legacy_g7_swap_sysctl(content: &str) -> bool {
    let normalized = content
        .lines()
        .map(|line| line.split_whitespace().collect::<String>())
        .collect::<Vec<_>>();

    normalized == ["vm.swappiness=10", "vm.vfs_cache_pressure=50"]
}

#[derive(Debug, Clone, Default)]
struct ResetMetadata {
    domain: Option<String>,
    site_user: Option<String>,
    database_engine: Option<String>,
    database_name: Option<String>,
    database_user: Option<String>,
    php_source: Option<String>,
    packages_to_purge: Vec<String>,
    services: Vec<String>,
    certbot_issued: bool,
    database_server_installed_by_installer: bool,
}

fn reset_metadata(paths: &ResetPaths) -> Result<ResetMetadata> {
    let mut metadata = ResetMetadata::default();

    if let Some(report) = read_json_if_exists(paths, REPORT_PATH)? {
        fill_metadata_from_report(&mut metadata, &report);
    }

    if let Some(config) = read_string_if_exists(paths, CONFIG_PATH)? {
        fill_metadata_from_config(&mut metadata, &config);
    }

    metadata.packages_to_purge =
        unique_names(metadata.packages_to_purge.iter().map(String::as_str));
    metadata.services = unique_names(metadata.services.iter().map(String::as_str));
    Ok(metadata)
}

fn read_json_if_exists(paths: &ResetPaths, path: &str) -> Result<Option<serde_json::Value>> {
    let payload = match fs::read(paths.resolve(path)) {
        Ok(payload) => payload,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: path.to_string(),
                source,
            });
        }
    };

    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|source| Error::FileReadFailed {
            path: path.to_string(),
            source: io::Error::other(source),
        })
}

fn read_string_if_exists(paths: &ResetPaths, path: &str) -> Result<Option<String>> {
    match fs::read_to_string(paths.resolve(path)) {
        Ok(payload) => Ok(Some(payload)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::FileReadFailed {
            path: path.to_string(),
            source,
        }),
    }
}

fn fill_metadata_from_report(metadata: &mut ResetMetadata, report: &serde_json::Value) {
    metadata.domain = metadata
        .domain
        .take()
        .or_else(|| report_string(report, "domain"));
    metadata.site_user = metadata
        .site_user
        .take()
        .or_else(|| report_string(report, "site_user"));
    metadata.database_engine = metadata
        .database_engine
        .take()
        .or_else(|| report_string(report, "database"))
        .or_else(|| report_string(report, "database_engine"));
    metadata.database_name = metadata
        .database_name
        .take()
        .or_else(|| report_string(report, "database_name"));
    metadata.database_user = metadata
        .database_user
        .take()
        .or_else(|| report_string(report, "database_user"));
    metadata.php_source = metadata
        .php_source
        .take()
        .or_else(|| report_string(report, "php_source"));
    let preinstall_packages = report_checks(report, "preinstall_package_checks");
    metadata.database_server_installed_by_installer = metadata
        .database_server_installed_by_installer
        || preinstall_packages.iter().any(|check| {
            check.name == MYSQL_SERVER_PACKAGE && check.status == BASELINE_NOT_INSTALLED
        });
    metadata.packages_to_purge.extend(
        preinstall_packages
            .into_iter()
            .filter(|check| check.status == BASELINE_NOT_INSTALLED)
            .map(|check| check.name),
    );
    metadata.services.extend(
        report_checks(report, "service_checks")
            .into_iter()
            .filter(|check| check.status == "pass")
            .map(|check| check.name),
    );
    metadata.certbot_issued = metadata.certbot_issued
        || report_checks(report, "certbot_checks")
            .iter()
            .any(|check| check.name == "tls-certificate" && check.status == "pass");
}

fn fill_metadata_from_config(metadata: &mut ResetMetadata, config: &str) {
    metadata.domain = metadata
        .domain
        .take()
        .or_else(|| config_string(config, "domain"));
    metadata.site_user = metadata
        .site_user
        .take()
        .or_else(|| config_string(config, "site_user"));
    metadata.database_engine = metadata
        .database_engine
        .take()
        .or_else(|| config_string(config, "database"));
    metadata.database_name = metadata
        .database_name
        .take()
        .or_else(|| config_string(config, "database_name"));
    metadata.database_user = metadata
        .database_user
        .take()
        .or_else(|| config_string(config, "database_user"));
    metadata.php_source = metadata
        .php_source
        .take()
        .or_else(|| config_string(config, "php_source"));
}

fn reset_services(paths: &ResetPaths, metadata: &ResetMetadata) -> Vec<String> {
    let mut services = metadata
        .services
        .iter()
        .filter(|service| !preserve_service_on_reset(service))
        .cloned()
        .collect::<Vec<_>>();

    if paths.resolve(SWAP_UNIT_PATH).exists() {
        services.push("swapfile.swap".to_string());
    }

    for unit in APP_SYSTEMD_UNITS {
        if paths
            .resolve(&format!("/etc/systemd/system/{unit}"))
            .exists()
        {
            services.push(unit.to_string());
        }
    }

    unique_names(services.iter().map(String::as_str))
}

fn certbot_timer_should_be_preserved(paths: &ResetPaths, metadata: &ResetMetadata) -> bool {
    metadata
        .services
        .iter()
        .any(|service| preserve_service_on_reset(service))
        || paths.resolve("/etc/systemd/system/certbot.timer").exists()
        || paths.resolve("/lib/systemd/system/certbot.timer").exists()
}

fn preserved_certificate_domains(
    paths: &ResetPaths,
    metadata: &ResetMetadata,
) -> Result<Vec<String>> {
    let mut domains = Vec::new();

    if let Some(domain) = metadata.domain.as_deref().filter(|domain| {
        metadata.certbot_issued
            || paths
                .resolve(&format!("{LETSENCRYPT_LIVE_DIR}/{domain}"))
                .exists()
    }) {
        domains.push(domain.to_string());
    }

    domains.extend(letsencrypt_live_domains(paths)?);
    Ok(unique_names(domains.iter().map(String::as_str)))
}

fn letsencrypt_live_domains(paths: &ResetPaths) -> Result<Vec<String>> {
    let live_dir = paths.resolve(LETSENCRYPT_LIVE_DIR);
    let entries = match fs::read_dir(&live_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: LETSENCRYPT_LIVE_DIR.to_string(),
                source,
            });
        }
    };

    let mut domains = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::FileReadFailed {
            path: LETSENCRYPT_LIVE_DIR.to_string(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "README" || !valid_path_segment(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() || path.is_symlink() {
            domains.push(name);
        }
    }

    Ok(unique_names(domains.iter().map(String::as_str)))
}

fn disable_services<R: CommandRunner>(
    probe: &SystemProbe<R>,
    services: &[String],
) -> Result<Vec<ResetAction>> {
    let mut actions = Vec::new();

    for service in services {
        match probe.service_activity(service).map_err(command_error)? {
            g7_system::service::ServiceActivity::NotFound => {
                actions.push(ResetAction::new(
                    format!("service:{service}"),
                    "missing",
                    "service was already absent",
                ));
                continue;
            }
            g7_system::service::ServiceActivity::Active
            | g7_system::service::ServiceActivity::Inactive
            | g7_system::service::ServiceActivity::Unknown => {}
        }

        let output = probe.disable_service_now(service).map_err(command_error)?;
        if output.status == 0 {
            actions.push(ResetAction::new(
                format!("service:{service}"),
                "disabled",
                "service disabled before reset",
            ));
        } else if service_disable_reports_missing(&output) {
            actions.push(ResetAction::new(
                format!("service:{service}"),
                "missing",
                "service unit was already absent",
            ));
        } else {
            require_success(
                "service-disable",
                format!("systemctl disable --now {service}"),
                output,
            )?;
        }
    }

    Ok(actions)
}

fn service_disable_reports_missing(output: &CommandOutput) -> bool {
    let text = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    text.contains("does not exist")
        || text.contains("not loaded")
        || text.contains("not found")
        || text.contains("could not be found")
}

fn systemd_reset_failed_reports_only_missing(output: &CommandOutput) -> bool {
    let text = format!("{}\n{}", output.stdout, output.stderr);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    std::iter::once(first).chain(lines).all(|line| {
        let line = line.to_ascii_lowercase();
        line.contains("failed to reset failed state of unit")
            && (line.contains("not loaded")
                || line.contains("not found")
                || line.contains("could not be found"))
    })
}

fn reset_packages<R: CommandRunner>(
    probe: &SystemProbe<R>,
    packages: &[String],
    dry_run: bool,
) -> Result<Vec<ResetAction>> {
    let mut actions = Vec::new();
    let mut purge_packages = Vec::new();

    for package in packages {
        if preserve_package_on_reset(package) {
            actions.push(ResetAction::new(
                format!("package:{package}"),
                if dry_run {
                    "would-preserve"
                } else {
                    "preserved"
                },
                "인증서 보존을 위해 certbot 계열 패키지는 apt purge 대상에서 제외했습니다.",
            ));
            continue;
        }

        if dry_run {
            actions.push(ResetAction::new(
                format!("package:{package}"),
                "would-purge",
                "설치기가 설치한 패키지를 제거할 예정입니다.",
            ));
            continue;
        }

        match probe.package_status(package).map_err(command_error)? {
            PackageStatus::Installed | PackageStatus::Unknown => {
                purge_packages.push(package.clone())
            }
            PackageStatus::NotInstalled => actions.push(ResetAction::new(
                format!("package:{package}"),
                "skipped",
                "현재 dpkg 설치 상태가 아니어서 apt purge 대상에서 제외했습니다.",
            )),
        }
    }

    if purge_packages.is_empty() {
        return Ok(actions);
    }

    let mut protected_packages = vec![
        "certbot".to_string(),
        "letsencrypt".to_string(),
        "python3-certbot".to_string(),
    ];
    protected_packages.extend(
        packages
            .iter()
            .filter(|package| preserve_package_on_reset(package))
            .cloned(),
    );
    protected_packages = unique_names(protected_packages.iter().map(String::as_str));
    let mut installed_protected = Vec::new();
    for package in protected_packages {
        if probe.package_status(&package).map_err(command_error)? == PackageStatus::Installed {
            installed_protected.push(package);
        }
    }
    if !installed_protected.is_empty() {
        let output = probe
            .apt_mark_manual(&installed_protected)
            .map_err(command_error)?;
        require_success("package-preserve", "apt-mark manual", output)?;
    }

    let output = probe.apt_purge(&purge_packages).map_err(command_error)?;
    require_success("package-purge", "apt-get purge", output)?;
    actions.extend(purge_packages.iter().map(|package| {
        ResetAction::new(
            format!("package:{package}"),
            "purged",
            "설치기가 설치한 패키지를 제거했습니다.",
        )
    }));

    Ok(actions)
}

fn database_reset_message(database_name: Option<&str>, database_user: Option<&str>) -> String {
    match (database_name, database_user) {
        (Some(database_name), Some(database_user)) => {
            format!("database `{database_name}` and user `{database_user}`")
        }
        (Some(database_name), None) => format!("database `{database_name}`"),
        (None, Some(database_user)) => format!("database user `{database_user}`"),
        (None, None) => "database metadata missing".to_string(),
    }
}

fn reset_mysql_data_dir(
    paths: &ResetPaths,
    metadata: &ResetMetadata,
    dry_run: bool,
) -> Result<Option<ResetAction>> {
    if !metadata.database_server_installed_by_installer {
        return Ok(None);
    }

    let data_dir = paths.resolve(MYSQL_DATA_DIR);
    if !data_dir.exists() {
        return Ok(Some(ResetAction::new(
            "database-data",
            "missing",
            "MySQL 데이터 디렉터리가 이미 없습니다.",
        )));
    }
    if dry_run {
        return Ok(Some(ResetAction::new(
            "database-data",
            "would-delete",
            format!("설치기가 생성한 {MYSQL_DATA_DIR}을 삭제할 예정입니다."),
        )));
    }

    fs::remove_dir_all(&data_dir).map_err(|source| Error::FileRemoveFailed {
        path: MYSQL_DATA_DIR.to_string(),
        source,
    })?;
    Ok(Some(ResetAction::new(
        "database-data",
        "deleted",
        format!("설치기가 생성한 {MYSQL_DATA_DIR}을 삭제했습니다."),
    )))
}

fn reset_database<R: CommandRunner>(
    probe: &SystemProbe<R>,
    engine: DatabaseEngine,
    database_name: Option<&str>,
    database_user: Option<&str>,
) -> Result<ResetAction> {
    let sql = database_reset_sql(database_name, database_user);
    match probe.database_apply_sql(engine, &sql) {
        Ok(output) => {
            let output = retry_database_reset_after_start(probe, engine, &sql, output)?;
            require_success("database-drop", "database root sql", output)?;
            Ok(ResetAction::new(
                "database",
                "dropped",
                database_reset_message(database_name, database_user),
            ))
        }
        Err(g7_system::probe::SystemProbeError::Command(
            g7_system::command::CommandError::Execute { program, .. },
        )) if matches!(program.as_str(), "mysql" | "mariadb") => Ok(ResetAction::new(
            "database",
            "skipped",
            "DB 클라이언트가 이미 제거되어 DB 정리를 건너뛰었습니다. 이전 리셋 시도에서 패키지가 먼저 제거된 상태로 판단합니다.",
        )),
        Err(error) => Err(command_error(error)),
    }
}

fn retry_database_reset_after_start<R: CommandRunner>(
    probe: &SystemProbe<R>,
    engine: DatabaseEngine,
    sql: &str,
    output: CommandOutput,
) -> Result<CommandOutput> {
    if output.status == 0 {
        return Ok(output);
    }

    let service = match engine {
        DatabaseEngine::MariaDb => "mariadb",
        DatabaseEngine::MySql => "mysql",
    };
    if probe.service_activity(service).map_err(command_error)?
        != g7_system::service::ServiceActivity::Inactive
    {
        return Ok(output);
    }

    let start_output = probe.start_service(service).map_err(command_error)?;
    require_success(
        "database-start-for-reset",
        format!("systemctl start {service}"),
        start_output,
    )?;
    probe.database_apply_sql(engine, sql).map_err(command_error)
}

fn database_reset_sql(database_name: Option<&str>, database_user: Option<&str>) -> String {
    let mut sql = String::new();
    if let Some(database_name) = database_name {
        sql.push_str(&format!(
            "DROP DATABASE IF EXISTS `{}`;\n",
            escape_mysql_identifier(database_name)
        ));
    }
    if let Some(database_user) = database_user {
        sql.push_str(&format!(
            "DROP USER IF EXISTS '{}'@'localhost';\n",
            escape_mysql_string(database_user)
        ));
    }
    sql.push_str("FLUSH PRIVILEGES;\n");
    sql
}

fn escape_mysql_identifier(value: &str) -> String {
    value.replace('`', "``")
}

fn escape_mysql_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "''")
}

fn require_success(
    step: &'static str,
    command: impl Into<String>,
    output: CommandOutput,
) -> Result<()> {
    if output.status == 0 {
        return Ok(());
    }

    Err(Error::ResetCommandFailed {
        step,
        command: command.into(),
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn command_error(error: g7_system::probe::SystemProbeError) -> Error {
    Error::ResetCommandFailed {
        step: "command",
        command: error.to_string(),
        status: -1,
        stdout: String::new(),
        stderr: error.to_string(),
    }
}

fn require_root<R: CommandRunner>(probe: &SystemProbe<R>) -> Result<()> {
    match probe.current_privilege() {
        Ok(g7_system::privilege::Privilege::Root) => Ok(()),
        _ => Err(Error::PrivilegeRequired),
    }
}

fn validate_reset_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path == "/" || path.contains("..") {
        return Err(Error::UnsafeResetPath {
            path: path.to_string(),
        });
    }

    let allowed = [
        "/etc/g7-installer",
        "/var/lib/g7-installer",
        "/var/log/g7-installer",
        BACKUP_DIR,
        "/var/www/g7",
        "/etc/nginx/sites-available/g7.conf",
        "/etc/nginx/sites-enabled/g7.conf",
        "/etc/nginx/sites-available/g7-default-deny.conf",
        "/etc/nginx/sites-enabled/g7-default-deny.conf",
        "/etc/apache2/sites-available/g7.conf",
        "/etc/apache2/sites-enabled/g7.conf",
        "/etc/apt/sources.list.d/ondrej-ubuntu-php-noble.sources",
        "/etc/apt/sources.list.d/ondrej-ubuntu-php-noble.list",
        "/etc/apt/sources.list.d/ondrej-php.list",
        SWAP_FILE_PATH,
        SWAP_UNIT_PATH,
        SWAP_SYSCTL_PATH,
        "/etc/systemd/system/g7-frankenphp.service",
        "/opt/g7-frankenphp",
        "/etc/systemd/system/g7-queue.service",
        "/etc/systemd/system/g7-scheduler.service",
        "/etc/systemd/system/g7-scheduler.timer",
        "/etc/systemd/system/g7-reverb.service",
        "/etc/systemd/system/laravel-queue.service",
        "/etc/systemd/system/laravel-scheduler.service",
        "/etc/systemd/system/laravel-scheduler.timer",
        "/usr/local/bin/g7",
        "/tmp/g7",
    ];

    if allowed
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
        || is_safe_site_root(path)
        || is_safe_runtime_config(path)
    {
        Ok(())
    } else {
        Err(Error::UnsafeResetPath {
            path: path.to_string(),
        })
    }
}

fn is_safe_site_root(path: &str) -> bool {
    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() >= 4
        && parts[1] == "home"
        && (parts[3] == "public_html" || parts[3] == "www")
        && valid_path_segment(&parts[2])
    {
        return true;
    }

    parts.len() >= 4 && parts[1] == "var" && parts[2] == "www" && valid_path_segment(&parts[3])
}

fn is_safe_runtime_config(path: &str) -> bool {
    if path == "/etc/nginx/conf.d/g7-runtime-tuning.conf"
        || path == "/etc/mysql/conf.d/g7-installer.cnf"
        || path == MYSQL_CONFIG_CANDIDATE_PATH
        || path == "/etc/mysql/mariadb.conf.d/z-g7-installer.cnf"
        || path == "/etc/apache2/conf-available/g7-runtime.conf"
        || path == "/etc/apache2/conf-enabled/g7-runtime.conf"
    {
        return true;
    }

    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() == 6
        && parts[1] == "var"
        && parts[2] == "lib"
        && parts[3] == "php"
        && parts[4] == "sessions"
        && parts[5].starts_with("g7-")
        && valid_path_segment(&parts[5])
    {
        return true;
    }

    if parts.len() != 7 || parts[1] != "etc" || parts[2] != "php" || !valid_path_segment(&parts[3])
    {
        return false;
    }

    let sapi = parts[4].as_str();
    let config_dir = parts[5].as_str();
    let file_name = parts[6].as_str();

    (sapi == "fpm"
        && config_dir == "pool.d"
        && file_name.starts_with("g7-")
        && file_name.ends_with(".conf"))
        || ((sapi == "fpm" || sapi == "cli")
            && config_dir == "conf.d"
            && file_name == "99-g7-installer.ini")
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportCheck {
    name: String,
    status: String,
}

fn report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report.get(key)?.as_str().map(ToString::to_string)
}

fn report_checks(report: &serde_json::Value, key: &str) -> Vec<ReportCheck> {
    report
        .get(key)
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| {
            Some(ReportCheck {
                name: value.get("name")?.as_str()?.to_string(),
                status: value.get("status")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn config_string(config: &str, key: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let line = line.trim();
        let (left, right) = line.split_once('=')?;
        if left.trim() != key {
            return None;
        }
        let value = right.trim().trim_matches('"').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn unique_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn files_contain_systemd_units(files: &[String]) -> bool {
    files
        .iter()
        .any(|path| path.starts_with("/etc/systemd/system/"))
}

#[cfg(test)]
mod tests {
    use super::{
        APP_SYSTEMD_UNITS, ResetMetadata, ResetPaths, delete_site_user_account, reset_database,
        reset_mysql_data_dir, reset_packages, run_with_probe_and_paths,
        systemd_reset_failed_reports_only_missing, validate_reset_path,
    };
    use crate::installer_paths::MYSQL_CONFIG_CANDIDATE_PATH;
    use crate::runtime_resources::{G7_RUNTIME_FILES, G7_RUNTIME_SERVICES};
    use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
    use g7_system::SystemProbe;
    use g7_system::command::{CommandError, CommandOutput, FakeCommandRunner};
    use g7_system::database::DatabaseEngine;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn systemd_reset_failed_accepts_only_absent_installer_units() {
        let absent = CommandOutput::failure(
            1,
            "Failed to reset failed state of unit g7-queue.service: Unit g7-queue.service not loaded.\nFailed to reset failed state of unit g7-reverb.service: Unit g7-reverb.service not loaded.",
        );
        assert!(systemd_reset_failed_reports_only_missing(&absent));

        let mixed = CommandOutput::failure(
            1,
            "Failed to reset failed state of unit g7-queue.service: Unit g7-queue.service not loaded.\nAccess denied",
        );
        assert!(!systemd_reset_failed_reports_only_missing(&mixed));
    }

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn account_delete_retries_after_userdel_reports_active_process()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::failure(1, "no process"));
        runner.push_output(CommandOutput::failure(1, "no process"));
        runner.push_output(CommandOutput::failure(
            8,
            "user is currently used by process 123",
        ));
        runner.push_output(CommandOutput::success("terminated"));
        runner.push_output(CommandOutput::failure(1, "no remaining process"));
        runner.push_output(CommandOutput::success("deleted"));
        let probe = SystemProbe::new(runner);

        let actions = delete_site_user_account(&probe, "g7")?;

        assert!(actions.iter().any(|action| {
            action.name == "account-processes:g7" && action.status == "terminated"
        }));
        assert!(
            actions
                .iter()
                .any(|action| action.name == "account:g7" && action.status == "deleted")
        );
        assert_eq!(
            probe
                .runner()
                .recorded()
                .iter()
                .filter(|spec| spec.program == "userdel")
                .count(),
            2
        );
        Ok(())
    }

    #[test]
    fn database_reset_starts_inactive_service_and_retries()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::failure(
            1,
            "Can't connect to local MySQL server through socket",
        ));
        runner.push_output(CommandOutput {
            status: 3,
            stdout: "inactive\n".to_string(),
            stderr: String::new(),
        });
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let probe = SystemProbe::new(runner);

        let action = reset_database(
            &probe,
            DatabaseEngine::MySql,
            Some("g7devops"),
            Some("g7devops"),
        )?;

        assert_eq!(action.status, "dropped");
        let recorded = probe.runner().recorded();
        assert_eq!(recorded.len(), 4);
        assert_eq!(recorded[0].program, OsString::from("mysql"));
        assert_eq!(recorded[1].program, OsString::from("systemctl"));
        assert_eq!(
            recorded[1].args,
            vec![OsString::from("is-active"), OsString::from("mysql")]
        );
        assert_eq!(recorded[2].program, OsString::from("systemctl"));
        assert_eq!(
            recorded[2].args,
            vec![OsString::from("start"), OsString::from("mysql")]
        );
        assert_eq!(recorded[3].program, OsString::from("mysql"));
        Ok(())
    }

    #[test]
    fn reset_allows_installer_php_runtime_configs() {
        validate_reset_path("/etc/php/8.5/fpm/conf.d/99-g7-installer.ini")
            .expect("php-fpm override should be resettable");
        validate_reset_path("/etc/php/8.5/cli/conf.d/99-g7-installer.ini")
            .expect("frankenphp cli override should be resettable");
        validate_reset_path("/etc/php/8.5/fpm/pool.d/g7-g7devops.conf")
            .expect("site php-fpm pool should be resettable");

        validate_reset_path("/etc/php/8.5/cli/conf.d/20-opcache.ini")
            .expect_err("operator PHP config must stay outside reset allowlist");
        validate_reset_path("/etc/php/8.5/apache2/conf.d/99-g7-installer.ini")
            .expect_err("unsupported PHP SAPI must stay outside reset allowlist");
    }

    #[test]
    fn reset_allows_only_the_installer_mysql_candidate() {
        validate_reset_path(MYSQL_CONFIG_CANDIDATE_PATH)
            .expect("installer MySQL validation candidate should be resettable");

        validate_reset_path("/etc/mysql/operator.cnf")
            .expect_err("operator MySQL config must stay outside reset allowlist");
    }

    #[test]
    fn reset_contract_covers_all_g7_finalize_resources() {
        for service in G7_RUNTIME_SERVICES {
            assert!(APP_SYSTEMD_UNITS.contains(&service));
        }
        for path in G7_RUNTIME_FILES {
            validate_reset_path(path).expect("G7 finalize unit must be resettable");
        }
        for path in [
            "/home/g7devops/public_html/storage/app/settings/drivers.json",
            "/home/g7devops/public_html/storage/app/settings/mail.json",
            "/home/g7devops/public_html/public/storage",
        ] {
            validate_reset_path(path).expect("G7 finalize site setting must be resettable");
        }
    }

    #[test]
    fn reset_continues_when_database_client_is_already_removed()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "database": "mysql",
                "database_name": "g7devops",
                "database_user": "g7devops"
            }"#,
        )?;
        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/var/log/g7-installer/report.json".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_error(CommandError::Execute {
            program: "mysql".to_string(),
            message: "No such file or directory".to_string(),
        });
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "database" && action.status == "skipped" })
        );
        assert!(!fs_root.join("var/log/g7-installer/report.json").exists());
        assert!(
            !fs_root
                .join("var/lib/g7-installer/owned-files.json")
                .exists()
        );

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_preserves_certbot_timer_instead_of_disabling_it()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "service_checks": [
                    {"name": "certbot.timer", "status": "pass", "message": "was active"}
                ]
            }"#,
        )?;
        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/var/log/g7-installer/report.json".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.actions.iter().any(|action| {
            action.name == "service:certbot.timer" && action.status == "preserved"
        }));
        assert!(!report.actions.iter().any(|action| {
            action.name == "service:certbot.timer" && action.status == "disabled"
        }));
        assert!(!fs_root.join("var/log/g7-installer/report.json").exists());
        assert!(
            !fs_root
                .join("var/lib/g7-installer/owned-files.json")
                .exists()
        );

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_removes_only_owned_paths() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        fs::create_dir_all(fs_root.join("usr/local/bin"))?;
        fs::write(fs_root.join("var/www/g7/test.txt"), "ok")?;
        fs::write(fs_root.join("usr/local/bin/g7"), "old")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/var/www/g7/test.txt".to_string(),
                "/var/www/g7".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/var/www/g7".to_string()));
        assert!(report.removed.contains(&"/usr/local/bin/g7".to_string()));
        assert!(!fs_root.join("var/www/g7").exists());
        assert!(!fs_root.join("usr/local/bin/g7").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_can_remove_legacy_g7_without_owned_metadata()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("usr/local/bin"))?;
        fs::write(fs_root.join("usr/local/bin/g7"), "old")?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/usr/local/bin/g7".to_string()));
        assert!(!fs_root.join("usr/local/bin/g7").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_preserves_existing_letsencrypt_lineage_without_report_success()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::create_dir_all(fs_root.join("etc/letsencrypt/live/g7devops.com"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "preinstall_package_checks": [
                    {"name": "certbot", "status": "not-installed", "message": "fresh"},
                    {"name": "python3-certbot-nginx", "status": "not-installed", "message": "fresh"}
                ],
                "certbot_checks": [
                    {"name": "tls-config", "status": "fail", "message": "rate limited"}
                ],
                "service_checks": [
                    {"name": "certbot.timer", "status": "pass", "message": "active"}
                ]
            }"#,
        )?;
        fs::write(
            fs_root.join("etc/letsencrypt/live/g7devops.com/fullchain.pem"),
            "cert",
        )?;
        fs::write(
            fs_root.join("etc/letsencrypt/live/g7devops.com/privkey.pem"),
            "key",
        )?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, true, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.actions.iter().any(|action| {
            action.name == "certificate:g7devops.com" && action.status == "would-preserve"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "package:certbot" && action.status == "would-preserve"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "package:python3-certbot-nginx" && action.status == "would-preserve"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "service:certbot.timer" && action.status == "would-preserve"
        }));
        assert!(
            !report.actions.iter().any(|action| {
                action.name == "package:certbot" && action.status == "would-purge"
            })
        );
        assert!(!report.actions.iter().any(|action| {
            action.name == "service:certbot.timer" && action.status == "would-disable"
        }));

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_preserves_certbot_packages_even_without_live_certificate()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "domain": "g7devops.com",
                "preinstall_package_checks": [
                    {"name": "nginx", "status": "not-installed", "message": "fresh"},
                    {"name": "certbot", "status": "not-installed", "message": "fresh"},
                    {"name": "python3-certbot-nginx", "status": "not-installed", "message": "fresh"}
                ],
                "certbot_checks": [
                    {"name": "tls-config", "status": "fail", "message": "rate limited"}
                ]
            }"#,
        )?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, true, &probe, &ResetPaths::with_root(&fs_root))?;
        let dry_run_snapshot = report
            .actions
            .iter()
            .map(|action| format!("{}:{}", action.name, action.status))
            .collect::<Vec<_>>();

        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "package:nginx" && action.status == "would-purge" })
        );
        assert!(report.actions.iter().any(|action| {
            action.name == "package:certbot" && action.status == "would-preserve"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "package:python3-certbot-nginx" && action.status == "would-preserve"
        }));
        assert!(
            !report.actions.iter().any(|action| {
                action.name == "package:certbot" && action.status == "would-purge"
            })
        );
        assert!(
            dry_run_snapshot
                .iter()
                .any(|line| line == "package:certbot:would-preserve")
        );
        assert!(
            dry_run_snapshot
                .iter()
                .any(|line| line == "package:python3-certbot-nginx:would-preserve")
        );
        assert!(
            !dry_run_snapshot
                .iter()
                .any(|line| line == "package:certbot:would-purge")
        );

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_removes_created_services_database_account_packages_and_preserves_cert()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::create_dir_all(fs_root.join("etc/systemd/system"))?;
        fs::create_dir_all(fs_root.join("etc/sysctl.d"))?;
        fs::create_dir_all(fs_root.join("etc/apt/sources.list.d"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        fs::create_dir_all(fs_root.join("etc/php/8.3/fpm/pool.d"))?;
        fs::create_dir_all(fs_root.join("etc/mysql/conf.d"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html/storage/app/settings"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html/public"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "domain": "example.com",
                "site_user": "g7",
                "database": "mysql",
                "database_name": "g7_example",
                "database_user": "g7_user",
                "php_source": "ondrej",
                "preinstall_package_checks": [
                    {"name": "nginx", "status": "not-installed", "message": "fresh"},
                    {"name": "php8.3-fpm", "status": "not-installed", "message": "fresh"},
                    {"name": "mysql-server", "status": "not-installed", "message": "fresh"},
                    {"name": "certbot", "status": "not-installed", "message": "fresh"},
                    {"name": "python3-certbot-nginx", "status": "not-installed", "message": "fresh"},
                    {"name": "curl", "status": "installed", "message": "preexisting"}
                ],
                "service_checks": [
                    {"name": "nginx", "status": "pass", "message": "active"},
                    {"name": "php8.3-fpm", "status": "pass", "message": "active"}
                ],
                "certbot_checks": [
                    {"name": "tls-certificate", "status": "pass", "message": "issued"}
                ]
            }"#,
        )?;
        for path in G7_RUNTIME_FILES {
            fs::write(fs_root.join(path.trim_start_matches('/')), "unit")?;
        }
        fs::write(
            fs_root.join("home/g7/public_html/storage/app/settings/drivers.json"),
            r#"{"queue_driver":"redis"}"#,
        )?;
        fs::write(
            fs_root.join("home/g7/public_html/storage/app/settings/mail.json"),
            r#"{"host":"127.0.0.1"}"#,
        )?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            fs_root.join("home/g7/public_html/storage/app/public"),
            fs_root.join("home/g7/public_html/public/storage"),
        )?;
        fs::write(
            fs_root.join("etc/systemd/system/swapfile.swap"),
            "[Unit]\nDescription=G7 Installer managed swapfile\n",
        )?;
        fs::write(
            fs_root.join("etc/sysctl.d/99-g7-installer-swap.conf"),
            "vm.swappiness=10\nvm.vfs_cache_pressure=50\n",
        )?;
        fs::write(fs_root.join("swapfile"), "swap")?;
        fs::write(
            fs_root.join("etc/apt/sources.list.d/ondrej-ubuntu-php-noble.sources"),
            "source",
        )?;
        fs::write(
            fs_root.join("etc/nginx/conf.d/g7-runtime-tuning.conf"),
            "nginx",
        )?;
        fs::write(fs_root.join("etc/php/8.3/fpm/pool.d/g7-g7.conf"), "pool")?;
        fs::write(fs_root.join("etc/mysql/conf.d/g7-installer.cnf"), "mysql")?;
        fs::create_dir_all(fs_root.join("var/lib/mysql"))?;
        fs::write(fs_root.join("var/lib/mysql/mysql.ibd"), "installer data")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                G7_RUNTIME_FILES[0].to_string(),
                G7_RUNTIME_FILES[1].to_string(),
                G7_RUNTIME_FILES[2].to_string(),
                G7_RUNTIME_FILES[3].to_string(),
                "/etc/apt/sources.list.d/ondrej-ubuntu-php-noble.sources".to_string(),
                "/etc/nginx/conf.d/g7-runtime-tuning.conf".to_string(),
                "/etc/php/8.3/fpm/pool.d/g7-g7.conf".to_string(),
                "/etc/mysql/conf.d/g7-installer.cnf".to_string(),
                "/home/g7/public_html/storage/app/settings/drivers.json".to_string(),
                "/home/g7/public_html/storage/app/settings/mail.json".to_string(),
                "/home/g7/public_html/public/storage".to_string(),
                "/home/g7/public_html".to_string(),
                "/var/log/g7-installer/report.json".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        for _service in 0..7 {
            runner.push_output(CommandOutput::success("active\n"));
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("terminated"));
        runner.push_output(CommandOutput::failure(1, "no remaining process"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::success("marked manual"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "account:g7" && action.status == "deleted" })
        );
        assert!(report.actions.iter().any(|action| {
            action.name == "database" && action.status == "remove-with-server-data"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "certificate:example.com" && action.status == "preserved"
        }));
        assert!(report.actions.iter().any(|action| {
            action.name == "service:swapfile.swap" && action.status == "disabled"
        }));
        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "package:nginx" && action.status == "purged" })
        );
        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "package:certbot" && action.status == "preserved" })
        );
        assert!(report.actions.iter().any(|action| {
            action.name == "package:python3-certbot-nginx" && action.status == "preserved"
        }));
        assert!(
            !fs_root
                .join("etc/nginx/conf.d/g7-runtime-tuning.conf")
                .exists()
        );
        assert!(
            !fs_root
                .join("etc/apt/sources.list.d/ondrej-ubuntu-php-noble.sources")
                .exists()
        );
        assert!(!fs_root.join("home/g7/public_html").exists());
        assert!(
            !fs_root
                .join("home/g7/public_html/storage/app/settings/drivers.json")
                .exists()
        );
        assert!(
            !fs_root
                .join("home/g7/public_html/storage/app/settings/mail.json")
                .exists()
        );
        assert!(!fs_root.join("home/g7/public_html/public/storage").exists());
        for path in G7_RUNTIME_FILES {
            assert!(!fs_root.join(path.trim_start_matches('/')).exists());
        }
        assert!(!fs_root.join("etc/systemd/system/swapfile.swap").exists());
        assert!(
            !fs_root
                .join("etc/sysctl.d/99-g7-installer-swap.conf")
                .exists()
        );
        assert!(!fs_root.join("swapfile").exists());
        assert!(!fs_root.join("var/lib/mysql").exists());
        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "database-data" && action.status == "deleted" })
        );

        let recorded = probe.runner().recorded();
        assert_eq!(recorded[0].program, OsString::from("id"));
        assert_ne!(recorded[1].program, OsString::from("mysql"));
        assert!(!recorded.iter().any(|spec| { spec.program == "certbot" }));
        assert!(!recorded.iter().any(|spec| spec.program == "mysql"));
        assert!(recorded.iter().any(|spec| {
            spec.program == "systemctl"
                && spec.args.first() == Some(&OsString::from("reset-failed"))
                && spec.args.contains(&OsString::from("g7-scheduler.service"))
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "pkill"
                && spec.args
                    == vec![
                        OsString::from("-TERM"),
                        OsString::from("-u"),
                        OsString::from("g7"),
                    ]
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "userdel"
                && spec.args == vec![OsString::from("-r"), OsString::from("g7")]
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "apt-mark"
                && spec.args.contains(&OsString::from("manual"))
                && spec.args.contains(&OsString::from("certbot"))
                && spec.args.contains(&OsString::from("python3-certbot-nginx"))
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "env"
                && spec.args.contains(&OsString::from("apt-get"))
                && spec.args.contains(&OsString::from("purge"))
                && spec.args.contains(&OsString::from("nginx"))
                && spec.args.contains(&OsString::from("php8.3-fpm"))
                && !spec.args.contains(&OsString::from("certbot"))
                && !spec.args.contains(&OsString::from("python3-certbot-nginx"))
                && !spec.args.contains(&OsString::from("curl"))
        }));

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_preserves_mysql_data_when_server_predates_installer()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        let mysql_data = fs_root.join("var/lib/mysql");
        fs::create_dir_all(&mysql_data)?;
        fs::write(mysql_data.join("mysql.ibd"), "operator data")?;
        let metadata = ResetMetadata::default();

        let action = reset_mysql_data_dir(&ResetPaths::with_root(&fs_root), &metadata, false)?;

        assert!(action.is_none());
        assert!(mysql_data.join("mysql.ibd").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_purges_installer_owned_partially_configured_package()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("install ok unpacked"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::success("purged"));
        let probe = SystemProbe::new(runner);

        let actions = reset_packages(&probe, &["mysql-server".to_string()], false)?;

        assert!(
            actions.iter().any(|action| {
                action.name == "package:mysql-server" && action.status == "purged"
            })
        );
        assert!(probe.runner().recorded().iter().any(|spec| {
            spec.program == "env"
                && spec.args.contains(&OsString::from("purge"))
                && spec.args.contains(&OsString::from("mysql-server"))
        }));
        Ok(())
    }

    #[test]
    fn reset_skips_missing_package_names_before_purge()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "preinstall_package_checks": [
                    {"name": "nginx", "status": "not-installed", "message": "fresh"},
                    {"name": "g7-frankenphp", "status": "not-installed", "message": "not an apt package"}
                ]
            }"#,
        )?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::failure(1, "dpkg-query: no packages found"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::failure(1, "not installed"));
        runner.push_output(CommandOutput::success(""));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "package:nginx" && action.status == "purged" })
        );
        assert!(report.actions.iter().any(|action| {
            action.name == "package:g7-frankenphp" && action.status == "skipped"
        }));
        let recorded = probe.runner().recorded();
        let purge = recorded
            .iter()
            .find(|spec| {
                spec.program == "env"
                    && spec.args.contains(&OsString::from("apt-get"))
                    && spec.args.contains(&OsString::from("purge"))
            })
            .expect("apt purge command");
        assert!(purge.args.contains(&OsString::from("nginx")));
        assert!(!purge.args.contains(&OsString::from("g7-frankenphp")));

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_keeps_metadata_when_package_purge_fails()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        let report_path = fs_root.join("var/log/g7-installer/report.json");
        fs::write(
            &report_path,
            r#"{
                "preinstall_package_checks": [
                    {"name": "nginx", "status": "not-installed", "message": "fresh"}
                ]
            }"#,
        )?;
        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/var/www/g7".to_string(),
                "/var/log/g7-installer/report.json".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("install ok installed"));
        runner.push_output(CommandOutput::failure(100, "apt purge failed"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let result =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root));

        assert!(result.is_err());
        assert!(report_path.exists());
        assert!(
            fs_root
                .join("var/lib/g7-installer/owned-files.json")
                .exists()
        );
        assert!(fs_root.join("var/www/g7").exists());

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_allows_only_scoped_site_roots() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html/public"))?;
        fs::write(fs_root.join("home/g7/public_html/public/index.php"), "ok")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/home/g7/public_html/public/index.php".to_string(),
                "/home/g7/public_html/public".to_string(),
                "/home/g7/public_html".to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/home/g7/public_html".to_string()));
        assert!(!fs_root.join("home/g7/public_html").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        root.push(format!("g7-reset-fs-root-{}", unique_temp_suffix()?));
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
