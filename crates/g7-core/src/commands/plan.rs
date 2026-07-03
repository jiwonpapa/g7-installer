use crate::{Error, Result};
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub domain: String,
    pub mode: &'static str,
    pub fresh_server_only: bool,
    pub changes_made: bool,
    pub preflight_gates: Vec<PlanGate>,
    pub packages: Vec<PlanPackage>,
    pub files: Vec<PlanFile>,
    pub services: Vec<PlanService>,
    pub ports: Vec<PlanPort>,
    pub stop_conditions: Vec<PlanStopCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGate {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPackage {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    pub path: &'static str,
    pub action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanService {
    pub name: &'static str,
    pub action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPort {
    pub port: u16,
    pub protocol: &'static str,
    pub purpose: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStopCondition {
    pub reason: &'static str,
}

pub fn build(domain: String) -> Result<InstallPlan> {
    let domain = normalize_domain(domain)?;

    Ok(InstallPlan {
        domain,
        mode: "dry-run",
        fresh_server_only: true,
        changes_made: false,
        preflight_gates: vec![
            PlanGate {
                name: "os",
                description: "Require Ubuntu 24.04 LTS.",
            },
            PlanGate {
                name: "privilege",
                description: "Install requires root or sudo.",
            },
            PlanGate {
                name: "fresh-server",
                description: "Abort if existing web services or unowned G7 paths are detected.",
            },
            PlanGate {
                name: "network",
                description: "Require ports 80 and 443 before HTTP/HTTPS setup.",
            },
        ],
        packages: vec![
            PlanPackage {
                name: "nginx",
                description: "Web server and reverse proxy.",
            },
            PlanPackage {
                name: "php8.3-fpm",
                description: "PHP runtime for G7.",
            },
            PlanPackage {
                name: "php8.3 extensions",
                description: "Common PHP extensions required by G7.",
            },
            PlanPackage {
                name: "mariadb-server",
                description: "Default MVP database server.",
            },
            PlanPackage {
                name: "certbot",
                description: "Let's Encrypt certificate issuance.",
            },
            PlanPackage {
                name: "python3-certbot-nginx",
                description: "Certbot Nginx integration.",
            },
            PlanPackage {
                name: "curl unzip ca-certificates",
                description: "Release download and extraction utilities.",
            },
        ],
        files: vec![
            PlanFile {
                path: "/etc/g7-installer/config.toml",
                action: "create",
            },
            PlanFile {
                path: STATE_PATH,
                action: "create/update",
            },
            PlanFile {
                path: OWNED_FILES_PATH,
                action: "create/update",
            },
            PlanFile {
                path: "/var/log/g7-installer/install.log",
                action: "create/append",
            },
            PlanFile {
                path: "/var/www/g7",
                action: "create",
            },
            PlanFile {
                path: "/etc/nginx/sites-available/g7.conf",
                action: "create",
            },
            PlanFile {
                path: "/etc/nginx/sites-enabled/g7.conf",
                action: "create symlink",
            },
            PlanFile {
                path: "/etc/systemd/system/g7-queue.service",
                action: "create when worker is enabled",
            },
            PlanFile {
                path: "/etc/systemd/system/g7-reverb.service",
                action: "create when realtime server is enabled",
            },
        ],
        services: vec![
            PlanService {
                name: "nginx",
                action: "enable and reload",
            },
            PlanService {
                name: "php8.3-fpm",
                action: "enable and restart",
            },
            PlanService {
                name: "mariadb",
                action: "enable and start",
            },
            PlanService {
                name: "g7-queue.service",
                action: "optional enable and start",
            },
            PlanService {
                name: "g7-reverb.service",
                action: "optional enable and start",
            },
        ],
        ports: vec![
            PlanPort {
                port: 80,
                protocol: "tcp",
                purpose: "HTTP and Let's Encrypt challenge.",
            },
            PlanPort {
                port: 443,
                protocol: "tcp",
                purpose: "HTTPS traffic.",
            },
        ],
        stop_conditions: vec![
            PlanStopCondition {
                reason: "Apache is running.",
            },
            PlanStopCondition {
                reason: "Nginx site configs already exist.",
            },
            PlanStopCondition {
                reason: "TCP port 80 or 443 is already in use.",
            },
            PlanStopCondition {
                reason: "/var/www/g7 already exists.",
            },
            PlanStopCondition {
                reason: "G7-related paths exist without owned-files metadata.",
            },
            PlanStopCondition {
                reason: "A previous installer state exists for another install.",
            },
        ],
    })
}

fn normalize_domain(domain: String) -> Result<String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();

    if domain.is_empty() {
        return Err(Error::MissingInput { field: "domain" });
    }

    if domain.contains('/') || domain.contains(':') || domain.chars().any(char::is_whitespace) {
        return Err(Error::InvalidDomain { domain });
    }

    if domain.len() > 253 || !domain.contains('.') {
        return Err(Error::InvalidDomain { domain });
    }

    if !domain
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.')
    {
        return Err(Error::InvalidDomain { domain });
    }

    if domain.split('.').any(|label| {
        label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
    }) {
        return Err(Error::InvalidDomain { domain });
    }

    Ok(domain)
}

#[cfg(test)]
mod tests {
    use super::build;
    use crate::Error;

    #[test]
    fn plan_normalizes_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build(" Example.COM. ".to_string())?;

        assert_eq!(plan.domain, "example.com");
        assert_eq!(plan.mode, "dry-run");
        assert!(!plan.changes_made);
        Ok(())
    }

    #[test]
    fn plan_describes_install_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build("example.com".to_string())?;

        assert!(plan.fresh_server_only);
        assert!(plan.packages.iter().any(|package| package.name == "nginx"));
        assert!(plan.files.iter().any(|file| file.path == "/var/www/g7"));
        assert!(plan.services.iter().any(|service| service.name == "nginx"));
        assert!(plan.ports.iter().any(|port| port.port == 443));
        assert!(
            plan.stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("Apache"))
        );
        Ok(())
    }

    #[test]
    fn plan_rejects_empty_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let err = match build(" ".to_string()) {
            Ok(_) => return Err(std::io::Error::other("empty domain should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::MissingInput { field: "domain" }));
        Ok(())
    }

    #[test]
    fn plan_rejects_url_like_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let err = match build("https://example.com".to_string()) {
            Ok(_) => return Err(std::io::Error::other("URL should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::InvalidDomain { .. }));
        Ok(())
    }

    #[test]
    fn plan_rejects_invalid_domain_labels() -> std::result::Result<(), Box<dyn std::error::Error>> {
        for domain in ["example", "-example.com", "example-.com", "exa_mple.com"] {
            let err = match build(domain.to_string()) {
                Ok(_) => {
                    return Err(std::io::Error::other("invalid domain should fail").into());
                }
                Err(err) => err,
            };

            assert!(matches!(err, Error::InvalidDomain { .. }));
        }
        Ok(())
    }
}
