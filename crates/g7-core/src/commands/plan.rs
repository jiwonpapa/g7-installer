use crate::{Error, Result};
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use g7_system::php::{DEFAULT_FPM_VERSION, SUPPORTED_FPM_VERSIONS};

pub const DEFAULT_PHP_VERSION: &str = DEFAULT_FPM_VERSION;
pub const DEFAULT_WEB_SERVER: &str = "nginx";
pub const DEFAULT_DATABASE_ENGINE: &str = "mariadb";
pub const DEFAULT_WWW_MODE: &str = "redirect-to-root";
pub const DEFAULT_REDIS_MODE: &str = "enable";
pub const DEFAULT_MAIL_MODE: &str = "none";
pub const DEFAULT_SMTP_PORT: u16 = 587;
pub const DEFAULT_SMTP_ENCRYPTION: &str = "starttls";

const SUPPORTED_WEB_SERVERS: [&str; 2] = ["nginx", "apache"];
const SUPPORTED_DATABASE_ENGINES: [&str; 2] = ["mariadb", "mysql"];
const SUPPORTED_WWW_MODES: [&str; 4] = ["redirect-to-root", "redirect-to-www", "include", "none"];
const SUPPORTED_REDIS_MODES: [&str; 2] = ["enable", "disable"];
const SUPPORTED_MAIL_MODES: [&str; 3] = ["none", "smtp-relay", "local-postfix"];
const SUPPORTED_SMTP_ENCRYPTION: [&str; 3] = ["none", "starttls", "tls"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub domain: String,
    pub deployment_mode: String,
    pub web_server: String,
    pub php_version: String,
    pub database_engine: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_from: Option<String>,
    pub smtp_encryption: Option<String>,
    pub rollback_enabled: bool,
    pub preserve_config: bool,
    pub dns_check_required: bool,
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
    pub name: String,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    pub path: &'static str,
    pub action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanService {
    pub name: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOptions {
    pub local_test: bool,
    pub web_server: String,
    pub php_version: String,
    pub database_engine: String,
    pub www_mode: String,
    pub redis_mode: String,
    pub mail_mode: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_from: Option<String>,
    pub smtp_encryption: String,
    pub rollback: bool,
    pub preserve_config: bool,
    pub dns_check: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            local_test: false,
            web_server: DEFAULT_WEB_SERVER.to_string(),
            php_version: DEFAULT_PHP_VERSION.to_string(),
            database_engine: DEFAULT_DATABASE_ENGINE.to_string(),
            www_mode: DEFAULT_WWW_MODE.to_string(),
            redis_mode: DEFAULT_REDIS_MODE.to_string(),
            mail_mode: DEFAULT_MAIL_MODE.to_string(),
            smtp_host: None,
            smtp_port: DEFAULT_SMTP_PORT,
            smtp_from: None,
            smtp_encryption: DEFAULT_SMTP_ENCRYPTION.to_string(),
            rollback: true,
            preserve_config: true,
            dns_check: true,
        }
    }
}

pub fn build(domain: String) -> Result<InstallPlan> {
    build_with_options(domain, PlanOptions::default())
}

pub fn build_with_options(domain: String, options: PlanOptions) -> Result<InstallPlan> {
    let domain = normalize_domain(domain)?;
    let web_server =
        normalize_supported_option("web-server", options.web_server, &SUPPORTED_WEB_SERVERS)?;
    let php_version = normalize_php_version(options.php_version)?;
    let database_engine = normalize_supported_option(
        "database",
        options.database_engine,
        &SUPPORTED_DATABASE_ENGINES,
    )?;
    let www_mode = normalize_supported_option("www-mode", options.www_mode, &SUPPORTED_WWW_MODES)?;
    let redis_mode =
        normalize_supported_option("redis", options.redis_mode, &SUPPORTED_REDIS_MODES)?;
    let mail_mode =
        normalize_supported_option("mail-mode", options.mail_mode, &SUPPORTED_MAIL_MODES)?;
    let smtp_encryption = normalize_supported_option(
        "smtp-encryption",
        options.smtp_encryption,
        &SUPPORTED_SMTP_ENCRYPTION,
    )?;
    validate_mail_options(
        &mail_mode,
        options.smtp_host.as_deref(),
        options.smtp_from.as_deref(),
    )?;
    let smtp_port = smtp_port_for_mode(&mail_mode, options.smtp_port);

    let dns_check_required = options.dns_check && !options.local_test;
    let deployment_mode = if options.local_test {
        "local-test"
    } else {
        "public"
    }
    .to_string();
    let packages = packages(
        &web_server,
        &php_version,
        &database_engine,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let files = files(&web_server, &redis_mode, &mail_mode, options.local_test);
    let services = services(
        &web_server,
        &php_version,
        &database_engine,
        &redis_mode,
        &mail_mode,
        options.local_test,
    );
    let ports = ports(&redis_mode, &mail_mode, smtp_port, options.local_test);
    let stop_conditions = stop_conditions(&web_server, options.local_test);

    Ok(InstallPlan {
        domain,
        deployment_mode,
        web_server,
        php_version: php_version.clone(),
        database_engine,
        www_mode,
        redis_mode,
        mail_mode: mail_mode.clone(),
        smtp_host: options.smtp_host,
        smtp_port: smtp_port_for_plan(&mail_mode, smtp_port),
        smtp_from: options.smtp_from,
        smtp_encryption: smtp_encryption_for_plan(&mail_mode, smtp_encryption),
        rollback_enabled: options.rollback,
        preserve_config: options.preserve_config,
        dns_check_required,
        mode: "dry-run",
        fresh_server_only: true,
        changes_made: false,
        preflight_gates: preflight_gates(options.local_test),
        packages,
        files,
        services,
        ports,
        stop_conditions,
    })
}

fn preflight_gates(local_test: bool) -> Vec<PlanGate> {
    let mut gates = vec![
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
            description: if local_test {
                "Require port 80 for local HTTP setup."
            } else {
                "Require ports 80 and 443 before HTTP/HTTPS setup."
            },
        },
    ];

    if local_test {
        gates.push(PlanGate {
            name: "local-hostname",
            description: "Use a local test hostname without public DNS or Let's Encrypt.",
        });
    } else {
        gates.push(PlanGate {
            name: "dns-public-ip",
            description: "Verify domain A/AAAA records match this VPS public IP before Certbot.",
        });
    }

    gates.extend([
        PlanGate {
            name: "www-canonical",
            description: "Apply requested root/www canonical host policy.",
        },
        PlanGate {
            name: "mail-outbound",
            description: "Check selected SMTP outbound port before writing mail settings.",
        },
        PlanGate {
            name: "rollback",
            description: "Track created installer-owned files for rollback on failure.",
        },
        PlanGate {
            name: "config-preserve",
            description: "Preserve existing configuration instead of overwriting unowned files.",
        },
    ]);

    if !local_test {
        gates.push(PlanGate {
            name: "certbot-renewal",
            description: "Enable Let's Encrypt renewal through certbot.timer.",
        });
    }

    gates
}

fn packages(
    web_server: &str,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanPackage> {
    let mut packages = vec![
        PlanPackage {
            name: web_server_package(web_server).to_string(),
            description: "Web server and reverse proxy.",
        },
        PlanPackage {
            name: format!("php{php_version}-fpm"),
            description: "PHP runtime for G7.",
        },
        PlanPackage {
            name: format!("php{php_version}-mysql php{php_version}-mbstring php{php_version}-xml"),
            description: "Core PHP extensions for database, strings, and XML.",
        },
        PlanPackage {
            name: format!("php{php_version}-curl php{php_version}-gd php{php_version}-zip"),
            description: "PHP extensions for HTTP, images, and archives.",
        },
        PlanPackage {
            name: format!("php{php_version}-intl php{php_version}-bcmath php{php_version}-opcache"),
            description: "PHP extensions for locale, decimal math, and performance.",
        },
        PlanPackage {
            name: format!("php{php_version}-imagick"),
            description: "Image processing extension for richer G7 media support.",
        },
        PlanPackage {
            name: database_package(database_engine).to_string(),
            description: "Selected SQL database server.",
        },
        PlanPackage {
            name: "curl unzip ca-certificates".to_string(),
            description: "Release download and extraction utilities.",
        },
    ];

    if !local_test {
        packages.push(PlanPackage {
            name: "certbot".to_string(),
            description: "Let's Encrypt certificate issuance.",
        });
        packages.push(PlanPackage {
            name: certbot_web_plugin_package(web_server).to_string(),
            description: "Certbot web server integration.",
        });
    }

    if redis_mode == "enable" {
        packages.push(PlanPackage {
            name: "redis-server".to_string(),
            description: "Local Redis cache/session/queue backend.",
        });
        packages.push(PlanPackage {
            name: format!("php{php_version}-redis"),
            description: "PHP Redis extension.",
        });
    }

    if mail_mode == "local-postfix" {
        packages.push(PlanPackage {
            name: "postfix mailutils".to_string(),
            description: "Optional local outbound mail transport.",
        });
    }

    packages
}

fn files(web_server: &str, redis_mode: &str, mail_mode: &str, local_test: bool) -> Vec<PlanFile> {
    let mut files = vec![
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
            path: "/var/lib/g7-installer/rollback.json",
            action: "create/update",
        },
        PlanFile {
            path: "/var/backups/g7-installer",
            action: "create for preserved config snapshots",
        },
        PlanFile {
            path: "/var/log/g7-installer/install.log",
            action: "create/append",
        },
        PlanFile {
            path: "/var/log/g7-installer/report.json",
            action: "create/update problem report",
        },
        PlanFile {
            path: "/var/www/g7",
            action: "create",
        },
        PlanFile {
            path: "/var/www/g7/.env",
            action: "create with DB/cache/mail settings",
        },
        web_server_available_file(web_server),
        web_server_enabled_file(web_server),
        PlanFile {
            path: "/etc/systemd/system/g7-queue.service",
            action: "create when worker is enabled",
        },
        PlanFile {
            path: "/etc/systemd/system/g7-reverb.service",
            action: "create when realtime server is enabled",
        },
    ];

    if redis_mode == "enable" {
        files.push(PlanFile {
            path: "/etc/g7-installer/redis.conf",
            action: "create Redis hardening overlay",
        });
    }

    if local_test {
        files.push(PlanFile {
            path: "/etc/g7-installer/local-hosts.txt",
            action: "write local hosts entry suggestion",
        });
    }

    if mail_mode != "none" {
        files.push(PlanFile {
            path: "/etc/g7-installer/mail.toml",
            action: "create SMTP delivery settings without secrets",
        });
    }

    files
}

fn services(
    web_server: &str,
    php_version: &str,
    database_engine: &str,
    redis_mode: &str,
    mail_mode: &str,
    local_test: bool,
) -> Vec<PlanService> {
    let mut services = vec![
        PlanService {
            name: web_server_service(web_server).to_string(),
            action: "enable and reload",
        },
        PlanService {
            name: format!("php{php_version}-fpm"),
            action: "enable and restart",
        },
        PlanService {
            name: database_service(database_engine).to_string(),
            action: "enable and start",
        },
        PlanService {
            name: "g7-queue.service".to_string(),
            action: "optional enable and start",
        },
        PlanService {
            name: "g7-reverb.service".to_string(),
            action: "optional enable and start",
        },
    ];

    if !local_test {
        services.push(PlanService {
            name: "certbot.timer".to_string(),
            action: "enable and verify renewal timer",
        });
    }

    if redis_mode == "enable" {
        services.push(PlanService {
            name: "redis-server".to_string(),
            action: "bind to 127.0.0.1, cap memory, enable and restart",
        });
    }

    if mail_mode == "local-postfix" {
        services.push(PlanService {
            name: "postfix".to_string(),
            action: "configure outbound-only mail transport",
        });
    }

    services
}

fn ports(redis_mode: &str, mail_mode: &str, smtp_port: u16, local_test: bool) -> Vec<PlanPort> {
    let mut ports = vec![PlanPort {
        port: 80,
        protocol: "tcp",
        purpose: if local_test {
            "Inbound local HTTP traffic."
        } else {
            "Inbound HTTP and Let's Encrypt challenge."
        },
    }];

    if !local_test {
        ports.push(PlanPort {
            port: 443,
            protocol: "tcp",
            purpose: "Inbound HTTPS traffic.",
        });
    }

    if redis_mode == "enable" {
        ports.push(PlanPort {
            port: 6379,
            protocol: "tcp",
            purpose: "Localhost-only Redis. Must not be open to the public internet.",
        });
    }

    if mail_mode == "smtp-relay" || mail_mode == "local-postfix" {
        ports.push(PlanPort {
            port: smtp_port,
            protocol: "tcp",
            purpose: "Outbound SMTP delivery check.",
        });
    }

    ports
}

fn stop_conditions(web_server: &str, local_test: bool) -> Vec<PlanStopCondition> {
    let other_web_server = if web_server == "nginx" {
        "Apache is running."
    } else {
        "Nginx is running."
    };
    let selected_web_config = if web_server == "nginx" {
        "Nginx site configs already exist."
    } else {
        "Apache site configs already exist."
    };

    let port_stop_condition = if local_test {
        "TCP port 80 is already in use."
    } else {
        "TCP port 80 or 443 is already in use."
    };

    let mut conditions = vec![
        PlanStopCondition {
            reason: other_web_server,
        },
        PlanStopCondition {
            reason: selected_web_config,
        },
        PlanStopCondition {
            reason: port_stop_condition,
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
        PlanStopCondition {
            reason: "Selected SMTP outbound port cannot be reached.",
        },
        PlanStopCondition {
            reason: "Redis is configured to bind publicly.",
        },
    ];

    if !local_test {
        conditions.push(PlanStopCondition {
            reason: "Domain A/AAAA records do not match this VPS public IP.",
        });
        conditions.push(PlanStopCondition {
            reason: "Requested www host does not resolve to this VPS public IP.",
        });
        conditions.push(PlanStopCondition {
            reason: "Existing Let's Encrypt certificate conflicts with installer ownership.",
        });
    }

    conditions
}

fn web_server_package(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "apache2"
    } else {
        "nginx"
    }
}

fn web_server_service(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "apache2"
    } else {
        "nginx"
    }
}

fn certbot_web_plugin_package(web_server: &str) -> &'static str {
    if web_server == "apache" {
        "python3-certbot-apache"
    } else {
        "python3-certbot-nginx"
    }
}

fn database_package(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql-server"
    } else {
        "mariadb-server"
    }
}

fn database_service(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql"
    } else {
        "mariadb"
    }
}

fn web_server_available_file(web_server: &str) -> PlanFile {
    if web_server == "apache" {
        PlanFile {
            path: "/etc/apache2/sites-available/g7.conf",
            action: "create",
        }
    } else {
        PlanFile {
            path: "/etc/nginx/sites-available/g7.conf",
            action: "create",
        }
    }
}

fn web_server_enabled_file(web_server: &str) -> PlanFile {
    if web_server == "apache" {
        PlanFile {
            path: "/etc/apache2/sites-enabled/g7.conf",
            action: "create symlink",
        }
    } else {
        PlanFile {
            path: "/etc/nginx/sites-enabled/g7.conf",
            action: "create symlink",
        }
    }
}

fn smtp_port_for_plan(mail_mode: &str, port: u16) -> Option<u16> {
    if mail_mode == "none" {
        None
    } else {
        Some(port)
    }
}

fn smtp_port_for_mode(mail_mode: &str, port: u16) -> u16 {
    if mail_mode == "local-postfix" && port == DEFAULT_SMTP_PORT {
        25
    } else {
        port
    }
}

fn smtp_encryption_for_plan(mail_mode: &str, encryption: String) -> Option<String> {
    if mail_mode == "none" {
        None
    } else {
        Some(encryption)
    }
}

fn normalize_php_version(version: String) -> Result<String> {
    let version = version.trim().to_string();

    if SUPPORTED_FPM_VERSIONS.contains(&version.as_str()) {
        Ok(version)
    } else {
        Err(Error::InvalidPhpVersion {
            version,
            supported: SUPPORTED_FPM_VERSIONS.join(", "),
        })
    }
}

fn normalize_supported_option(
    field: &'static str,
    value: String,
    supported: &[&str],
) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();

    if supported.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(Error::InvalidOption {
            field,
            value,
            supported: supported.join(", "),
        })
    }
}

fn validate_mail_options(
    mail_mode: &str,
    smtp_host: Option<&str>,
    smtp_from: Option<&str>,
) -> Result<()> {
    if mail_mode != "smtp-relay" {
        return Ok(());
    }

    if optional_trimmed_is_empty(smtp_host) {
        return Err(Error::MissingInput { field: "smtp-host" });
    }

    if optional_trimmed_is_empty(smtp_from) {
        return Err(Error::MissingInput { field: "smtp-from" });
    }

    if let Some(host) = smtp_host {
        validate_config_safe_value("smtp-host", host)?;
    }

    if let Some(from) = smtp_from {
        validate_config_safe_value("smtp-from", from)?;
    }

    Ok(())
}

fn optional_trimmed_is_empty(value: Option<&str>) -> bool {
    match value {
        Some(value) => value.trim().is_empty(),
        None => true,
    }
}

fn validate_config_safe_value(field: &'static str, value: &str) -> Result<()> {
    if value.contains('"') || value.contains('\n') || value.contains('\r') {
        return Err(Error::InvalidOption {
            field,
            value: value.to_string(),
            supported: "plain value without quotes or newlines".to_string(),
        });
    }

    Ok(())
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
    use super::{PlanOptions, build, build_with_options};
    use crate::Error;

    #[test]
    fn plan_normalizes_domain() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build(" Example.COM. ".to_string())?;

        assert_eq!(plan.domain, "example.com");
        assert_eq!(plan.deployment_mode, "public");
        assert_eq!(plan.web_server, "nginx");
        assert_eq!(plan.php_version, "8.5");
        assert_eq!(plan.database_engine, "mariadb");
        assert_eq!(plan.www_mode, "redirect-to-root");
        assert_eq!(plan.redis_mode, "enable");
        assert_eq!(plan.mail_mode, "none");
        assert_eq!(plan.mode, "dry-run");
        assert!(!plan.changes_made);
        Ok(())
    }

    #[test]
    fn plan_describes_install_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build("example.com".to_string())?;

        assert!(plan.fresh_server_only);
        assert!(plan.packages.iter().any(|package| package.name == "nginx"));
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "redis-server")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "certbot.timer")
        );
        assert!(plan.files.iter().any(|file| file.path == "/var/www/g7"));
        assert!(plan.services.iter().any(|service| service.name == "nginx"));
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "mariadb")
        );
        assert!(plan.ports.iter().any(|port| port.port == 443));
        assert!(plan.ports.iter().any(|port| port.port == 6379));
        assert!(
            plan.stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("public IP"))
        );
        Ok(())
    }

    #[test]
    fn plan_supports_local_test_domain_without_dns_or_certbot()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            local_test: true,
            dns_check: true,
            www_mode: "none".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("g7-test.local".to_string(), options)?;

        assert_eq!(plan.deployment_mode, "local-test");
        assert!(!plan.dns_check_required);
        assert!(
            !plan
                .packages
                .iter()
                .any(|package| package.name == "certbot")
        );
        assert!(
            !plan
                .services
                .iter()
                .any(|service| service.name == "certbot.timer")
        );
        assert!(!plan.ports.iter().any(|port| port.port == 443));
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/etc/g7-installer/local-hosts.txt")
        );
        assert!(
            !plan
                .stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("public IP"))
        );
        Ok(())
    }

    #[test]
    fn plan_supports_apache_and_mysql_choices()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            web_server: "apache".to_string(),
            database_engine: "mysql".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.web_server, "apache");
        assert_eq!(plan.database_engine, "mysql");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "apache2")
        );
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "mysql-server")
        );
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/etc/apache2/sites-available/g7.conf")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "apache2")
        );
        assert!(plan.services.iter().any(|service| service.name == "mysql"));
        Ok(())
    }

    #[test]
    fn plan_allows_php_83_compat_option() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            php_version: "8.3".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.php_version, "8.3");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "php8.3-fpm")
        );
        Ok(())
    }

    #[test]
    fn plan_rejects_unsupported_php_version() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let options = PlanOptions {
            php_version: "8.4".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("unsupported PHP should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::InvalidPhpVersion { .. }));
        Ok(())
    }

    #[test]
    fn plan_requires_smtp_relay_details() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("smtp host should be required").into()),
            Err(err) => err,
        };

        assert!(matches!(err, Error::MissingInput { field: "smtp-host" }));
        Ok(())
    }

    #[test]
    fn plan_accepts_smtp_relay_details() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_from: Some("no-reply@example.com".to_string()),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.mail_mode, "smtp-relay");
        assert_eq!(plan.smtp_port, Some(587));
        assert!(plan.ports.iter().any(|port| port.port == 587));
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
