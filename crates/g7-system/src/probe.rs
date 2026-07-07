use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::account::{chmod_recursive, chown_recursive, create_login_user, user_exists};
use crate::apt::{apt_candidate_available, apt_install, apt_purge, apt_update};
use crate::certbot::renew_dry_run;
use crate::command::{CommandError, CommandOutput, CommandRunner, RealCommandRunner};
use crate::network::{
    dns_ipv4_records, dns_ipv6_records, http_host_smoke, public_ipv4, public_ipv6, tcp_connect,
};
use crate::nginx::config_test;
use crate::os::{OsRelease, OsReleaseError, read_os_release};
use crate::package::{PackageStatus, package_status};
use crate::port::{PortStatus, tcp_port_status};
use crate::privilege::{Privilege, current_privilege};
use crate::service::{ServiceActivity, disable_now, enable_now, is_active, reload};
use std::net::IpAddr;

#[derive(Debug)]
pub struct SystemProbe<R> {
    runner: R,
    os_release_path: PathBuf,
    fs_root: PathBuf,
}

impl SystemProbe<RealCommandRunner> {
    pub fn real() -> Self {
        Self {
            runner: RealCommandRunner,
            os_release_path: PathBuf::from("/etc/os-release"),
            fs_root: PathBuf::from("/"),
        }
    }
}

impl<R: CommandRunner> SystemProbe<R> {
    pub fn new(runner: R) -> Self {
        Self {
            runner,
            os_release_path: PathBuf::from("/etc/os-release"),
            fs_root: PathBuf::from("/"),
        }
    }

    pub fn with_os_release_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.os_release_path = path.into();
        self
    }

    pub fn with_fs_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.fs_root = root.into();
        self
    }

    pub fn os_release(&self) -> Result<OsRelease, SystemProbeError> {
        read_os_release(&self.os_release_path).map_err(SystemProbeError::OsRelease)
    }

    pub fn service_activity(&self, service: &str) -> Result<ServiceActivity, SystemProbeError> {
        is_active(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn package_status(&self, package: &str) -> Result<PackageStatus, SystemProbeError> {
        package_status(&self.runner, package).map_err(SystemProbeError::Command)
    }

    pub fn apt_update(&self) -> Result<CommandOutput, SystemProbeError> {
        apt_update(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn apt_install(&self, packages: &[String]) -> Result<CommandOutput, SystemProbeError> {
        apt_install(&self.runner, packages).map_err(SystemProbeError::Command)
    }

    pub fn apt_purge(&self, packages: &[String]) -> Result<CommandOutput, SystemProbeError> {
        apt_purge(&self.runner, packages).map_err(SystemProbeError::Command)
    }

    pub fn apt_candidate_available(&self, package: &str) -> Result<bool, SystemProbeError> {
        apt_candidate_available(&self.runner, package).map_err(SystemProbeError::Command)
    }

    pub fn tcp_port_status(&self, port: u16) -> Result<PortStatus, SystemProbeError> {
        tcp_port_status(&self.runner, port).map_err(SystemProbeError::Command)
    }

    pub fn public_ipv4(&self) -> Result<Option<IpAddr>, SystemProbeError> {
        public_ipv4(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn public_ipv6(&self) -> Result<Option<IpAddr>, SystemProbeError> {
        public_ipv6(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn dns_ipv4_records(&self, host: &str) -> Result<Vec<IpAddr>, SystemProbeError> {
        dns_ipv4_records(&self.runner, host).map_err(SystemProbeError::Command)
    }

    pub fn dns_ipv6_records(&self, host: &str) -> Result<Vec<IpAddr>, SystemProbeError> {
        dns_ipv6_records(&self.runner, host).map_err(SystemProbeError::Command)
    }

    pub fn tcp_connect(&self, host: &str, port: u16) -> Result<bool, SystemProbeError> {
        tcp_connect(&self.runner, host, port).map_err(SystemProbeError::Command)
    }

    pub fn http_host_smoke(&self, host: &str) -> Result<bool, SystemProbeError> {
        http_host_smoke(&self.runner, host).map_err(SystemProbeError::Command)
    }

    pub fn certbot_renew_dry_run(
        &self,
        cert_name: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        renew_dry_run(&self.runner, cert_name).map_err(SystemProbeError::Command)
    }

    pub fn current_privilege(&self) -> Result<Privilege, SystemProbeError> {
        current_privilege(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn enable_service_now(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        enable_now(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn disable_service_now(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        disable_now(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn reload_service(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        reload(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn nginx_config_test(&self) -> Result<CommandOutput, SystemProbeError> {
        config_test(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn user_exists(&self, user: &str) -> Result<bool, SystemProbeError> {
        user_exists(&self.runner, user).map_err(SystemProbeError::Command)
    }

    pub fn create_login_user(&self, user: &str) -> Result<CommandOutput, SystemProbeError> {
        create_login_user(&self.runner, user).map_err(SystemProbeError::Command)
    }

    pub fn chown_recursive(
        &self,
        owner_group: &str,
        path: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        chown_recursive(&self.runner, owner_group, path).map_err(SystemProbeError::Command)
    }

    pub fn chmod_recursive(
        &self,
        mode: &str,
        path: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        chmod_recursive(&self.runner, mode, path).map_err(SystemProbeError::Command)
    }

    pub fn path_exists(&self, path: &Path) -> bool {
        self.resolve_path(path).exists()
    }

    pub fn directory_entries(&self, path: &Path) -> Result<Vec<PathBuf>, SystemProbeError> {
        let resolved = self.resolve_path(path);
        let entries = match fs::read_dir(&resolved) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(SystemProbeError::Filesystem {
                    path: path.display().to_string(),
                    message: err.to_string(),
                });
            }
        };

        let mut paths = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|err| SystemProbeError::Filesystem {
                path: path.display().to_string(),
                message: err.to_string(),
            })?;
            paths.push(entry.path());
        }

        Ok(paths)
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if self.fs_root == Path::new("/") {
            return path.to_path_buf();
        }

        match path.strip_prefix("/") {
            Ok(stripped) => self.fs_root.join(stripped),
            Err(_) => self.fs_root.join(path),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemProbeError {
    #[error(transparent)]
    Command(#[from] CommandError),

    #[error(transparent)]
    OsRelease(#[from] OsReleaseError),

    #[error("failed to inspect filesystem path {path}: {message}")]
    Filesystem { path: String, message: String },
}
