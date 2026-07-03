use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsRelease {
    pub id: String,
    pub version_id: String,
    pub pretty_name: String,
}

impl OsRelease {
    pub fn is_supported_ubuntu(&self) -> bool {
        self.id == "ubuntu" && self.version_id == "24.04"
    }
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
}
