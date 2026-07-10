//! Central defaults used by install planning and execution.
//!
//! Keep version, port, path, and upstream URL defaults here so release bumps do not drift.

pub(crate) const PHP_READY_FILENAME: &str = "g7inst-ready.php";
pub(crate) const GNUBOARD7_REPO_URL: &str = "https://github.com/gnuboard/g7.git";
pub(crate) const GNUBOARD7_LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/gnuboard/g7/releases/latest";
pub(crate) const LARAVEL_REPO_URL: &str = "https://github.com/laravel/laravel.git";
pub(crate) const LARAVEL_RELEASE_REF: &str = "12.x";
pub(crate) const APP_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source";
pub(crate) const GNUBOARD7_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source/gnuboard7";
pub(crate) const LARAVEL_SOURCE_DIR: &str = "/var/lib/g7-installer/app-source/laravel";
pub(crate) const WORDPRESS_DOWNLOAD_URL: &str = "https://wordpress.org/latest.zip";
pub(crate) const WORDPRESS_ARCHIVE_PATH: &str = "/var/lib/g7-installer/app-source/wordpress.zip";
pub(crate) const WORDPRESS_EXTRACT_DIR: &str = "/var/lib/g7-installer/app-source/wordpress-extract";
pub(crate) const WORDPRESS_SOURCE_DIR: &str =
    "/var/lib/g7-installer/app-source/wordpress-extract/wordpress";
pub(crate) const CERTBOT_HTTP01_CHALLENGE_DIR: &str = ".well-known/acme-challenge";
pub(crate) const CERTBOT_HTTP01_SMOKE_FILENAME: &str = "g7inst-certbot-http01-smoke.txt";
pub(crate) const CERTBOT_HTTP01_SMOKE_CONTENT: &str = "g7-installer-certbot-http01-ok\n";
pub(crate) const SWAP_FILE_PATH: &str = "/swapfile";
pub(crate) const SWAP_UNIT_PATH: &str = "/etc/systemd/system/swapfile.swap";
pub(crate) const SWAP_SYSCTL_PATH: &str = "/etc/sysctl.d/99-g7-installer-swap.conf";
pub(crate) const FRANKENPHP_VERSION: &str = "v1.12.4";
pub(crate) const FRANKENPHP_DIR: &str = "/opt/g7-frankenphp";
pub(crate) const FRANKENPHP_BIN_PATH: &str = "/opt/g7-frankenphp/frankenphp";
pub(crate) const FRANKENPHP_SERVICE_NAME: &str = "g7-frankenphp";
pub(crate) const FRANKENPHP_SERVICE_PATH: &str = "/etc/systemd/system/g7-frankenphp.service";
pub(crate) const FRANKENPHP_HOST: &str = "127.0.0.1";
pub(crate) const FRANKENPHP_PORT: &str = "7080";
pub(crate) const FRANKENPHP_LISTEN: &str = "127.0.0.1:7080";
pub(crate) const GNUBOARD7_REQUIRED_FILES: &[&str] = &[
    ".env.example",
    "artisan",
    "composer.json",
    "public/index.php",
    "public/build/core/template-engine.min.js",
];
pub(crate) const LARAVEL_REQUIRED_FILES: &[&str] =
    &["artisan", "composer.json", "public/index.php"];
pub(crate) const WORDPRESS_REQUIRED_FILES: &[&str] = &["wp-settings.php", "wp-admin/install.php"];
pub(crate) const WORDPRESS_REQUIRED_DIRS: &[&str] = &["wp-content"];
