//! Application profiles for the installer plan.
//!
//! This module is the canonical source for app-specific server requirements.
//! UI labels, CLI flags, reports, and future mutating install phases must read
//! these profiles instead of duplicating WordPress, G7, or Laravel assumptions.

use crate::{Error, Result};

pub const DEFAULT_APP_PROFILE: &str = "gnuboard7";
pub const SUPPORTED_APP_PROFILES: [&str; 4] =
    ["gnuboard7", "wordpress", "laravel", "laravel-octane"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub min_php: &'static str,
    pub database_requirement: &'static str,
    pub document_root: DocumentRootStrategy,
    pub php_extensions: &'static [&'static str],
    pub system_packages: &'static [&'static str],
    pub services: &'static [&'static str],
    pub writable_paths: &'static [&'static str],
    pub post_install_steps: &'static [&'static str],
    pub health_checks: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRootStrategy {
    SiteRoot,
    PublicSubdir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRequirement {
    pub name: String,
    pub status: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFollowupStep {
    pub name: &'static str,
    pub description: &'static str,
}

const WORDPRESS_EXTENSIONS: &[&str] = &[
    "mysqli", "mysqlnd", "curl", "dom", "exif", "fileinfo", "hash", "intl", "mbstring", "xml",
    "zip", "gd", "imagick",
];

const GNUBOARD7_EXTENSIONS: &[&str] = &[
    "bcmath",
    "ctype",
    "curl",
    "dom",
    "exif",
    "fileinfo",
    "filter",
    "gd",
    "hash",
    "imagick",
    "intl",
    "json",
    "ldap",
    "libxml",
    "maxminddb",
    "mbstring",
    "memcached",
    "openssl",
    "pcntl",
    "pcre",
    "pdo",
    "pdo_mysql",
    "phar",
    "posix",
    "redis",
    "session",
    "simplexml",
    "sodium",
    "tokenizer",
    "xml",
    "xmlwriter",
    "zip",
    "zlib",
];

const LARAVEL_EXTENSIONS: &[&str] = &[
    "bcmath",
    "ctype",
    "curl",
    "dom",
    "fileinfo",
    "filter",
    "hash",
    "mbstring",
    "openssl",
    "pdo",
    "pdo_mysql",
    "session",
    "tokenizer",
    "xml",
];

const LARAVEL_OCTANE_EXTENSIONS: &[&str] = &[
    "bcmath",
    "ctype",
    "curl",
    "dom",
    "fileinfo",
    "filter",
    "hash",
    "mbstring",
    "openssl",
    "pcntl",
    "pdo",
    "pdo_mysql",
    "session",
    "tokenizer",
    "xml",
];

const WORDPRESS_PROFILE: AppProfile = AppProfile {
    id: "wordpress",
    label: "WordPress",
    summary: "PHP publishing CMS with rewrite rules, wp-config.php, uploads, and HTTPS.",
    min_php: "8.3",
    database_requirement: "MySQL 8.0+ or MariaDB 10.6+",
    document_root: DocumentRootStrategy::SiteRoot,
    php_extensions: WORDPRESS_EXTENSIONS,
    system_packages: &["wp-cli"],
    services: &[],
    writable_paths: &["wp-content/uploads"],
    post_install_steps: &[
        "download WordPress release",
        "create database and wp-config.php",
        "generate salts",
        "apply rewrite rules",
    ],
    health_checks: &["GET /", "GET /wp-admin/install.php"],
};

const GNUBOARD7_PROFILE: AppProfile = AppProfile {
    id: "gnuboard7",
    label: "Gnuboard 7",
    summary: "Laravel-based G7 application with Composer, Node build, Redis, queue, scheduler, and Reverb.",
    min_php: "8.2",
    database_requirement: "MySQL 8.0+ or MariaDB 10.3+",
    document_root: DocumentRootStrategy::PublicSubdir,
    php_extensions: GNUBOARD7_EXTENSIONS,
    system_packages: &["composer", "nodejs", "npm"],
    services: &[
        "g7-queue.service",
        "g7-scheduler.service",
        "g7-scheduler.timer",
        "g7-reverb.service",
    ],
    writable_paths: &["storage", "bootstrap/cache"],
    post_install_steps: &[
        "download G7 release",
        "run composer install",
        "build frontend assets",
        "create .env and APP_KEY",
        "link storage",
        "open browser installer at /install",
        "run migrations and optimization after browser install",
        "enable queue, scheduler, and Reverb after browser install",
    ],
    health_checks: &[
        "GET /",
        "php artisan about",
        "queue worker active",
        "scheduler active",
    ],
};

const LARAVEL_PROFILE: AppProfile = AppProfile {
    id: "laravel",
    label: "Laravel",
    summary: "Generic Laravel app with Composer, frontend build, public document root, and artisan optimization.",
    min_php: "8.2",
    database_requirement: "MySQL or MariaDB supported by the selected Laravel release",
    document_root: DocumentRootStrategy::PublicSubdir,
    php_extensions: LARAVEL_EXTENSIONS,
    system_packages: &["composer", "nodejs", "npm"],
    services: &[
        "laravel-queue.service",
        "laravel-scheduler.service",
        "laravel-scheduler.timer",
    ],
    writable_paths: &["storage", "bootstrap/cache"],
    post_install_steps: &[
        "fetch application source",
        "run composer install --no-dev",
        "install/build frontend assets when present",
        "create .env and APP_KEY",
        "run migrations when requested",
        "cache Laravel config/routes/views",
    ],
    health_checks: &[
        "GET /",
        "php artisan about",
        "queue worker active when enabled",
    ],
};

const LARAVEL_OCTANE_PROFILE: AppProfile = AppProfile {
    id: "laravel-octane",
    label: "Laravel Octane",
    summary: "Laravel app served by Octane workers on FrankenPHP behind an Nginx edge proxy.",
    min_php: "8.2",
    database_requirement: "MySQL or MariaDB supported by the selected Laravel release",
    document_root: DocumentRootStrategy::PublicSubdir,
    php_extensions: LARAVEL_OCTANE_EXTENSIONS,
    system_packages: &["composer", "nodejs", "npm"],
    services: &[
        "laravel-queue.service",
        "laravel-scheduler.service",
        "laravel-scheduler.timer",
    ],
    writable_paths: &["storage", "bootstrap/cache"],
    post_install_steps: &[
        "fetch Laravel application source",
        "run composer install --no-dev",
        "install Laravel Octane with FrankenPHP",
        "build frontend assets",
        "create .env and APP_KEY",
        "run migrations and optimization",
        "serve app through Octane on 127.0.0.1:7080",
    ],
    health_checks: &[
        "GET /",
        "php artisan about",
        "g7-frankenphp Octane service active",
        "queue worker active when enabled",
    ],
};

pub fn resolve_app_profile(value: &str) -> Result<&'static AppProfile> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "wordpress" => Ok(&WORDPRESS_PROFILE),
        "laravel" => Ok(&LARAVEL_PROFILE),
        "laravel-octane" | "octane" => Ok(&LARAVEL_OCTANE_PROFILE),
        "gnuboard7" | "g7" => Ok(&GNUBOARD7_PROFILE),
        _ => Err(Error::InvalidOption {
            field: "app-profile",
            value,
            supported: SUPPORTED_APP_PROFILES.join(", "),
        }),
    }
}

impl AppProfile {
    pub fn document_root_for(&self, site_root: &str) -> String {
        match self.document_root {
            DocumentRootStrategy::SiteRoot => site_root.to_string(),
            DocumentRootStrategy::PublicSubdir => format!("{site_root}/public"),
        }
    }

    pub fn followup_steps(&self) -> Vec<AppFollowupStep> {
        self.post_install_steps
            .iter()
            .map(|step| AppFollowupStep {
                name: step,
                description: "app install phase",
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentRootStrategy, resolve_app_profile};

    #[test]
    fn resolves_default_gnuboard_alias() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let profile = resolve_app_profile("g7")?;

        assert_eq!(profile.id, "gnuboard7");
        assert_eq!(profile.document_root, DocumentRootStrategy::PublicSubdir);
        assert!(profile.php_extensions.contains(&"redis"));
        assert!(profile.system_packages.contains(&"composer"));
        Ok(())
    }

    #[test]
    fn wordpress_uses_site_root_document_root()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let profile = resolve_app_profile("wordpress")?;

        assert_eq!(
            profile.document_root_for("/home/g7/public_html"),
            "/home/g7/public_html"
        );
        assert!(profile.php_extensions.contains(&"mysqli"));
        Ok(())
    }

    #[test]
    fn laravel_octane_uses_public_root_and_frankenphp_service()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let profile = resolve_app_profile("octane")?;

        assert_eq!(profile.id, "laravel-octane");
        assert_eq!(
            profile.document_root_for("/home/g7/public_html"),
            "/home/g7/public_html/public"
        );
        assert!(
            profile
                .health_checks
                .contains(&"g7-frankenphp Octane service active")
        );
        assert!(
            profile
                .post_install_steps
                .iter()
                .any(|step| step.contains("Octane"))
        );
        Ok(())
    }
}
