//! Cross-process lock for all installer mutations.
//!
//! The lock lives under `/run/lock` so reset can remove installer metadata
//! without unlinking the inode that protects the active operation.

use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

pub const LOCK_PATH: &str = "/run/lock/g7-installer.lock";

#[derive(Debug)]
pub struct InstallerLock {
    file: File,
}

impl InstallerLock {
    pub fn acquire(path: &Path, operation: &str) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock_exclusive()?;
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "pid={} operation={operation}", std::process::id())?;
        file.sync_data()?;

        Ok(Self { file })
    }
}

impl Drop for InstallerLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::InstallerLock;
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn second_process_lock_is_rejected_until_guard_drops() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = temp_lock_path();
        let first = InstallerLock::acquire(&path, "install")?;
        let error =
            InstallerLock::acquire(&path, "reset").expect_err("second lock must be rejected");
        assert_eq!(error.kind(), ErrorKind::WouldBlock);

        drop(first);
        let second = InstallerLock::acquire(&path, "reset")?;
        drop(second);
        fs::remove_file(path)?;
        Ok(())
    }

    fn temp_lock_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "g7-installer-lock-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
