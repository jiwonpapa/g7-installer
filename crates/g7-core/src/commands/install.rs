//! Prepared install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files. It
//! must not silently perform server changes that are not represented in
//! `plan.rs`, `state.json`, and `owned-files.json`.
//!
//! Current phase rule: v0.1.x prepares installer metadata only. App web-root
//! creation, package installation, database creation, Redis hardening, and SSH
//! changes belong to later phases after explicit verification routines exist.

use std::fs;
use std::fs::OpenOptions;
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
        &report_content(&install_plan),
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
    completed_steps.push("state-written".to_string());
    let mut state = InstallerState::new(install_id(&install_plan.domain), install_plan.domain);
    state.phase = "prepared".to_string();
    state.completed_steps = completed_steps.clone();

    let state_path = paths.resolve(STATE_PATH);
    write_state_file(&state_path, &state).map_err(|source| Error::FileWriteFailed {
        path: STATE_PATH.to_string(),
        source,
    })?;

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

fn report_content(plan: &plan::InstallPlan) -> String {
    format!(
        "{{\n  \"version\": 1,\n  \"domain\": \"{}\",\n  \"phase\": \"prepared\",\n  \"site_user\": \"{}\",\n  \"web_root\": \"{}\",\n  \"security_profile\": \"{}\",\n  \"ssh_policy\": \"{}\",\n  \"problem\": null\n}}\n",
        plan.domain, plan.site_user, plan.web_root, plan.security_profile, plan.ssh_policy
    )
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
        assert_eq!(report.php_version, "8.5");
        assert_eq!(report.database_engine, "mysql");
        assert_eq!(report.site_user, "g7");
        assert_eq!(report.web_root_mode, "public-html");
        assert_eq!(report.web_root, "/home/g7/public_html");
        assert_eq!(report.redis_mode, "enable");
        assert_eq!(report.security_profile, "standard");
        assert_eq!(report.ssh_policy, "audit-only");
        assert_eq!(report.phase, "prepared");
        assert!(fs_root.join("etc/g7-installer/config.toml").exists());
        let config = fs::read_to_string(fs_root.join("etc/g7-installer/config.toml"))?;
        assert!(config.contains("deployment_mode = \"public\""));
        assert!(config.contains("web_server = \"nginx\""));
        assert!(config.contains("php_version = \"8.5\""));
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
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);
        let options = super::plan::PlanOptions {
            local_test: true,
            dns_check: true,
            www_mode: "none".to_string(),
            ..super::plan::PlanOptions::default()
        };

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
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(uid));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        Ok(SystemProbe::new(runner)
            .with_os_release_path(os_release_path)
            .with_fs_root(fs_root))
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
