use super::*;

pub const DEFAULT_PHP_VERSION: &str = DEFAULT_FPM_VERSION;
pub const DEFAULT_PHP_SOURCE: &str = PHP_SOURCE_AUTO;
pub const DEFAULT_WEB_SERVER: &str = "nginx";
pub const DEFAULT_DATABASE_ENGINE: &str = "mysql";
pub const DEFAULT_SITE_USER: &str = "g7";
pub const DEFAULT_WEB_ROOT_MODE: &str = "public-html";
pub const DEFAULT_WWW_MODE: &str = "redirect-to-www";
pub const DEFAULT_REDIS_MODE: &str = "enable";
pub const DEFAULT_MAIL_MODE: &str = "none";
pub const DEFAULT_SMTP_PORT: u16 = 587;
pub const DEFAULT_SMTP_ENCRYPTION: &str = "starttls";
pub const DEFAULT_SECURITY_PROFILE: &str = "standard";
pub const DEFAULT_SSH_POLICY: &str = "audit-only";

pub(super) const SUPPORTED_WEB_SERVERS: [&str; 3] = ["nginx", "apache", "frankenphp"];
pub(super) const SUPPORTED_DATABASE_ENGINES: [&str; 2] = ["mysql", "mariadb"];
pub(super) const SUPPORTED_WEB_ROOT_MODES: [&str; 4] = ["public-html", "www", "system", "custom"];
pub(super) const SUPPORTED_WWW_MODES: [&str; 4] =
    ["redirect-to-root", "redirect-to-www", "include", "none"];
pub(super) const SUPPORTED_REDIS_MODES: [&str; 2] = ["enable", "disable"];
pub(super) const SUPPORTED_MAIL_MODES: [&str; 3] = ["none", "smtp-relay", "local-postfix"];
pub(super) const SUPPORTED_SMTP_ENCRYPTION: [&str; 3] = ["none", "starttls", "tls"];
pub(super) const SUPPORTED_SECURITY_PROFILES: [&str; 3] = ["audit-only", "standard", "hardened"];
pub(super) const SUPPORTED_SSH_POLICIES: [&str; 2] = ["audit-only", "harden"];
