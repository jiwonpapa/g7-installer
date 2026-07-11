use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

use crate::atomic::atomic_write;

pub const STATE_PATH: &str = "/var/lib/g7-installer/state.json";
pub const STATE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallerStepState {
    pub id: String,
    pub status: String,
    pub attempts: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub restore_status: Option<String>,
}

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
    #[serde(default)]
    pub current_step: Option<String>,
    #[serde(default)]
    pub steps: Vec<InstallerStepState>,
}

impl InstallerState {
    pub fn new(install_id: String, domain: String) -> Self {
        Self {
            version: STATE_VERSION,
            install_id,
            domain,
            phase: InstallerPhase::Initialized.as_str().to_string(),
            completed_steps: Vec::new(),
            current_step: None,
            steps: Vec::new(),
        }
    }

    pub fn set_phase(&mut self, phase: InstallerPhase) {
        self.phase = phase.as_str().to_string();
    }

    pub fn begin_step(&mut self, id: &str) {
        self.version = STATE_VERSION;
        self.current_step = Some(id.to_string());
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.status = "running".to_string();
            step.attempts = step.attempts.saturating_add(1);
            step.last_error = None;
            step.restore_status = None;
        } else {
            self.steps.push(InstallerStepState {
                id: id.to_string(),
                status: "running".to_string(),
                attempts: 1,
                last_error: None,
                restore_status: None,
            });
        }
    }

    pub fn complete_step(&mut self, id: &str) {
        self.version = STATE_VERSION;
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.status = "completed".to_string();
            step.last_error = None;
            step.restore_status = None;
        }
        if self.current_step.as_deref() == Some(id) {
            self.current_step = None;
        }
    }

    pub fn fail_step(&mut self, id: &str, error: impl Into<String>, restored: bool) {
        self.version = STATE_VERSION;
        self.current_step = Some(id.to_string());
        let error = error.into();
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.status = "failed".to_string();
            step.last_error = Some(error);
            step.restore_status =
                Some(if restored { "restored" } else { "not-restored" }.to_string());
        } else {
            self.steps.push(InstallerStepState {
                id: id.to_string(),
                status: "failed".to_string(),
                attempts: 1,
                last_error: Some(error),
                restore_status: Some(
                    if restored { "restored" } else { "not-restored" }.to_string(),
                ),
            });
        }
    }

    pub fn step_is_completed(&self, id: &str) -> bool {
        self.steps
            .iter()
            .any(|step| step.id == id && step.status == "completed")
    }

    pub fn can_retry(&self) -> bool {
        self.phase != InstallerPhase::Completed.as_str()
            && (self.current_step.is_some()
                || self.steps.iter().any(|step| step.status == "failed"))
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
    use super::{InstallerPhase, InstallerState, STATE_VERSION};

    #[test]
    fn new_state_starts_initialized() {
        let state = InstallerState::new("test-id".to_string(), "example.com".to_string());

        assert_eq!(state.version, STATE_VERSION);
        assert_eq!(state.phase, InstallerPhase::Initialized.as_str());
        assert!(state.completed_steps.is_empty());
        assert!(state.current_step.is_none());
        assert!(state.steps.is_empty());
    }

    #[test]
    fn app_mutation_phase_marks_rollback_boundary() {
        assert!(!InstallerPhase::PackagesInstalled.app_mutates_server());
        assert!(InstallerPhase::AppConfigured.app_mutates_server());
    }

    #[test]
    fn step_failure_can_be_retried_without_resetting_install_state() {
        let mut state = InstallerState::new("test-id".to_string(), "example.com".to_string());
        state.begin_step("runtime");
        state.fail_step("runtime", "config mismatch", true);

        assert!(state.can_retry());
        assert_eq!(state.current_step.as_deref(), Some("runtime"));
        assert_eq!(state.steps[0].attempts, 1);
        assert_eq!(state.steps[0].restore_status.as_deref(), Some("restored"));

        state.begin_step("runtime");
        state.complete_step("runtime");
        assert_eq!(state.steps[0].attempts, 2);
        assert!(state.current_step.is_none());
        assert!(state.step_is_completed("runtime"));
    }

    #[test]
    fn version_one_state_deserializes_with_retry_defaults() -> Result<(), Box<dyn std::error::Error>>
    {
        let state: InstallerState = serde_json::from_str(
            r#"{"version":1,"install_id":"old","domain":"example.com","phase":"vhost-enabled","completed_steps":[]}"#,
        )?;

        assert_eq!(state.version, 1);
        assert!(state.current_step.is_none());
        assert!(state.steps.is_empty());
        Ok(())
    }
}
