use super::*;
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{self as unix_fs, PermissionsExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileSnapshot {
    path: String,
    kind: String,
    backup: Option<String>,
    link_target: Option<String>,
    mode: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionManifest {
    version: u32,
    install_id: String,
    step: String,
    status: String,
    files: Vec<FileSnapshot>,
}

pub(super) struct StepTransaction {
    paths: InstallPaths,
    manifest_path: String,
    backup_dir: String,
    manifest: TransactionManifest,
}

impl StepTransaction {
    pub(super) fn begin(
        paths: &InstallPaths,
        install_id: &str,
        step: &str,
        files: &[String],
    ) -> Result<Self> {
        let root = format!("{TRANSACTION_DIR}/{install_id}/{step}");
        let backup_dir = format!("{root}/files");
        let manifest_path = format!("{root}/manifest.json");
        fs::create_dir_all(paths.resolve(&backup_dir)).map_err(|source| {
            Error::FileWriteFailed {
                path: backup_dir.clone(),
                source,
            }
        })?;

        let mut transaction = Self {
            paths: paths.clone(),
            manifest_path,
            backup_dir,
            manifest: TransactionManifest {
                version: 1,
                install_id: install_id.to_string(),
                step: step.to_string(),
                status: "started".to_string(),
                files: Vec::new(),
            },
        };
        for path in files {
            transaction.snapshot(path)?;
        }
        transaction.persist()?;
        Ok(transaction)
    }

    pub(super) fn complete(mut self) -> Result<()> {
        self.manifest.status = "completed".to_string();
        self.persist()
    }

    pub(super) fn restore(mut self) -> Result<()> {
        restore_snapshots(&self.paths, &self.manifest.files)?;
        self.manifest.status = "restored".to_string();
        self.persist()
    }

    fn snapshot(&mut self, path: &str) -> Result<()> {
        let target = self.paths.resolve(path);
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.manifest.files.push(FileSnapshot {
                    path: path.to_string(),
                    kind: "missing".to_string(),
                    backup: None,
                    link_target: None,
                    mode: None,
                });
                return Ok(());
            }
            Err(source) => {
                return Err(Error::FileReadFailed {
                    path: path.to_string(),
                    source,
                });
            }
        };

        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&target).map_err(|source| Error::FileReadFailed {
                path: path.to_string(),
                source,
            })?;
            self.manifest.files.push(FileSnapshot {
                path: path.to_string(),
                kind: "symlink".to_string(),
                backup: None,
                link_target: Some(target.display().to_string()),
                mode: None,
            });
            return Ok(());
        }

        if metadata.is_file() {
            let backup = format!(
                "{}/{:04}.backup",
                self.backup_dir,
                self.manifest.files.len()
            );
            fs::copy(&target, self.paths.resolve(&backup)).map_err(|source| {
                Error::FileWriteFailed {
                    path: backup.clone(),
                    source,
                }
            })?;
            #[cfg(unix)]
            fs::set_permissions(
                self.paths.resolve(&backup),
                fs::Permissions::from_mode(0o600),
            )
            .map_err(|source| Error::FileWriteFailed {
                path: backup.clone(),
                source,
            })?;
            #[cfg(unix)]
            let mode = Some(metadata.permissions().mode());
            #[cfg(not(unix))]
            let mode = None;
            self.manifest.files.push(FileSnapshot {
                path: path.to_string(),
                kind: "file".to_string(),
                backup: Some(backup),
                link_target: None,
                mode,
            });
            return Ok(());
        }

        self.manifest.files.push(FileSnapshot {
            path: path.to_string(),
            kind: "directory".to_string(),
            backup: None,
            link_target: None,
            mode: None,
        });
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        let payload = serde_json::to_vec_pretty(&self.manifest).map_err(|source| {
            Error::InstallVerificationFailed {
                checks: format!("transaction manifest serialization failed: {source}"),
            }
        })?;
        g7_state::atomic::atomic_write(&self.paths.resolve(&self.manifest_path), &payload).map_err(
            |source| Error::FileWriteFailed {
                path: self.manifest_path.clone(),
                source,
            },
        )
    }
}

pub(super) fn restore_unfinished_transaction(
    paths: &InstallPaths,
    install_id: &str,
    step: &str,
) -> Result<bool> {
    let manifest_path = format!("{TRANSACTION_DIR}/{install_id}/{step}/manifest.json");
    let payload = match fs::read(paths.resolve(&manifest_path)) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: manifest_path,
                source,
            });
        }
    };
    let mut manifest: TransactionManifest =
        serde_json::from_slice(&payload).map_err(|source| Error::InstallVerificationFailed {
            checks: format!("transaction manifest is invalid: {source}"),
        })?;
    if manifest.status != "started" {
        return Ok(false);
    }
    restore_snapshots(paths, &manifest.files)?;
    manifest.status = "restored".to_string();
    let payload = serde_json::to_vec_pretty(&manifest).map_err(|source| {
        Error::InstallVerificationFailed {
            checks: format!("transaction manifest serialization failed: {source}"),
        }
    })?;
    g7_state::atomic::atomic_write(&paths.resolve(&manifest_path), &payload).map_err(|source| {
        Error::FileWriteFailed {
            path: manifest_path,
            source,
        }
    })?;
    Ok(true)
}

fn restore_snapshots(paths: &InstallPaths, snapshots: &[FileSnapshot]) -> Result<()> {
    for snapshot in snapshots.iter().rev() {
        let target = paths.resolve(&snapshot.path);
        match snapshot.kind.as_str() {
            "missing" => remove_file_or_symlink(&target, &snapshot.path)?,
            "file" => {
                let backup =
                    snapshot
                        .backup
                        .as_deref()
                        .ok_or_else(|| Error::InstallVerificationFailed {
                            checks: format!("missing transaction backup for {}", snapshot.path),
                        })?;
                let payload =
                    fs::read(paths.resolve(backup)).map_err(|source| Error::FileReadFailed {
                        path: backup.to_string(),
                        source,
                    })?;
                g7_state::atomic::atomic_write(&target, &payload).map_err(|source| {
                    Error::FileWriteFailed {
                        path: snapshot.path.clone(),
                        source,
                    }
                })?;
                #[cfg(unix)]
                if let Some(mode) = snapshot.mode {
                    fs::set_permissions(&target, fs::Permissions::from_mode(mode)).map_err(
                        |source| Error::FileWriteFailed {
                            path: snapshot.path.clone(),
                            source,
                        },
                    )?;
                }
            }
            "symlink" => {
                remove_file_or_symlink(&target, &snapshot.path)?;
                #[cfg(unix)]
                unix_fs::symlink(
                    snapshot.link_target.as_deref().ok_or_else(|| {
                        Error::InstallVerificationFailed {
                            checks: format!("missing symlink target for {}", snapshot.path),
                        }
                    })?,
                    &target,
                )
                .map_err(|source| Error::FileWriteFailed {
                    path: snapshot.path.clone(),
                    source,
                })?;
            }
            "directory" => {}
            other => {
                return Err(Error::InstallVerificationFailed {
                    checks: format!("unsupported transaction snapshot kind: {other}"),
                });
            }
        }
    }
    Ok(())
}

fn remove_file_or_symlink(target: &Path, logical: &str) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(target).map_err(|source| Error::FileWriteFailed {
                path: logical.to_string(),
                source,
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::FileReadFailed {
            path: logical.to_string(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        Ok(std::env::temp_dir().join(format!(
            "g7-transaction-{name}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        )))
    }

    #[test]
    fn transaction_restores_existing_and_removes_new_files()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("restore")?;
        let paths = InstallPaths::with_root(&root);
        fs::create_dir_all(root.join("etc/nginx"))?;
        fs::write(root.join("etc/nginx/existing.conf"), "before")?;
        let transaction = StepTransaction::begin(
            &paths,
            "install-id",
            "vhost",
            &[
                "/etc/nginx/existing.conf".to_string(),
                "/etc/nginx/new.conf".to_string(),
            ],
        )?;
        fs::write(root.join("etc/nginx/existing.conf"), "after")?;
        fs::write(root.join("etc/nginx/new.conf"), "new")?;

        transaction.restore()?;

        assert_eq!(
            fs::read_to_string(root.join("etc/nginx/existing.conf"))?,
            "before"
        );
        assert!(!root.join("etc/nginx/new.conf").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn completed_transaction_is_not_restored_on_resume()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("complete")?;
        let paths = InstallPaths::with_root(&root);
        let transaction = StepTransaction::begin(
            &paths,
            "install-id",
            "runtime",
            &["/etc/php/new.ini".to_string()],
        )?;
        transaction.complete()?;

        assert!(!restore_unfinished_transaction(
            &paths,
            "install-id",
            "runtime"
        )?);
        let manifest = fs::read_to_string(
            root.join("var/lib/g7-installer/transactions/install-id/runtime/manifest.json"),
        )?;
        assert!(manifest.contains("\"status\": \"completed\""));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unfinished_transaction_restores_files_symlinks_and_marks_manifest()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("resume")?;
        let paths = InstallPaths::with_root(&root);
        fs::create_dir_all(root.join("etc/nginx"))?;
        fs::write(root.join("etc/nginx/original.conf"), "original")?;
        unix_fs::symlink("original.conf", root.join("etc/nginx/enabled.conf"))?;
        let transaction = StepTransaction::begin(
            &paths,
            "install-id",
            "vhost",
            &[
                "/etc/nginx/original.conf".to_string(),
                "/etc/nginx/enabled.conf".to_string(),
                "/etc/nginx/new.conf".to_string(),
                "/etc/nginx".to_string(),
            ],
        )?;
        fs::write(root.join("etc/nginx/original.conf"), "changed")?;
        fs::remove_file(root.join("etc/nginx/enabled.conf"))?;
        unix_fs::symlink("new.conf", root.join("etc/nginx/enabled.conf"))?;
        fs::write(root.join("etc/nginx/new.conf"), "new")?;
        drop(transaction);

        assert!(restore_unfinished_transaction(
            &paths,
            "install-id",
            "vhost"
        )?);
        assert_eq!(
            fs::read_to_string(root.join("etc/nginx/original.conf"))?,
            "original"
        );
        assert_eq!(
            fs::read_link(root.join("etc/nginx/enabled.conf"))?,
            PathBuf::from("original.conf")
        );
        assert!(!root.join("etc/nginx/new.conf").exists());
        let manifest = fs::read_to_string(
            root.join("var/lib/g7-installer/transactions/install-id/vhost/manifest.json"),
        )?;
        assert!(manifest.contains("\"status\": \"restored\""));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn unfinished_transaction_rejects_invalid_manifest_and_snapshot_kind()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = temp_root("invalid")?;
        let paths = InstallPaths::with_root(&root);
        let manifest_dir = root.join("var/lib/g7-installer/transactions/install-id/runtime");
        fs::create_dir_all(&manifest_dir)?;
        fs::write(manifest_dir.join("manifest.json"), "not-json")?;
        assert!(restore_unfinished_transaction(&paths, "install-id", "runtime").is_err());

        let manifest = TransactionManifest {
            version: 1,
            install_id: "install-id".to_string(),
            step: "runtime".to_string(),
            status: "started".to_string(),
            files: vec![FileSnapshot {
                path: "/etc/php/invalid".to_string(),
                kind: "unsupported".to_string(),
                backup: None,
                link_target: None,
                mode: None,
            }],
        };
        fs::write(
            manifest_dir.join("manifest.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        assert!(restore_unfinished_transaction(&paths, "install-id", "runtime").is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
