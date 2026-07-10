use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::account::{
    chmod_path, chmod_recursive, chown_recursive, create_login_user, delete_login_user,
    set_login_password, user_exists,
};
use crate::apache::{config_test as apache_config_test, enable_module as apache_enable_module};
use crate::app::{artisan, composer_install, composer_require, npm_install, npm_run_build};
use crate::apt::{apt_add_repository, apt_candidate_available, apt_install, apt_purge, apt_update};
use crate::archive::{
    copy_dir_contents, download_file, git_clone, git_diff_index_clean, git_fsck_full,
    git_ls_files_error_unmatch, git_rev_parse_head, test_dir, test_file, unzip_archive, unzip_test,
};
use crate::certbot::{certonly_webroot, delete_cert, renew_dry_run};
use crate::command::CommandSpec;
use crate::command::{CommandError, CommandOutput, CommandRunner, RealCommandRunner};
use crate::database::{DatabaseEngine, apply_sql};
use crate::mail::{postconf_set, postfix_preseed};
use crate::network::{
    dns_ipv4_records, dns_ipv6_records, http_host_path_smoke, http_host_smoke, public_ipv4,
    public_ipv6, tcp_connect,
};
use crate::nginx::config_test as nginx_config_test;
use crate::os::{OsRelease, OsReleaseError, read_os_release};
use crate::package::{PackageStatus, package_status};
use crate::port::{PortStatus, tcp_port_status};
use crate::privilege::{Privilege, current_privilege};
use crate::service::{ServiceActivity, disable_now, enable_now, is_active, reload, restart};
use crate::systemd::daemon_reload;
use std::net::IpAddr;

#[derive(Debug)]
pub struct SystemProbe<R> {
    runner: R,
    os_release_path: PathBuf,
    fs_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_kib: u64,
    pub available_kib: u64,
    pub swap_total_kib: u64,
    pub swap_free_kib: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilesystemInfo {
    pub total_kib: u64,
    pub available_kib: u64,
    pub total_inodes: u64,
    pub available_inodes: u64,
}

impl SystemProbe<RealCommandRunner> {
    pub fn real() -> Self {
        Self {
            runner: RealCommandRunner::default(),
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

    pub fn runner(&self) -> &R {
        &self.runner
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

    pub fn apt_add_repository(&self, repository: &str) -> Result<CommandOutput, SystemProbeError> {
        apt_add_repository(&self.runner, repository).map_err(SystemProbeError::Command)
    }

    pub fn apt_purge(&self, packages: &[String]) -> Result<CommandOutput, SystemProbeError> {
        apt_purge(&self.runner, packages).map_err(SystemProbeError::Command)
    }

    pub fn apt_candidate_available(&self, package: &str) -> Result<bool, SystemProbeError> {
        apt_candidate_available(&self.runner, package).map_err(SystemProbeError::Command)
    }

    pub fn postfix_preseed(&self, mailname: &str) -> Result<CommandOutput, SystemProbeError> {
        postfix_preseed(&self.runner, mailname).map_err(SystemProbeError::Command)
    }

    pub fn postconf_set(&self, key: &str, value: &str) -> Result<CommandOutput, SystemProbeError> {
        postconf_set(&self.runner, key, value).map_err(SystemProbeError::Command)
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

    pub fn http_host_path_smoke(&self, host: &str, path: &str) -> Result<bool, SystemProbeError> {
        http_host_path_smoke(&self.runner, host, path).map_err(SystemProbeError::Command)
    }

    pub fn certbot_renew_dry_run(
        &self,
        cert_name: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        renew_dry_run(&self.runner, cert_name).map_err(SystemProbeError::Command)
    }

    pub fn certbot_certonly_webroot(
        &self,
        webroot: &str,
        cert_name: &str,
        domains: &[String],
        email: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        certonly_webroot(&self.runner, webroot, cert_name, domains, email)
            .map_err(SystemProbeError::Command)
    }

    pub fn certbot_delete_cert(&self, cert_name: &str) -> Result<CommandOutput, SystemProbeError> {
        delete_cert(&self.runner, cert_name).map_err(SystemProbeError::Command)
    }

    pub fn database_apply_sql(
        &self,
        engine: DatabaseEngine,
        sql: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        apply_sql(&self.runner, engine, sql).map_err(SystemProbeError::Command)
    }

    pub fn current_privilege(&self) -> Result<Privilege, SystemProbeError> {
        current_privilege(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn enable_service_now(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        enable_now(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn systemd_daemon_reload(&self) -> Result<CommandOutput, SystemProbeError> {
        daemon_reload(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn disable_service_now(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        disable_now(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn reload_service(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        reload(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn restart_service(&self, service: &str) -> Result<CommandOutput, SystemProbeError> {
        restart(&self.runner, service).map_err(SystemProbeError::Command)
    }

    pub fn nginx_config_test(&self) -> Result<CommandOutput, SystemProbeError> {
        nginx_config_test(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn apache_config_test(&self) -> Result<CommandOutput, SystemProbeError> {
        apache_config_test(&self.runner).map_err(SystemProbeError::Command)
    }

    pub fn apache_enable_module(&self, module: &str) -> Result<CommandOutput, SystemProbeError> {
        apache_enable_module(&self.runner, module).map_err(SystemProbeError::Command)
    }

    pub fn download_file(
        &self,
        url: &str,
        output_path: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        download_file(&self.runner, url, output_path).map_err(SystemProbeError::Command)
    }

    pub fn unzip_archive(
        &self,
        archive_path: &str,
        destination: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        unzip_archive(&self.runner, archive_path, destination).map_err(SystemProbeError::Command)
    }

    pub fn unzip_test(&self, archive_path: &str) -> Result<CommandOutput, SystemProbeError> {
        unzip_test(&self.runner, archive_path).map_err(SystemProbeError::Command)
    }

    pub fn git_clone(
        &self,
        repo_url: &str,
        reference: &str,
        destination: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        git_clone(&self.runner, repo_url, reference, destination).map_err(SystemProbeError::Command)
    }

    pub fn git_rev_parse_head(&self, repo_dir: &str) -> Result<CommandOutput, SystemProbeError> {
        git_rev_parse_head(&self.runner, repo_dir).map_err(SystemProbeError::Command)
    }

    pub fn git_fsck_full(&self, repo_dir: &str) -> Result<CommandOutput, SystemProbeError> {
        git_fsck_full(&self.runner, repo_dir).map_err(SystemProbeError::Command)
    }

    pub fn git_diff_index_clean(&self, repo_dir: &str) -> Result<CommandOutput, SystemProbeError> {
        git_diff_index_clean(&self.runner, repo_dir).map_err(SystemProbeError::Command)
    }

    pub fn git_ls_files_error_unmatch(
        &self,
        repo_dir: &str,
        path: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        git_ls_files_error_unmatch(&self.runner, repo_dir, path).map_err(SystemProbeError::Command)
    }

    pub fn copy_dir_contents(
        &self,
        source_dir: &str,
        destination_dir: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        copy_dir_contents(&self.runner, source_dir, destination_dir)
            .map_err(SystemProbeError::Command)
    }

    pub fn test_file(&self, path: &str) -> Result<CommandOutput, SystemProbeError> {
        test_file(&self.runner, path).map_err(SystemProbeError::Command)
    }

    pub fn test_dir(&self, path: &str) -> Result<CommandOutput, SystemProbeError> {
        test_dir(&self.runner, path).map_err(SystemProbeError::Command)
    }

    pub fn composer_install(&self, cwd: &Path) -> Result<CommandOutput, SystemProbeError> {
        composer_install(&self.runner, cwd).map_err(SystemProbeError::Command)
    }

    pub fn composer_require(
        &self,
        cwd: &Path,
        package: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        composer_require(&self.runner, cwd, package).map_err(SystemProbeError::Command)
    }

    pub fn npm_install(&self, cwd: &Path) -> Result<CommandOutput, SystemProbeError> {
        npm_install(&self.runner, cwd).map_err(SystemProbeError::Command)
    }

    pub fn npm_run_build(&self, cwd: &Path) -> Result<CommandOutput, SystemProbeError> {
        npm_run_build(&self.runner, cwd).map_err(SystemProbeError::Command)
    }

    pub fn artisan<I, S>(&self, cwd: &Path, args: I) -> Result<CommandOutput, SystemProbeError>
    where
        I: IntoIterator<Item = S>,
        S: Into<std::ffi::OsString>,
    {
        artisan(&self.runner, cwd, args).map_err(SystemProbeError::Command)
    }

    pub fn user_exists(&self, user: &str) -> Result<bool, SystemProbeError> {
        user_exists(&self.runner, user).map_err(SystemProbeError::Command)
    }

    pub fn create_login_user(&self, user: &str) -> Result<CommandOutput, SystemProbeError> {
        create_login_user(&self.runner, user).map_err(SystemProbeError::Command)
    }

    pub fn set_login_password(
        &self,
        user: &str,
        password: &str,
    ) -> Result<CommandOutput, SystemProbeError> {
        set_login_password(&self.runner, user, password).map_err(SystemProbeError::Command)
    }

    pub fn delete_login_user(&self, user: &str) -> Result<CommandOutput, SystemProbeError> {
        delete_login_user(&self.runner, user).map_err(SystemProbeError::Command)
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

    pub fn chmod_path(&self, mode: &str, path: &str) -> Result<CommandOutput, SystemProbeError> {
        chmod_path(&self.runner, mode, path).map_err(SystemProbeError::Command)
    }

    pub fn path_exists(&self, path: &Path) -> bool {
        self.resolve_path(path).exists()
    }

    pub fn total_memory_kib(&self) -> Result<Option<u64>, SystemProbeError> {
        Ok(self.memory_info()?.map(|info| info.total_kib))
    }

    pub fn memory_info(&self) -> Result<Option<MemoryInfo>, SystemProbeError> {
        let path = self.resolve_path(Path::new("/proc/meminfo"));
        let payload = match fs::read_to_string(&path) {
            Ok(payload) => payload,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(SystemProbeError::Filesystem {
                    path: "/proc/meminfo".to_string(),
                    message: err.to_string(),
                });
            }
        };

        let value = |name: &str| parse_meminfo_value(&payload, name).unwrap_or(0);
        let total_kib = value("MemTotal");
        Ok((total_kib > 0).then_some(MemoryInfo {
            total_kib,
            available_kib: value("MemAvailable"),
            swap_total_kib: value("SwapTotal"),
            swap_free_kib: value("SwapFree"),
        }))
    }

    pub fn root_filesystem_info(&self) -> Result<Option<FilesystemInfo>, SystemProbeError> {
        if self.fs_root != Path::new("/") {
            return Ok(None);
        }
        let blocks = self
            .runner
            .run(&CommandSpec::new("df").args(["-Pk", "--", "/"]))?;
        let inodes = self
            .runner
            .run(&CommandSpec::new("df").args(["-Pi", "--", "/"]))?;
        if blocks.status != 0 || inodes.status != 0 {
            return Ok(None);
        }
        let (total_kib, available_kib) = parse_df_capacity(&blocks.stdout).unwrap_or((0, 0));
        let (total_inodes, available_inodes) = parse_df_capacity(&inodes.stdout).unwrap_or((0, 0));
        Ok(
            (total_kib > 0 && total_inodes > 0).then_some(FilesystemInfo {
                total_kib,
                available_kib,
                total_inodes,
                available_inodes,
            }),
        )
    }

    pub fn vcpu_count(&self) -> Result<Option<usize>, SystemProbeError> {
        let path = self.resolve_path(Path::new("/proc/cpuinfo"));
        let payload = match fs::read_to_string(&path) {
            Ok(payload) => payload,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(SystemProbeError::Filesystem {
                    path: "/proc/cpuinfo".to_string(),
                    message: err.to_string(),
                });
            }
        };

        let count = payload
            .lines()
            .filter(|line| line.trim_start().starts_with("processor"))
            .count();
        Ok((count > 0).then_some(count))
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

fn parse_meminfo_value(payload: &str, name: &str) -> Option<u64> {
    payload.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == name)
            .then(|| value.split_whitespace().next()?.parse().ok())
            .flatten()
    })
}

fn parse_df_capacity(payload: &str) -> Option<(u64, u64)> {
    let line = payload.lines().rfind(|line| !line.trim().is_empty())?;
    let columns = line.split_whitespace().collect::<Vec<_>>();
    Some((columns.get(1)?.parse().ok()?, columns.get(3)?.parse().ok()?))
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
