//! Vite build manifest integrity checks shared by install and post-install finalization.

use std::fs;
use std::io;
use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViteManifestAudit {
    pub referenced: Vec<String>,
    pub missing: Vec<String>,
}

pub(crate) fn audit_vite_manifest(
    manifest_path: &Path,
    build_dir: &Path,
) -> Result<ViteManifestAudit> {
    let payload = fs::read(manifest_path).map_err(|source| Error::FileReadFailed {
        path: manifest_path.display().to_string(),
        source,
    })?;
    let manifest = serde_json::from_slice::<serde_json::Value>(&payload).map_err(|source| {
        Error::FileReadFailed {
            path: manifest_path.display().to_string(),
            source: io::Error::other(source),
        }
    })?;
    let mut referenced = Vec::new();
    collect_manifest_assets(&manifest, &mut referenced);
    referenced.sort();
    referenced.dedup();
    let missing = referenced
        .iter()
        .filter(|asset| !build_dir.join(asset).is_file())
        .cloned()
        .collect();
    Ok(ViteManifestAudit {
        referenced,
        missing,
    })
}

fn collect_manifest_assets(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == "file" {
                    if let Some(asset) = value.as_str() {
                        output.push(asset.to_string());
                    }
                } else if matches!(key.as_str(), "css" | "assets") {
                    if let Some(values) = value.as_array() {
                        output.extend(
                            values
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::to_string),
                        );
                    }
                }
                collect_manifest_assets(value, output);
            }
        }
        serde_json::Value::Array(values) => values
            .iter()
            .for_each(|value| collect_manifest_assets(value, output)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn audit_collects_file_css_and_assets_references()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("g7-vite-audit-{nonce}"));
        fs::create_dir_all(root.join("assets"))?;
        fs::write(root.join("assets/app.js"), "ok")?;
        fs::write(
            root.join("manifest.json"),
            r#"{"app":{"file":"assets/app.js","css":["assets/app.css"],"assets":["assets/logo.webp"]}}"#,
        )?;

        let audit = audit_vite_manifest(&root.join("manifest.json"), &root)?;

        assert_eq!(
            audit.referenced,
            vec!["assets/app.css", "assets/app.js", "assets/logo.webp"]
        );
        assert_eq!(audit.missing, vec!["assets/app.css", "assets/logo.webp"]);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
