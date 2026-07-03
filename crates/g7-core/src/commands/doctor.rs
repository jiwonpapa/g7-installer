use std::path::{Path, PathBuf};

use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use g7_system::command::CommandRunner;
use g7_system::os::OsRelease;
use g7_system::port::PortStatus;
use g7_system::privilege::Privilege;
use g7_system::service::ServiceActivity;
use g7_system::{SystemProbe, SystemProbeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub install_allowed: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: DoctorCheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
    Pending,
}

pub fn run() -> DoctorReport {
    run_with_probe(&SystemProbe::real())
}

pub fn run_with_probe<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorReport {
    let checks = vec![
        ubuntu_check(probe.os_release()),
        privilege_check(probe.current_privilege()),
        nginx_check(probe.service_activity("nginx")),
        apache_check(probe.service_activity("apache2")),
        port_check(80, probe.tcp_port_status(80)),
        port_check(443, probe.tcp_port_status(443)),
        nginx_config_check(probe),
        apache_config_check(probe),
        g7_web_root_check(probe),
        installer_state_check(probe),
        g7_owned_files_check(probe),
        certbot_check(probe),
    ];

    let install_allowed = checks.iter().all(|check| {
        !matches!(
            check.status,
            DoctorCheckStatus::Fail | DoctorCheckStatus::Pending
        )
    });

    DoctorReport {
        install_allowed,
        checks,
    }
}

fn nginx_check(result: Result<ServiceActivity, SystemProbeError>) -> DoctorCheck {
    match result {
        Ok(ServiceActivity::Active) => DoctorCheck {
            name: "nginx-service",
            status: DoctorCheckStatus::Fail,
            message: "Nginx is already running. This installer is for fresh VPS installs."
                .to_string(),
        },
        Ok(ServiceActivity::Inactive | ServiceActivity::NotFound) => DoctorCheck {
            name: "nginx-service",
            status: DoctorCheckStatus::Pass,
            message: "Nginx is not running.".to_string(),
        },
        Ok(ServiceActivity::Unknown) => DoctorCheck {
            name: "nginx-service",
            status: DoctorCheckStatus::Warn,
            message: "Could not determine Nginx service state.".to_string(),
        },
        Err(err) => DoctorCheck {
            name: "nginx-service",
            status: DoctorCheckStatus::Warn,
            message: format!("Could not inspect Nginx service: {err}"),
        },
    }
}

fn ubuntu_check(result: Result<OsRelease, SystemProbeError>) -> DoctorCheck {
    match result {
        Ok(release) if release.is_supported_ubuntu() => DoctorCheck {
            name: "ubuntu-version",
            status: DoctorCheckStatus::Pass,
            message: format!("{} is supported.", release.pretty_name),
        },
        Ok(release) => DoctorCheck {
            name: "ubuntu-version",
            status: DoctorCheckStatus::Fail,
            message: format!(
                "{} is not supported. MVP requires Ubuntu 24.04 LTS.",
                release.pretty_name
            ),
        },
        Err(err) => DoctorCheck {
            name: "ubuntu-version",
            status: DoctorCheckStatus::Fail,
            message: format!("Could not detect Ubuntu version: {err}"),
        },
    }
}

fn privilege_check(result: Result<Privilege, SystemProbeError>) -> DoctorCheck {
    match result {
        Ok(Privilege::Root) => DoctorCheck {
            name: "privilege",
            status: DoctorCheckStatus::Pass,
            message: "Running as root.".to_string(),
        },
        Ok(Privilege::User) => DoctorCheck {
            name: "privilege",
            status: DoctorCheckStatus::Warn,
            message: "Not running as root. doctor can continue, but install requires sudo/root."
                .to_string(),
        },
        Ok(Privilege::Unknown) => DoctorCheck {
            name: "privilege",
            status: DoctorCheckStatus::Warn,
            message: "Could not determine current uid.".to_string(),
        },
        Err(err) => DoctorCheck {
            name: "privilege",
            status: DoctorCheckStatus::Warn,
            message: format!("Could not determine current uid: {err}"),
        },
    }
}

fn apache_check(result: Result<ServiceActivity, SystemProbeError>) -> DoctorCheck {
    match result {
        Ok(ServiceActivity::Active) => DoctorCheck {
            name: "apache-service",
            status: DoctorCheckStatus::Fail,
            message: "Apache is already running. This installer is for fresh VPS installs."
                .to_string(),
        },
        Ok(ServiceActivity::Inactive | ServiceActivity::NotFound) => DoctorCheck {
            name: "apache-service",
            status: DoctorCheckStatus::Pass,
            message: "Apache is not running.".to_string(),
        },
        Ok(ServiceActivity::Unknown) => DoctorCheck {
            name: "apache-service",
            status: DoctorCheckStatus::Warn,
            message: "Could not determine Apache service state.".to_string(),
        },
        Err(err) => DoctorCheck {
            name: "apache-service",
            status: DoctorCheckStatus::Warn,
            message: format!("Could not inspect Apache service: {err}"),
        },
    }
}

fn port_check(port: u16, result: Result<PortStatus, SystemProbeError>) -> DoctorCheck {
    match result {
        Ok(PortStatus::Free) => DoctorCheck {
            name: port_check_name(port),
            status: DoctorCheckStatus::Pass,
            message: format!("TCP port {port} is free."),
        },
        Ok(PortStatus::InUse) => DoctorCheck {
            name: port_check_name(port),
            status: DoctorCheckStatus::Fail,
            message: format!("TCP port {port} is already in use."),
        },
        Ok(PortStatus::Unknown) => DoctorCheck {
            name: port_check_name(port),
            status: DoctorCheckStatus::Warn,
            message: format!("Could not determine TCP port {port} state."),
        },
        Err(err) => DoctorCheck {
            name: port_check_name(port),
            status: DoctorCheckStatus::Warn,
            message: format!("Could not inspect TCP port {port}: {err}"),
        },
    }
}

fn port_check_name(port: u16) -> &'static str {
    match port {
        80 => "port-80",
        443 => "port-443",
        _ => "port",
    }
}

fn nginx_config_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    let paths = [
        Path::new("/etc/nginx/sites-enabled"),
        Path::new("/etc/nginx/conf.d"),
    ];
    let mut existing = Vec::new();

    for path in paths {
        match probe.directory_entries(path) {
            Ok(entries) => existing.extend(entries),
            Err(err) => {
                return DoctorCheck {
                    name: "nginx-config",
                    status: DoctorCheckStatus::Warn,
                    message: format!("Could not inspect {}: {err}", path.display()),
                };
            }
        }
    }

    if existing.is_empty() {
        DoctorCheck {
            name: "nginx-config",
            status: DoctorCheckStatus::Pass,
            message: "No existing Nginx site config entries found.".to_string(),
        }
    } else {
        DoctorCheck {
            name: "nginx-config",
            status: DoctorCheckStatus::Fail,
            message: format!(
                "Found {} existing Nginx config entr{}.",
                existing.len(),
                plural_y(existing.len())
            ),
        }
    }
}

fn apache_config_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    let paths = [
        Path::new("/etc/apache2/sites-enabled"),
        Path::new("/etc/apache2/conf-enabled"),
    ];
    let mut existing = Vec::new();

    for path in paths {
        match probe.directory_entries(path) {
            Ok(entries) => existing.extend(entries),
            Err(err) => {
                return DoctorCheck {
                    name: "apache-config",
                    status: DoctorCheckStatus::Warn,
                    message: format!("Could not inspect {}: {err}", path.display()),
                };
            }
        }
    }

    if existing.is_empty() {
        DoctorCheck {
            name: "apache-config",
            status: DoctorCheckStatus::Pass,
            message: "No existing Apache site config entries found.".to_string(),
        }
    } else {
        DoctorCheck {
            name: "apache-config",
            status: DoctorCheckStatus::Fail,
            message: format!(
                "Found {} existing Apache config entr{}.",
                existing.len(),
                plural_y(existing.len())
            ),
        }
    }
}

fn g7_web_root_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    if probe.path_exists(Path::new("/var/www/g7")) {
        DoctorCheck {
            name: "g7-web-root",
            status: DoctorCheckStatus::Fail,
            message: "/var/www/g7 already exists.".to_string(),
        }
    } else {
        DoctorCheck {
            name: "g7-web-root",
            status: DoctorCheckStatus::Pass,
            message: "/var/www/g7 does not exist.".to_string(),
        }
    }
}

fn installer_state_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    if probe.path_exists(Path::new(STATE_PATH)) {
        DoctorCheck {
            name: "installer-state",
            status: DoctorCheckStatus::Fail,
            message: format!(
                "{STATE_PATH} already exists. Use status/resume handling instead of a fresh install."
            ),
        }
    } else {
        DoctorCheck {
            name: "installer-state",
            status: DoctorCheckStatus::Pass,
            message: "No existing installer state found.".to_string(),
        }
    }
}

fn g7_owned_files_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    let owned_files_path = Path::new(OWNED_FILES_PATH);
    let known_paths = [
        Path::new("/etc/g7-installer/config.toml"),
        Path::new("/var/lib/g7-installer"),
        Path::new("/var/log/g7-installer"),
        Path::new("/etc/nginx/sites-available/g7.conf"),
        Path::new("/etc/nginx/sites-enabled/g7.conf"),
        Path::new("/etc/apache2/sites-available/g7.conf"),
        Path::new("/etc/apache2/sites-enabled/g7.conf"),
        Path::new("/etc/systemd/system/g7-queue.service"),
        Path::new("/etc/systemd/system/g7-reverb.service"),
    ];

    if probe.path_exists(owned_files_path) {
        return DoctorCheck {
            name: "owned-files",
            status: DoctorCheckStatus::Warn,
            message: format!(
                "{OWNED_FILES_PATH} exists. Existing installer ownership metadata was found."
            ),
        };
    }

    let existing = known_paths
        .iter()
        .filter(|path| probe.path_exists(path))
        .map(|path| path.to_path_buf())
        .collect::<Vec<PathBuf>>();

    if existing.is_empty() {
        DoctorCheck {
            name: "owned-files",
            status: DoctorCheckStatus::Pass,
            message: "No unowned G7 installer paths found.".to_string(),
        }
    } else {
        DoctorCheck {
            name: "owned-files",
            status: DoctorCheckStatus::Fail,
            message: format!(
                "Found {} G7-related path{} without {OWNED_FILES_PATH}.",
                existing.len(),
                plural_s(existing.len())
            ),
        }
    }
}

fn certbot_check<R: CommandRunner>(probe: &SystemProbe<R>) -> DoctorCheck {
    match probe.directory_entries(Path::new("/etc/letsencrypt/live")) {
        Ok(entries) if entries.is_empty() => DoctorCheck {
            name: "certbot-live",
            status: DoctorCheckStatus::Pass,
            message: "No existing Let's Encrypt live certificates found.".to_string(),
        },
        Ok(entries) => DoctorCheck {
            name: "certbot-live",
            status: DoctorCheckStatus::Warn,
            message: format!(
                "Found {} existing Let's Encrypt live entr{}. Domain-specific ownership check will run during install.",
                entries.len(),
                plural_y(entries.len())
            ),
        },
        Err(err) => DoctorCheck {
            name: "certbot-live",
            status: DoctorCheckStatus::Warn,
            message: format!("Could not inspect /etc/letsencrypt/live: {err}"),
        },
    }
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

#[cfg(test)]
mod tests {
    use super::{DoctorCheckStatus, run_with_probe};
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn doctor_allows_clean_fresh_server_with_non_root_warning()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let probe = clean_probe(&os_release_path, &fs_root, "1000\n")?;

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.install_allowed);
        assert!(
            report.checks.iter().any(|check| {
                check.name == "privilege" && check.status == DoctorCheckStatus::Warn
            })
        );
        Ok(())
    }

    #[test]
    fn doctor_fails_when_apache_is_active() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("active\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let probe = SystemProbe::new(runner)
            .with_os_release_path(&os_release_path)
            .with_fs_root(&fs_root);

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.checks.iter().any(|check| {
            check.name == "apache-service" && check.status == DoctorCheckStatus::Fail
        }));
        Ok(())
    }

    #[test]
    fn doctor_fails_when_nginx_config_exists() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::write(
            fs_root.join("etc/nginx/sites-enabled/default"),
            "server {}\n",
        )?;
        let probe = clean_probe(&os_release_path, &fs_root, "0\n")?;

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.checks.iter().any(|check| {
            check.name == "nginx-config" && check.status == DoctorCheckStatus::Fail
        }));
        assert!(!report.install_allowed);
        Ok(())
    }

    #[test]
    fn doctor_fails_when_apache_config_exists()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/apache2/sites-enabled"))?;
        fs::write(
            fs_root.join("etc/apache2/sites-enabled/000-default.conf"),
            "<VirtualHost *:80></VirtualHost>\n",
        )?;
        let probe = clean_probe(&os_release_path, &fs_root, "0\n")?;

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.checks.iter().any(|check| {
            check.name == "apache-config" && check.status == DoctorCheckStatus::Fail
        }));
        assert!(!report.install_allowed);
        Ok(())
    }

    #[test]
    fn doctor_fails_when_g7_web_root_exists() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        let probe = clean_probe(&os_release_path, &fs_root, "0\n")?;

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.checks.iter().any(|check| {
            check.name == "g7-web-root" && check.status == DoctorCheckStatus::Fail
        }));
        assert!(!report.install_allowed);
        Ok(())
    }

    #[test]
    fn doctor_fails_when_unowned_g7_path_exists()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        let probe = clean_probe(&os_release_path, &fs_root, "0\n")?;

        let report = run_with_probe(&probe);

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(report.checks.iter().any(|check| {
            check.name == "owned-files" && check.status == DoctorCheckStatus::Fail
        }));
        assert!(!report.install_allowed);
        Ok(())
    }

    fn clean_probe(
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
        let unique = unique_temp_suffix()?;
        path.push(format!("g7-os-release-{}-{unique}.txt", std::process::id()));
        fs::write(
            &path,
            "ID=ubuntu\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\n",
        )?;
        Ok(path)
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        let unique = unique_temp_suffix()?;
        root.push(format!("g7-fs-root-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn unique_temp_suffix() -> std::result::Result<String, Box<dyn std::error::Error>> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{nanos}-{count}"))
    }
}
