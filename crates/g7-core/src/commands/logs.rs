use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLocation {
    pub path: PathBuf,
    pub command_audit_path: PathBuf,
}

pub fn location() -> LogLocation {
    LogLocation {
        path: PathBuf::from("/var/log/g7-installer/install.log"),
        command_audit_path: PathBuf::from("/var/log/g7-installer/commands.jsonl"),
    }
}
