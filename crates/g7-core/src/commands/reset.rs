use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, read_owned_files};
use g7_system::SystemProbe;
use g7_system::command::{CommandOutput, CommandRunner};
use g7_system::database::DatabaseEngine;

const LEGACY_INSTALLER_PATHS: [&str; 2] = ["/usr/local/bin/g7", "/tmp/g7"];
const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
const CONFIG_PATH: &str = "/etc/g7-installer/config.toml";
const BASELINE_NOT_INSTALLED: &str = "not-installed";
const APP_SYSTEMD_UNITS: [&str; 5] = [
    "g7-queue.service",
    "g7-scheduler.timer",
    "g7-reverb.service",
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

    require_root(probe)?;
    let metadata = reset_metadata(paths)?;
    let mut actions = Vec::new();

    if let Some(domain) = metadata.domain.as_deref().filter(|domain| {
        metadata.certbot_issued
            || paths
                .resolve(&format!("/etc/letsencrypt/live/{domain}"))
                .exists()
    }) {
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

    if metadata.database_name.is_some() || metadata.database_user.is_some() {
        let database_name = metadata.database_name.as_deref();
        let database_user = metadata.database_user.as_deref();
        if dry_run {
            actions.push(ResetAction::new(
                "database",
                "would-drop",
                database_reset_message(database_name, database_user),
            ));
        } else {
            let sql = database_reset_sql(database_name, database_user);
            let engine =
                DatabaseEngine::from_id(metadata.database_engine.as_deref().unwrap_or("mysql"));
            let output = probe
                .database_apply_sql(engine, &sql)
                .map_err(command_error)?;
            require_success("database-drop", "database root sql", output)?;
            actions.push(ResetAction::new(
                "database",
                "dropped",
                database_reset_message(database_name, database_user),
            ));
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
                format!("account:{site_user}"),
                "would-delete",
                "site Linux account and home directory would be deleted",
            ));
        } else if probe.user_exists(site_user).map_err(command_error)? {
            let output = probe.delete_login_user(site_user).map_err(command_error)?;
            require_success("account-delete", format!("userdel -r {site_user}"), output)?;
            actions.push(ResetAction::new(
                format!("account:{site_user}"),
                "deleted",
                "site Linux account and home directory deleted",
            ));
        } else {
            actions.push(ResetAction::new(
                format!("account:{site_user}"),
                "missing",
                "site Linux account did not exist",
            ));
        }
    }

    let (removed, missing) = remove_reset_files(paths, dry_run)?;

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
    } else if dry_run && files_contain_systemd_units(&removed) {
        actions.push(ResetAction::new(
            "systemd:daemon-reload",
            "would-reload",
            "systemd unit cache would be reloaded",
        ));
    }

    let packages = metadata.packages_to_purge;
    if dry_run {
        actions.extend(packages.iter().map(|package| {
            ResetAction::new(
                format!("package:{package}"),
                "would-purge",
                "설치기가 설치한 패키지를 제거할 예정입니다.",
            )
        }));
    } else if !packages.is_empty() {
        let output = probe.apt_purge(&packages).map_err(command_error)?;
        require_success("package-purge", "apt-get purge", output)?;
        actions.extend(packages.iter().map(|package| {
            ResetAction::new(
                format!("package:{package}"),
                "purged",
                "설치기가 설치한 패키지를 제거했습니다.",
            )
        }));
    }

    Ok(ResetReport {
        dry_run,
        actions,
        removed,
        missing,
    })
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
    let (removed, missing) = remove_reset_files(paths, dry_run)?;

    Ok(ResetReport {
        dry_run,
        actions: Vec::new(),
        removed,
        missing,
    })
}

fn remove_reset_files(paths: &ResetPaths, dry_run: bool) -> Result<(Vec<String>, Vec<String>)> {
    let mut files = reset_file_list(paths)?;
    files.sort_by_key(|path| std::cmp::Reverse(path_depth(path)));

    let mut removed = Vec::new();
    let mut missing = Vec::new();

    for path in files {
        validate_reset_path(&path)?;
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

fn reset_file_list(paths: &ResetPaths) -> Result<Vec<String>> {
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

    Ok(files)
}

#[derive(Debug, Clone, Default)]
struct ResetMetadata {
    domain: Option<String>,
    site_user: Option<String>,
    database_engine: Option<String>,
    database_name: Option<String>,
    database_user: Option<String>,
    packages_to_purge: Vec<String>,
    services: Vec<String>,
    certbot_issued: bool,
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
    metadata.packages_to_purge.extend(
        report_checks(report, "preinstall_package_checks")
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
}

fn reset_services(paths: &ResetPaths, metadata: &ResetMetadata) -> Vec<String> {
    let mut services = metadata.services.clone();

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
        require_success(
            "service-disable",
            format!("systemctl disable --now {service}"),
            output,
        )?;
        actions.push(ResetAction::new(
            format!("service:{service}"),
            "disabled",
            "service disabled before reset",
        ));
    }

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
        "/var/backups/g7-installer",
        "/var/www/g7",
        "/etc/nginx/sites-available/g7.conf",
        "/etc/nginx/sites-enabled/g7.conf",
        "/etc/apache2/sites-available/g7.conf",
        "/etc/apache2/sites-enabled/g7.conf",
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
        || path == "/etc/mysql/mariadb.conf.d/60-g7-installer.cnf"
    {
        return true;
    }

    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    parts.len() == 7
        && parts[1] == "etc"
        && parts[2] == "php"
        && valid_path_segment(&parts[3])
        && parts[4] == "fpm"
        && ((parts[5] == "pool.d" && parts[6].starts_with("g7-") && parts[6].ends_with(".conf"))
            || (parts[5] == "conf.d" && parts[6] == "99-g7-installer.ini"))
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
    use super::{ResetPaths, run_with_probe_and_paths};
    use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    fn reset_removes_created_services_database_account_packages_and_preserves_cert()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/log/g7-installer"))?;
        fs::create_dir_all(fs_root.join("etc/systemd/system"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        fs::create_dir_all(fs_root.join("etc/php/8.3/fpm/pool.d"))?;
        fs::create_dir_all(fs_root.join("etc/mysql/conf.d"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html"))?;
        fs::write(
            fs_root.join("var/log/g7-installer/report.json"),
            r#"{
                "domain": "example.com",
                "site_user": "g7",
                "database": "mysql",
                "database_name": "g7_example",
                "database_user": "g7_user",
                "preinstall_package_checks": [
                    {"name": "nginx", "status": "not-installed", "message": "fresh"},
                    {"name": "php8.3-fpm", "status": "not-installed", "message": "fresh"},
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
        fs::write(fs_root.join("etc/systemd/system/g7-queue.service"), "unit")?;
        fs::write(
            fs_root.join("etc/nginx/conf.d/g7-runtime-tuning.conf"),
            "nginx",
        )?;
        fs::write(fs_root.join("etc/php/8.3/fpm/pool.d/g7-g7.conf"), "pool")?;
        fs::write(fs_root.join("etc/mysql/conf.d/g7-installer.cnf"), "mysql")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/etc/systemd/system/g7-queue.service".to_string(),
                "/etc/nginx/conf.d/g7-runtime-tuning.conf".to_string(),
                "/etc/php/8.3/fpm/pool.d/g7-g7.conf".to_string(),
                "/etc/mysql/conf.d/g7-installer.cnf".to_string(),
                "/home/g7/public_html".to_string(),
                "/var/log/g7-installer/report.json".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("active\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("active\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("active\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("0\n"));
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
        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "database" && action.status == "dropped" })
        );
        assert!(report.actions.iter().any(|action| {
            action.name == "certificate:example.com" && action.status == "preserved"
        }));
        assert!(
            report
                .actions
                .iter()
                .any(|action| { action.name == "package:nginx" && action.status == "purged" })
        );
        assert!(
            !fs_root
                .join("etc/nginx/conf.d/g7-runtime-tuning.conf")
                .exists()
        );
        assert!(!fs_root.join("home/g7/public_html").exists());

        let recorded = probe.runner().recorded();
        assert_eq!(recorded[0].program, OsString::from("id"));
        assert_eq!(recorded[1].program, OsString::from("mysql"));
        assert!(!recorded.iter().any(|spec| { spec.program == "certbot" }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "mysql"
                && spec.stdin.as_deref().is_some_and(|stdin| {
                    let sql = String::from_utf8_lossy(stdin);
                    sql.contains("DROP DATABASE IF EXISTS `g7_example`;")
                        && sql.contains("DROP USER IF EXISTS 'g7_user'@'localhost';")
                })
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "userdel"
                && spec.args == vec![OsString::from("-r"), OsString::from("g7")]
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == "env"
                && spec.args.contains(&OsString::from("apt-get"))
                && spec.args.contains(&OsString::from("purge"))
                && spec.args.contains(&OsString::from("nginx"))
                && spec.args.contains(&OsString::from("php8.3-fpm"))
                && !spec.args.contains(&OsString::from("curl"))
        }));

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
