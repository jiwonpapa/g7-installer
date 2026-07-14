//! Linux distribution release detection and installer support policy.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const MIN_UBUNTU_VERSION: (u16, u16) = (22, 4);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsRelease {
    pub id: String,
    pub version_id: String,
    pub pretty_name: String,
}

impl OsRelease {
    /// Returns true for Ubuntu 22.04 and every newer Ubuntu release.
    pub fn is_supported_ubuntu(&self) -> bool {
        self.id == "ubuntu"
            && parse_version_pair(&self.version_id)
                .is_some_and(|version| version >= MIN_UBUNTU_VERSION)
    }
}

fn parse_version_pair(value: &str) -> Option<(u16, u16)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;

    Some((major, minor))
}

pub fn read_os_release(path: &Path) -> Result<OsRelease, OsReleaseError> {
    let content = fs::read_to_string(path).map_err(|err| OsReleaseError::Read {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;

    parse_os_release(&content)
}

pub fn parse_os_release(content: &str) -> Result<OsRelease, OsReleaseError> {
    let mut values = BTreeMap::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        values.insert(key.to_string(), unquote(value));
    }

    let id = required(&values, "ID")?;
    let version_id = required(&values, "VERSION_ID")?;
    let pretty_name = match values.get("PRETTY_NAME") {
        Some(value) => value.clone(),
        None => format!("{id} {version_id}"),
    };

    Ok(OsRelease {
        id,
        version_id,
        pretty_name,
    })
}

fn required(
    values: &BTreeMap<String, String>,
    key: &'static str,
) -> Result<String, OsReleaseError> {
    values
        .get(key)
        .cloned()
        .filter(|value| !value.is_empty())
        .ok_or(OsReleaseError::MissingField { field: key })
}

fn unquote(value: &str) -> String {
    let value = value.trim();

    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OsReleaseError {
    #[error("failed to read os-release from {path}: {message}")]
    Read { path: String, message: String },

    #[error("os-release is missing {field}")]
    MissingField { field: &'static str },
}

#[cfg(test)]
mod tests {
    use super::parse_os_release;

    #[test]
    fn parses_supported_ubuntu_release() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let release = parse_os_release(
            r#"
ID=ubuntu
VERSION_ID="24.04"
PRETTY_NAME="Ubuntu 24.04.4 LTS"
"#,
        )?;

        assert!(release.is_supported_ubuntu());
        assert_eq!(release.pretty_name, "Ubuntu 24.04.4 LTS");
        Ok(())
    }

    #[test]
    fn supports_ubuntu_2204_and_newer_without_an_upper_bound()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        for version in ["22.04", "22.10", "24.04", "26.04", "30.10"] {
            let release = parse_os_release(&format!(
                "ID=ubuntu\nVERSION_ID=\"{version}\"\nPRETTY_NAME=\"Ubuntu {version}\"\n"
            ))?;

            assert!(
                release.is_supported_ubuntu(),
                "{version} should be supported"
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_old_other_or_malformed_releases()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        for (id, version) in [
            ("ubuntu", "20.04"),
            ("debian", "24.04"),
            ("ubuntu", "rolling"),
            ("ubuntu", "22"),
        ] {
            let release = parse_os_release(&format!(
                "ID={id}\nVERSION_ID=\"{version}\"\nPRETTY_NAME=\"test\"\n"
            ))?;

            assert!(!release.is_supported_ubuntu(), "{id} {version} should fail");
        }
        Ok(())
    }
}
