use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::command::{CommandError, CommandRunner, RealCommandRunner};
use crate::os::{OsRelease, OsReleaseError, read_os_release};
use crate::package::{PackageStatus, package_status};
use crate::port::{PortStatus, tcp_port_status};
use crate::privilege::{Privilege, current_privilege};
use crate::service::{ServiceActivity, is_active};

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

    pub fn tcp_port_status(&self, port: u16) -> Result<PortStatus, SystemProbeError> {
        tcp_port_status(&self.runner, port).map_err(SystemProbeError::Command)
    }

    pub fn current_privilege(&self) -> Result<Privilege, SystemProbeError> {
        current_privilege(&self.runner).map_err(SystemProbeError::Command)
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
