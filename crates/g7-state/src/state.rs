use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const STATE_PATH: &str = "/var/lib/g7-installer/state.json";

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
            phase: "initialized".to_string(),
            completed_steps: Vec::new(),
        }
    }
}

pub fn write_state_file(path: &Path, state: &InstallerState) -> io::Result<()> {
    let payload = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
    fs::write(path, payload)
}

#[cfg(test)]
mod tests {
    use super::InstallerState;

    #[test]
    fn new_state_starts_initialized() {
        let state = InstallerState::new("test-id".to_string(), "example.com".to_string());

        assert_eq!(state.version, 1);
        assert_eq!(state.phase, "initialized");
        assert!(state.completed_steps.is_empty());
    }
}
