use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, read_owned_files};
use g7_system::SystemProbe;
use g7_system::command::CommandRunner;

const LEGACY_INSTALLER_PATHS: [&str; 2] = ["/usr/local/bin/g7", "/tmp/g7"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetReport {
    pub dry_run: bool,
    pub removed: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetPaths {
    root: PathBuf,
}

impl ResetPaths {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/"),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);

        if self.root == Path::new("/") {
            return path.to_path_buf();
        }

        match path.strip_prefix("/") {
            Ok(stripped) => self.root.join(stripped),
            Err(_) => self.root.join(path),
        }
    }
}

pub fn run(yes: bool, dry_run: bool) -> Result<ResetReport> {
    run_with_probe_and_paths(yes, dry_run, &SystemProbe::real(), &ResetPaths::system())
}

pub fn run_with_probe_and_paths<R: CommandRunner>(
    yes: bool,
    dry_run: bool,
    probe: &SystemProbe<R>,
    paths: &ResetPaths,
) -> Result<ResetReport> {
    if !yes && !dry_run {
        return Err(Error::ResetConfirmationRequired);
    }

    require_root(probe)?;

    let mut files = reset_file_list(paths)?;
    files.sort_by_key(|path| std::cmp::Reverse(path_depth(path)));

    let mut removed = Vec::new();
    let mut missing = Vec::new();

    for path in files {
        validate_reset_path(&path)?;
        let target = paths.resolve(&path);

        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(path);
                continue;
            }
            Err(source) => {
                return Err(Error::FileReadFailed { path, source });
            }
        };

        if dry_run {
            removed.push(path);
            continue;
        }

        if metadata.file_type().is_dir() {
            fs::remove_dir_all(&target).map_err(|source| Error::FileRemoveFailed {
                path: path.clone(),
                source,
            })?;
        } else {
            fs::remove_file(&target).map_err(|source| Error::FileRemoveFailed {
                path: path.clone(),
                source,
            })?;
        }

        removed.push(path);
    }

    Ok(ResetReport {
        dry_run,
        removed,
        missing,
    })
}

fn reset_file_list(paths: &ResetPaths) -> Result<Vec<String>> {
    let metadata_path = paths.resolve(OWNED_FILES_PATH);
    let mut files = match read_owned_files(&metadata_path) {
        Ok(owned) => owned.files,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: OWNED_FILES_PATH.to_string(),
                source,
            });
        }
    };

    for path in LEGACY_INSTALLER_PATHS {
        if !files.iter().any(|owned| owned == path) {
            files.push(path.to_string());
        }
    }

    Ok(files)
}

fn require_root<R: CommandRunner>(probe: &SystemProbe<R>) -> Result<()> {
    match probe.current_privilege() {
        Ok(g7_system::privilege::Privilege::Root) => Ok(()),
        _ => Err(Error::PrivilegeRequired),
    }
}

fn validate_reset_path(path: &str) -> Result<()> {
    if !path.starts_with('/') || path == "/" || path.contains("..") {
        return Err(Error::UnsafeResetPath {
            path: path.to_string(),
        });
    }

    let allowed = [
        "/etc/g7-installer",
        "/var/lib/g7-installer",
        "/var/log/g7-installer",
        "/var/backups/g7-installer",
        "/var/www/g7",
        "/etc/nginx/sites-available/g7.conf",
        "/etc/nginx/sites-enabled/g7.conf",
        "/etc/apache2/sites-available/g7.conf",
        "/etc/apache2/sites-enabled/g7.conf",
        "/etc/systemd/system/g7-queue.service",
        "/etc/systemd/system/g7-reverb.service",
        "/usr/local/bin/g7",
        "/tmp/g7",
    ];

    if allowed
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
        || is_safe_site_root(path)
    {
        Ok(())
    } else {
        Err(Error::UnsafeResetPath {
            path: path.to_string(),
        })
    }
}

fn is_safe_site_root(path: &str) -> bool {
    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() >= 4
        && parts[1] == "home"
        && (parts[3] == "public_html" || parts[3] == "www")
        && valid_path_segment(&parts[2])
    {
        return true;
    }

    parts.len() >= 4 && parts[1] == "var" && parts[2] == "www" && valid_path_segment(&parts[3])
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

#[cfg(test)]
mod tests {
    use super::{ResetPaths, run_with_probe_and_paths};
    use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn reset_removes_only_owned_paths() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        fs::create_dir_all(fs_root.join("usr/local/bin"))?;
        fs::write(fs_root.join("var/www/g7/test.txt"), "ok")?;
        fs::write(fs_root.join("usr/local/bin/g7"), "old")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/var/www/g7/test.txt".to_string(),
                "/var/www/g7".to_string(),
                OWNED_FILES_PATH.to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/var/www/g7".to_string()));
        assert!(report.removed.contains(&"/usr/local/bin/g7".to_string()));
        assert!(!fs_root.join("var/www/g7").exists());
        assert!(!fs_root.join("usr/local/bin/g7").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_can_remove_legacy_g7_without_owned_metadata()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("usr/local/bin"))?;
        fs::write(fs_root.join("usr/local/bin/g7"), "old")?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/usr/local/bin/g7".to_string()));
        assert!(!fs_root.join("usr/local/bin/g7").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn reset_allows_only_scoped_site_roots() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/lib/g7-installer"))?;
        fs::create_dir_all(fs_root.join("home/g7/public_html/public"))?;
        fs::write(fs_root.join("home/g7/public_html/public/index.php"), "ok")?;

        let owned = OwnedFiles {
            version: 1,
            files: vec![
                "/home/g7/public_html/public/index.php".to_string(),
                "/home/g7/public_html/public".to_string(),
                "/home/g7/public_html".to_string(),
            ],
        };
        write_owned_files(&fs_root.join(strip_root(OWNED_FILES_PATH)), &owned)?;

        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let report =
            run_with_probe_and_paths(true, false, &probe, &ResetPaths::with_root(&fs_root))?;

        assert!(report.removed.contains(&"/home/g7/public_html".to_string()));
        assert!(!fs_root.join("home/g7/public_html").exists());
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        root.push(format!("g7-reset-fs-root-{}", unique_temp_suffix()?));
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn unique_temp_suffix() -> std::result::Result<String, Box<dyn std::error::Error>> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{}-{nanos}-{count}", std::process::id()))
    }

    fn strip_root(path: &str) -> &str {
        match path.strip_prefix('/') {
            Some(stripped) => stripped,
            None => path,
        }
    }
}
