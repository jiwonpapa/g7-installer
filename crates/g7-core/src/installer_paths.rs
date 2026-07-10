//! Canonical filesystem paths used by the installer.
//!
//! Keep installer-owned metadata paths here so reset, rollback, reporting, and
//! web APIs do not drift into slightly different hard-coded strings.

pub const CONFIG_PATH: &str = "/etc/g7-installer/config.toml";
pub const ETC_DIR: &str = "/etc/g7-installer";
pub const LIB_DIR: &str = "/var/lib/g7-installer";
pub const LOG_DIR: &str = "/var/log/g7-installer";
pub const BACKUP_DIR: &str = "/var/backups/g7-installer";
pub const LOG_PATH: &str = "/var/log/g7-installer/install.log";
pub const COMMAND_AUDIT_LOG_PATH: &str = g7_system::command::COMMAND_AUDIT_LOG_PATH;
pub const REPORT_PATH: &str = "/var/log/g7-installer/report.json";
pub const ROLLBACK_PATH: &str = "/var/lib/g7-installer/rollback.json";
pub const BACKUP_MANIFEST_PATH: &str = "/var/backups/g7-installer/manifest.json";
pub const LOCAL_HOSTS_PATH: &str = "/etc/g7-installer/local-hosts.txt";
pub const SECRETS_PATH: &str = "/etc/g7-installer/secrets.toml";
pub const SETUP_GUIDE_PATH: &str = "/var/log/g7-installer/setup-guide.md";
pub const LETSENCRYPT_LIVE_DIR: &str = "/etc/letsencrypt/live";
pub const NGINX_MAIN_CONFIG_PATH: &str = "/etc/nginx/nginx.conf";
pub const NGINX_MAIN_BACKUP_PATH: &str = "/var/backups/g7-installer/nginx.conf.before-g7";
