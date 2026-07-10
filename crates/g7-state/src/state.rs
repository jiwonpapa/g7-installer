use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

use crate::atomic::atomic_write;

pub const STATE_PATH: &str = "/var/lib/g7-installer/state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerPhase {
    Initialized,
    Prepared,
    PackageFailed,
    PackagesInstalled,
    VhostFailed,
    RuntimeConfigured,
    DatabaseConfigured,
    AppFetched,
    AppConfigured,
    VhostEnabled,
    TlsEnabled,
    HealthChecked,
    Completed,
}

impl InstallerPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Initialized => "initialized",
            Self::Prepared => "prepared",
            Self::PackageFailed => "package-failed",
            Self::PackagesInstalled => "packages-installed",
            Self::VhostFailed => "vhost-failed",
            Self::RuntimeConfigured => "runtime-configured",
            Self::DatabaseConfigured => "database-configured",
            Self::AppFetched => "app-fetched",
            Self::AppConfigured => "app-configured",
            Self::VhostEnabled => "vhost-enabled",
            Self::TlsEnabled => "tls-enabled",
            Self::HealthChecked => "health-checked",
            Self::Completed => "completed",
        }
    }

    pub fn app_mutates_server(self) -> bool {
        matches!(
            self,
            Self::VhostFailed
                | Self::RuntimeConfigured
                | Self::DatabaseConfigured
                | Self::AppFetched
                | Self::AppConfigured
                | Self::VhostEnabled
                | Self::TlsEnabled
                | Self::HealthChecked
                | Self::Completed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallerState {
    pub version: u32,
    pub install_id: String,
    pub domain: String,
    pub phase: String,
    pub completed_steps: Vec<String>,
}

impl InstallerState {
    pub fn new(install_id: String, domain: String) -> Self {
        Self {
            version: 1,
            install_id,
            domain,
            phase: InstallerPhase::Initialized.as_str().to_string(),
            completed_steps: Vec::new(),
        }
    }

    pub fn set_phase(&mut self, phase: InstallerPhase) {
        self.phase = phase.as_str().to_string();
    }
}

pub fn write_state_file(path: &Path, state: &InstallerState) -> io::Result<()> {
    let payload = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    atomic_write(path, &payload)
}

pub fn read_state_file(path: &Path) -> io::Result<InstallerState> {
    let payload = fs::read(path)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::{InstallerPhase, InstallerState};

    #[test]
    fn new_state_starts_initialized() {
        let state = InstallerState::new("test-id".to_string(), "example.com".to_string());

        assert_eq!(state.version, 1);
        assert_eq!(state.phase, InstallerPhase::Initialized.as_str());
        assert!(state.completed_steps.is_empty());
    }

    #[test]
    fn app_mutation_phase_marks_rollback_boundary() {
        assert!(!InstallerPhase::PackagesInstalled.app_mutates_server());
        assert!(InstallerPhase::AppConfigured.app_mutates_server());
    }
}
