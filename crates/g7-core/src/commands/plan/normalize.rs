use super::*;

pub(super) fn runtime_label(web_server: &str) -> &'static str {
    match web_server {
        "apache" => "Apache",
        "frankenphp" => "FrankenPHP",
        _ => "Nginx",
    }
}

pub(super) fn web_runtime_model(web_server: &str) -> &'static str {
    match web_server {
        "apache" => "Apache mpm_event/worker + proxy_fcgi + PHP-FPM",
        "frankenphp" => "Nginx edge proxy + FrankenPHP localhost app server",
        _ => "Nginx event worker + FastCGI PHP-FPM",
    }
}

pub(super) fn php_endpoint(web_server: &str, php_version: &str) -> String {
    if web_server == "frankenphp" {
        "FrankenPHP localhost app server 127.0.0.1:7080".to_string()
    } else {
        format!("/run/php/php{php_version}-fpm.sock")
    }
}

pub(super) fn database_label(database_engine: &str) -> &'static str {
    if database_engine == "mariadb" {
        "MariaDB"
    } else {
        "MySQL"
    }
}

pub(super) fn server_names(domain: &str, www_mode: &str) -> String {
    match www_mode {
        "redirect-to-www" => format!("www.{domain}"),
        "include" => format!("{domain} www.{domain}"),
        "none" => domain.to_string(),
        _ => domain.to_string(),
    }
}

pub(super) fn redirect_source(domain: &str, www_mode: &str) -> String {
    match www_mode {
        "redirect-to-root" if !domain.starts_with("www.") => format!("www.{domain} -> {domain}"),
        "redirect-to-www" if !domain.starts_with("www.") => format!("{domain} -> www.{domain}"),
        _ => "none".to_string(),
    }
}

pub(super) fn rewrite_policy(app_profile: &str) -> &'static str {
    match app_profile {
        "wordpress" => "WordPress permalink rewrite to /index.php",
        "laravel" => "Laravel public/ front controller rewrite",
        "laravel-octane" => "Nginx static assets and proxy to Laravel Octane on FrankenPHP",
        "gnuboard7-octane" => {
            "Gnuboard7 public/ front controller through Laravel Octane on FrankenPHP"
        }
        _ => "Gnuboard public/ front controller and PHP path handling",
    }
}

pub(super) fn php_version_at_least(selected: &str, minimum: &str) -> bool {
    let selected = php_version_tuple(selected);
    let minimum = php_version_tuple(minimum);

    selected >= minimum
}

pub(super) fn php_version_tuple(version: &str) -> (u16, u16) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_default();
    let minor = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_default();

    (major, minor)
}

pub(super) fn stop_conditions(
    web_server: &str,
    web_root: &str,
    local_test: bool,
) -> Vec<PlanStopCondition> {
    let other_web_server = match web_server {
        "apache" => "Nginx is running.",
        "frankenphp" => "Apache is running.",
        _ => "Apache is running.",
    };
    let selected_web_config = match web_server {
        "apache" => "Apache site configs already exist.",
        "frankenphp" => "Nginx site configs already exist.",
        _ => "Nginx site configs already exist.",
    };

    let port_stop_condition = if local_test {
        "TCP port 80 is already in use."
    } else {
        "TCP port 80 or 443 is already in use."
    };

    let mut conditions = vec![
        PlanStopCondition::new(other_web_server),
        PlanStopCondition::new(selected_web_config),
        PlanStopCondition::new(port_stop_condition),
        PlanStopCondition::new(format!(
            "{web_root} exists but is not empty or not owned by the selected site account."
        )),
        PlanStopCondition::new("/var/www/g7 legacy test root exists without installer ownership."),
        PlanStopCondition::new("G7-related paths exist without owned-files metadata."),
        PlanStopCondition::new("A previous installer state exists for another install."),
        PlanStopCondition::new("Selected SMTP outbound port cannot be reached."),
        PlanStopCondition::new("Redis is configured to bind publicly."),
        PlanStopCondition::new("Database is reachable from a non-local interface."),
        PlanStopCondition::new("SSH hardening would risk locking out the active session."),
    ];

    if !local_test {
        conditions.push(PlanStopCondition::new(
            "Domain A/AAAA records do not match this VPS public IP.",
        ));
        conditions.push(PlanStopCondition::new(
            "Requested www host does not resolve to this VPS public IP.",
        ));
        conditions.push(PlanStopCondition::new(
            "Existing Let's Encrypt certificate conflicts with installer ownership.",
        ));
    }

    conditions
}

pub(super) fn web_server_package(web_server: &str) -> &'static str {
    match web_server {
        "apache" => "apache2",
        "frankenphp" => "nginx",
        _ => "nginx",
    }
}

pub(super) fn web_server_package_description(web_server: &str) -> &'static str {
    match web_server {
        "frankenphp" => {
            "SSL, 정적 파일, 도메인 요청을 FrankenPHP 앱서버로 넘기는 Nginx edge입니다."
        }
        _ => "도메인 요청을 PHP 앱으로 전달하는 웹서버입니다.",
    }
}

pub(super) fn php_runtime_packages(web_server: &str, php_version: &str) -> String {
    if web_server == "frankenphp" {
        format!("php{php_version}-cli")
    } else {
        format!("php{php_version}-fpm php{php_version}-cli")
    }
}

pub(super) fn web_server_service(web_server: &str) -> &'static str {
    match web_server {
        "apache" => "apache2",
        "frankenphp" => "nginx",
        _ => "nginx",
    }
}

pub(super) fn certbot_web_plugin_package(web_server: &str) -> Option<&'static str> {
    match web_server {
        "apache" => Some("python3-certbot-apache"),
        "nginx" => Some("python3-certbot-nginx"),
        _ => None,
    }
}

pub(super) fn database_package(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql-server"
    } else {
        "mariadb-server"
    }
}

pub(super) fn database_service(database_engine: &str) -> &'static str {
    if database_engine == "mysql" {
        "mysql"
    } else {
        "mariadb"
    }
}

pub(super) fn web_server_available_file(web_server: &str) -> PlanFile {
    match web_server {
        "apache" => PlanFile::new("/etc/apache2/sites-available/g7.conf", "create"),
        _ => PlanFile::new("/etc/nginx/sites-available/g7.conf", "create"),
    }
}

pub(super) fn web_server_enabled_file(web_server: &str) -> PlanFile {
    match web_server {
        "apache" => PlanFile::new("/etc/apache2/sites-enabled/g7.conf", "create symlink"),
        _ => PlanFile::new("/etc/nginx/sites-enabled/g7.conf", "create symlink"),
    }
}

pub(super) fn app_config_file(
    app_profile: &crate::app_profile::AppProfile,
    web_root: &str,
) -> String {
    if app_profile.id == "wordpress" {
        format!("{web_root}/wp-config.php")
    } else {
        format!("{web_root}/.env")
    }
}

pub(super) fn smtp_port_for_plan(mail_mode: &str, port: u16) -> Option<u16> {
    if mail_mode == "none" {
        None
    } else {
        Some(port)
    }
}

pub(super) fn smtp_port_for_mode(mail_mode: &str, port: u16) -> u16 {
    if mail_mode == "local-postfix" && port == DEFAULT_SMTP_PORT {
        25
    } else {
        port
    }
}

pub(super) fn smtp_encryption_for_plan(mail_mode: &str, encryption: String) -> Option<String> {
    if mail_mode == "none" {
        None
    } else {
        Some(encryption)
    }
}

pub(super) fn normalize_php_version(version: String) -> Result<String> {
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

pub(super) fn normalize_php_source(php_version: &str, source: String) -> Result<String> {
    let source = normalize_supported_option("php-source", source, &SUPPORTED_PHP_SOURCES)?;
    let source = if source == PHP_SOURCE_AUTO {
        if php_version == UBUNTU_FPM_VERSION {
            PHP_SOURCE_UBUNTU
        } else {
            PHP_SOURCE_ONDREJ
        }
    } else {
        source.as_str()
    };

    if source == PHP_SOURCE_UBUNTU && php_version != UBUNTU_FPM_VERSION {
        return Err(Error::InvalidOption {
            field: "php-source",
            value: format!("{source}+php{php_version}"),
            supported: format!(
                "Ubuntu 24.04 기본 apt는 PHP {UBUNTU_FPM_VERSION} 기준입니다. PHP {php_version}은 php-source=ondrej로 Ondrej PHP PPA를 추가해야 합니다."
            ),
        });
    }

    Ok(source.to_string())
}

pub(super) fn normalize_site_user(site_user: String) -> Result<String> {
    let site_user = site_user.trim().to_string();

    if site_user.is_empty() {
        return Err(Error::MissingInput { field: "site-user" });
    }

    let valid = site_user
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        && !site_user.starts_with('-');

    if valid {
        Ok(site_user)
    } else {
        Err(Error::InvalidOption {
            field: "site-user",
            value: site_user,
            supported: "Linux account name using letters, digits, underscore, or dash".to_string(),
        })
    }
}

pub(super) fn validate_site_user_password(password: Option<&str>) -> Result<()> {
    let Some(password) = password else {
        return Ok(());
    };

    if password.len() < 8 {
        return Err(Error::InvalidOption {
            field: "site-password",
            value: "<redacted>".to_string(),
            supported: "at least 8 characters".to_string(),
        });
    }

    let unsupported = password
        .chars()
        .any(|ch| ch == ':' || ch == '\n' || ch == '\r' || ch.is_control());
    if unsupported {
        return Err(Error::InvalidOption {
            field: "site-password",
            value: "<redacted>".to_string(),
            supported: "no colon, newline, or control characters".to_string(),
        });
    }

    Ok(())
}

pub(super) fn validate_database_password(password: Option<&str>) -> Result<()> {
    let Some(password) = password else {
        return Ok(());
    };

    if password.len() < 8 {
        return Err(Error::InvalidOption {
            field: "database-password",
            value: "<redacted>".to_string(),
            supported: "at least 8 characters".to_string(),
        });
    }

    let unsupported = password
        .chars()
        .any(|ch| ch == '\'' || ch == '\\' || ch == '\n' || ch == '\r' || ch.is_control());
    if unsupported {
        return Err(Error::InvalidOption {
            field: "database-password",
            value: "<redacted>".to_string(),
            supported: "no single quote, backslash, newline, or control characters".to_string(),
        });
    }

    Ok(())
}

pub(super) fn normalize_database_identifier(
    field: &'static str,
    value: String,
    max_len: usize,
    supported: &str,
) -> Result<String> {
    let value = value.trim().to_string();

    if value.is_empty() {
        return Err(Error::MissingInput { field });
    }

    let valid = value.len() <= max_len
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && value
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
            .unwrap_or(false);

    if valid {
        Ok(value)
    } else {
        Err(Error::InvalidOption {
            field,
            value,
            supported: supported.to_string(),
        })
    }
}

pub(super) fn normalize_web_root_mode(
    mode: String,
    custom_web_root: &Option<String>,
) -> Result<String> {
    let mode = if custom_web_root.is_some() && mode == DEFAULT_WEB_ROOT_MODE {
        "custom".to_string()
    } else {
        mode
    };

    normalize_supported_option("web-root-mode", mode, &SUPPORTED_WEB_ROOT_MODES)
}

pub(super) fn web_root_for(
    domain: &str,
    site_user: &str,
    mode: &str,
    custom_web_root: Option<&str>,
) -> Result<String> {
    match mode {
        "public-html" => Ok(format!("/home/{site_user}/public_html")),
        "www" => Ok(format!("/home/{site_user}/www")),
        "system" => Ok(format!("/var/www/{domain}")),
        "custom" => match custom_web_root {
            Some(path) => normalize_custom_web_root(path),
            None => Err(Error::MissingInput { field: "web-root" }),
        },
        _ => Err(Error::InvalidOption {
            field: "web-root-mode",
            value: mode.to_string(),
            supported: SUPPORTED_WEB_ROOT_MODES.join(", "),
        }),
    }
}

pub(super) fn normalize_custom_web_root(path: &str) -> Result<String> {
    let path = path.trim().trim_end_matches('/').to_string();

    if path.is_empty() {
        return Err(Error::MissingInput { field: "web-root" });
    }

    if !path.starts_with('/')
        || path == "/"
        || path.contains('\n')
        || path.contains('\r')
        || path.contains('"')
        || path.split('/').any(|segment| segment == "..")
    {
        return Err(Error::InvalidOption {
            field: "web-root",
            value: path,
            supported: "absolute path without quotes, newlines, or parent traversal".to_string(),
        });
    }

    Ok(path)
}

pub(super) fn database_name_for_domain(domain: &str, app_profile: &str) -> String {
    let mut name = format!("{}_", database_prefix(app_profile));
    for ch in domain.chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch);
        } else {
            name.push('_');
        }
    }
    name.truncate(48);
    name.trim_end_matches('_').to_string()
}

pub(super) fn database_user_for_site_user(site_user: &str, app_profile: &str) -> String {
    let prefix = database_prefix(app_profile);
    let mut user = if site_user == DEFAULT_SITE_USER {
        format!("{prefix}_app")
    } else {
        format!("{prefix}_{site_user}")
    };
    user.truncate(32);
    user
}

pub(super) fn database_prefix(app_profile: &str) -> &'static str {
    match app_profile {
        "wordpress" => "wp",
        "laravel" | "laravel-octane" => "laravel",
        "gnuboard7-octane" => "g7",
        _ => "g7",
    }
}

pub(super) fn database_password_policy_label(policy: &str) -> &'static str {
    match policy {
        "user-provided-store-root-only" => {
            "사용자 입력값을 root-only 파일에 저장, 화면/로그 출력 금지"
        }
        _ => "무작위 생성 후 root-only 파일에 저장, 화면/로그 출력 금지",
    }
}

pub(super) fn normalize_supported_option(
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

pub(super) fn validate_mail_options(
    mail_mode: &str,
    smtp_host: Option<&str>,
    smtp_from: Option<&str>,
    smtp_username: Option<&str>,
    smtp_password: Option<&str>,
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

    if optional_trimmed_is_empty(smtp_username) {
        return Err(Error::MissingInput {
            field: "smtp-username",
        });
    }

    if optional_trimmed_is_empty(smtp_password) {
        return Err(Error::MissingInput {
            field: "smtp-password",
        });
    }

    if let Some(host) = smtp_host {
        validate_config_safe_value("smtp-host", host)?;
    }

    if let Some(from) = smtp_from {
        validate_config_safe_value("smtp-from", from)?;
    }

    if let Some(username) = smtp_username {
        validate_config_safe_value("smtp-username", username)?;
        if username.contains('"') || username.contains('\\') {
            return Err(Error::InvalidOption {
                field: "smtp-username",
                value: username.to_string(),
                supported: "value without double quote, backslash, newline, or control characters"
                    .to_string(),
            });
        }
    }

    if let Some(password) = smtp_password {
        validate_config_safe_value("smtp-password", password)?;
        if password.len() < 8 || password.contains('"') || password.contains('\\') {
            return Err(Error::InvalidOption {
                field: "smtp-password",
                value: "redacted".to_string(),
                supported:
                    "8+ characters without double quote, backslash, newline, or control characters"
                        .to_string(),
            });
        }
    }

    Ok(())
}

pub(super) fn optional_trimmed_is_empty(value: Option<&str>) -> bool {
    match value {
        Some(value) => value.trim().is_empty(),
        None => true,
    }
}

pub(super) fn validate_config_safe_value(field: &'static str, value: &str) -> Result<()> {
    if value.contains('"') || value.contains('\n') || value.contains('\r') {
        return Err(Error::InvalidOption {
            field,
            value: value.to_string(),
            supported: "plain value without quotes or newlines".to_string(),
        });
    }

    Ok(())
}

pub(super) fn normalize_domain(domain: String) -> Result<String> {
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
