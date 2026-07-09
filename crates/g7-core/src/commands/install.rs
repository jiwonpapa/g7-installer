//! Server install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files before
//! performing server changes. Every applied package/service step must be
//! represented in `plan.rs`, `state.json`, `owned-files.json`, and the report.
//!
//! Current phase rule: package installation, site account/web root creation,
//! Nginx/Apache/FrankenPHP vhost setup, PHP runtime/DB tuning, DB user creation,
//! TLS vhost mutation, app source handoff, and setup reporting are implemented.
//! Riskier shared-server mutations such as firewall changes remain deferred
//! until their rollback surface is explicit.

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
use crate::defaults::*;
use crate::installer_paths::{
    BACKUP_DIR, BACKUP_MANIFEST_PATH, CONFIG_PATH, ETC_DIR, LIB_DIR, LOCAL_HOSTS_PATH, LOG_DIR,
    LOG_PATH, REPORT_PATH, ROLLBACK_PATH, SECRETS_PATH, SETUP_GUIDE_PATH,
};
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
use g7_state::state::{InstallerPhase, InstallerState, STATE_PATH, write_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandRunner, CommandSpec};
use g7_system::database::DatabaseEngine;
use g7_system::package::PackageStatus;
use g7_system::port::PortStatus;
use g7_system::service::ServiceActivity;

mod apps;
mod database;
mod orchestrator;
mod packages;
mod report;
mod runtime;
mod site;
mod tls;
mod vhost;

pub use orchestrator::{InstallPaths, run, run_with_probe_and_paths};
pub use report::{InstallCheck, InstallReport};

use apps::*;
use database::*;
use orchestrator::*;
use packages::*;
use report::*;
use runtime::*;
use site::*;
use tls::*;
use vhost::*;

#[cfg(test)]
mod tests;
