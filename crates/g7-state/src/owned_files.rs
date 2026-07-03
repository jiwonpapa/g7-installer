use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const OWNED_FILES_PATH: &str = "/var/lib/g7-installer/owned-files.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedFiles {
    pub version: u32,
    pub files: Vec<String>,
}

impl Default for OwnedFiles {
    fn default() -> Self {
        Self {
            version: 1,
            files: Vec::new(),
        }
    }
}

pub fn write_owned_files(path: &Path, owned_files: &OwnedFiles) -> io::Result<()> {
    let payload = serde_json::to_vec_pretty(owned_files).map_err(io::Error::other)?;
    fs::write(path, payload)
}
