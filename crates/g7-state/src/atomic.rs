//! Atomic JSON/file replacement used by installer recovery metadata.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn atomic_write(path: &Path, payload: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "atomic write path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;

    let temp = temp_path(path)?;
    let result = write_and_replace(&temp, path, parent, payload);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_and_replace(temp: &Path, path: &Path, parent: &Path, payload: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(temp)?;
    file.write_all(payload)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    File::open(parent)?.sync_all()
}

fn temp_path(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "atomic write filename is invalid",
            )
        })?;
    Ok(path.with_file_name(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}

#[cfg(test)]
mod tests {
    use super::atomic_write;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn replaces_existing_file_without_leaving_temp_files() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = temp_root();
        fs::create_dir_all(&root)?;
        let path = root.join("state.json");
        atomic_write(&path, b"first")?;
        atomic_write(&path, b"second")?;

        assert_eq!(fs::read_to_string(&path)?, "second");
        assert_eq!(fs::read_dir(&root)?.count(), 1);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "g7-atomic-write-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
