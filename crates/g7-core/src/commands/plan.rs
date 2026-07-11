//! Canonical install policy for G7 Installer.
//!
//! This module is the source of truth for what the installer intends to manage.
//! CLI prompts, TUI fields, generated config, README examples, and release notes
//! must follow this plan instead of inventing separate defaults.
//!
//! Scope rule: plan may describe future server changes, but `install` must only
//! execute the subset that is implemented and tracked in state/owned-files.

use crate::app_profile::{AppFollowupStep, AppRequirement, resolve_app_profile};
use crate::{Error, Result};
use std::collections::HashSet;

pub use crate::app_profile::DEFAULT_APP_PROFILE;
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use g7_system::php::{
    DEFAULT_FPM_VERSION, PHP_SOURCE_AUTO, PHP_SOURCE_ONDREJ, PHP_SOURCE_UBUNTU,
    SUPPORTED_FPM_VERSIONS, SUPPORTED_PHP_SOURCES, UBUNTU_FPM_VERSION,
};

mod builder;
mod defaults;
mod normalize;
mod provisioning;
mod resources;
mod sizing;
mod types;

pub use builder::{PlanOptions, build, build_with_options};
pub use defaults::*;
pub use sizing::{ResolvedMemorySizing, resolve_memory_sizing};
pub use types::{
    InstallPlan, PlanFile, PlanGate, PlanPackage, PlanPort, PlanSecurityCheck, PlanService,
    PlanStopCondition, ProvisioningSection, ProvisioningSetting,
};

use normalize::*;
use provisioning::*;
use resources::*;
use sizing::*;

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
        assert_eq!(plan.php_source, "ondrej");
        assert_eq!(plan.database_engine, "mysql");
        assert_eq!(plan.database_version, "8.0");
        assert_eq!(plan.site_user, "g7");
        assert_eq!(plan.web_root_mode, "public-html");
        assert_eq!(plan.web_root, "/home/g7/public_html");
        assert_eq!(plan.security_profile, "standard");
        assert_eq!(plan.ssh_policy, "audit-only");
        assert_eq!(plan.www_mode, "redirect-to-www");
        assert_eq!(plan.redis_mode, "enable");
        assert_eq!(plan.mail_mode, "none");
        assert_eq!(plan.mode, "dry-run");
        assert!(!plan.changes_made);
        let web = plan
            .provisioning
            .iter()
            .find(|section| section.name == "web-server")
            .expect("web server section");
        assert!(web.settings.iter().any(|setting| {
            setting.key == "php_endpoint" && setting.value == "/run/php/php8.5-fpm-g7.sock"
        }));
        Ok(())
    }

    #[test]
    fn plan_describes_install_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build("example.com".to_string())?;

        assert!(plan.fresh_server_only);
        assert!(plan.packages.iter().any(|package| package.name == "nginx"));
        assert!(plan.packages.iter().any(|package| package.name == "git"));
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "composer")
        );
        assert!(plan.packages.iter().all(|package| {
            !package
                .name
                .split_whitespace()
                .any(|name| matches!(name, "nodejs" | "npm"))
        }));
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
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/home/g7/public_html")
        );
        assert!(plan.services.iter().any(|service| service.name == "nginx"));
        assert!(plan.services.iter().any(|service| service.name == "mysql"));
        assert!(plan.ports.iter().any(|port| port.port == 443));
        assert!(plan.ports.iter().any(|port| port.port == 3306));
        assert!(plan.ports.iter().any(|port| port.port == 6379));
        assert!(
            plan.security_checks
                .iter()
                .any(|check| check.name == "database-credentials")
        );
        assert!(
            plan.security_checks
                .iter()
                .any(|check| check.name == "ssh-config" && check.level == "audit")
        );
        assert!(
            plan.stop_conditions
                .iter()
                .any(|condition| condition.reason.contains("public IP"))
        );
        Ok(())
    }

    #[test]
    fn plan_includes_memory_sizing_presets() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let plan = build("example.com".to_string())?;
        let sizing = plan
            .provisioning
            .iter()
            .find(|section| section.name == "server-sizing")
            .expect("server sizing section");
        let php = plan
            .provisioning
            .iter()
            .find(|section| section.name == "php-runtime")
            .expect("php runtime section");
        let web = plan
            .provisioning
            .iter()
            .find(|section| section.name == "web-server")
            .expect("web server section");
        let database = plan
            .provisioning
            .iter()
            .find(|section| section.name == "database")
            .expect("database section");
        let redis = plan
            .provisioning
            .iter()
            .find(|section| section.name == "redis")
            .expect("redis section");

        assert!(sizing.summary.contains("1/2/4/8/16/32GB"));
        assert!(sizing.settings.iter().any(|setting| {
            setting.key == "tier_32gb"
                && setting.value.contains("db_buffer_pool=10G")
                && setting.value.contains("php_max_children=96")
                && setting.value.contains("apache_max_request_workers=400")
                && setting.value.contains("nginx_worker_processes=min(vCPU,8)")
        }));
        assert!(sizing.settings.iter().any(|setting| {
            setting.key == "tier_gt32gb"
                && setting.value.contains("db_buffer_pool=min(RAM*40%, 24G)")
                && setting.value.contains("redis_maxmemory=min(RAM*6%, 4G)")
                && setting
                    .value
                    .contains("apache_max_request_workers=min(vCPU*64, 800 per site)")
        }));
        assert!(web.settings.iter().any(|setting| {
            setting.key == "nginx_worker_processes_by_cpu_ram"
                && setting.value.contains("32GB=min(vCPU,8)")
                && setting.value.contains(">32GB=min(vCPU,16)")
        }));
        assert!(web.settings.iter().any(|setting| {
            setting.key == "apache_max_request_workers_by_ram"
                && setting.value.contains("16GB=300")
                && setting.value.contains("32GB=400")
                && setting.value.contains(">32GB=min(vCPU*64, 800 per site)")
        }));
        assert!(php.settings.iter().any(|setting| {
            setting.key == "max_children_by_ram"
                && setting.value.contains("32GB=96")
                && setting
                    .value
                    .contains(">32GB=min(floor(php_budget/128M), 192 per site)")
        }));
        assert!(php.settings.iter().any(|setting| {
            setting.key == "cpu_guard_by_ram"
                && setting
                    .value
                    .contains("32GB=min(memory_budget, vCPU*12, 96)")
        }));
        assert!(database.settings.iter().any(|setting| {
            setting.key == "buffer_pool_by_ram"
                && setting.value.contains("16GB=5G")
                && setting.value.contains("32GB=10G")
        }));
        assert!(redis.settings.iter().any(|setting| {
            setting.key == "maxmemory_by_ram"
                && setting.value.contains("32GB=2G")
                && setting.value.contains(">32GB=min(RAM*6%, 4G)")
        }));
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
    fn plan_supports_mysql_84_official_repository()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let plan = build_with_options(
            "example.com".to_string(),
            PlanOptions {
                database_version: "8.4".to_string(),
                ..PlanOptions::default()
            },
        )?;

        assert_eq!(plan.database_engine, "mysql");
        assert_eq!(plan.database_version, "8.4");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name == "mysql-apt-config")
        );
        Ok(())
    }

    #[test]
    fn plan_rejects_mariadb_and_unknown_mysql_versions() {
        let mariadb = build_with_options(
            "example.com".to_string(),
            PlanOptions {
                database_engine: "mariadb".to_string(),
                ..PlanOptions::default()
            },
        );
        assert!(matches!(
            mariadb,
            Err(Error::InvalidOption {
                field: "database",
                ..
            })
        ));

        let unknown = build_with_options(
            "example.com".to_string(),
            PlanOptions {
                database_version: "9.7".to_string(),
                ..PlanOptions::default()
            },
        );
        assert!(matches!(
            unknown,
            Err(Error::InvalidOption {
                field: "database-version",
                ..
            })
        ));
    }

    #[test]
    fn plan_supports_frankenphp_edge_runtime() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let options = PlanOptions {
            web_server: "frankenphp".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.web_server, "frankenphp");
        assert_eq!(plan.php_version, "8.5");
        assert_eq!(plan.php_source, "ondrej");
        assert!(plan.packages.iter().any(|package| package.name == "nginx"));
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name.contains("php8.5-cli"))
        );
        assert!(
            plan.packages
                .iter()
                .all(|package| !package.name.contains("php8.5-fpm"))
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "g7-frankenphp")
        );
        assert!(
            plan.services
                .iter()
                .all(|service| service.name != "php8.5-fpm")
        );
        assert!(
            plan.files
                .iter()
                .any(|file| file.path == "/opt/g7-frankenphp/frankenphp")
        );
        let web = plan
            .provisioning
            .iter()
            .find(|section| section.name == "web-server")
            .expect("web server section");
        assert!(web.settings.iter().any(
            |setting| setting.key == "selected_runtime" && setting.value.contains("FrankenPHP")
        ));
        assert!(web.settings.iter().any(
            |setting| setting.key == "php_endpoint" && setting.value.contains("127.0.0.1:7080")
        ));
        Ok(())
    }

    #[test]
    fn plan_allows_php_85_next_option() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            php_version: "8.5".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.php_version, "8.5");
        assert_eq!(plan.php_source, "ondrej");
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name.contains("php8.5-fpm"))
        );
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name.contains("php8.5-cli"))
        );
        assert!(
            plan.packages
                .iter()
                .any(|package| package.name.contains("software-properties-common"))
        );
        assert!(
            plan.packages
                .iter()
                .all(|package| !package.name.contains("php8.5-opcache"))
        );
        Ok(())
    }

    #[test]
    fn plan_uses_user_provided_database_credentials()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            database_name: Some("custom_g7".to_string()),
            database_user: Some("custom_user".to_string()),
            database_password: Some("Test-only_9x!".to_string()),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.database_name, "custom_g7");
        assert_eq!(plan.database_user, "custom_user");
        assert_eq!(
            plan.database_password_policy,
            "user-provided-store-root-only"
        );
        assert!(
            plan.provisioning
                .iter()
                .find(|section| section.name == "database")
                .expect("database section")
                .settings
                .iter()
                .any(|setting| setting.key == "password_policy"
                    && setting.value.contains("사용자 입력값"))
        );
        Ok(())
    }

    #[test]
    fn plan_supports_laravel_octane_only_with_frankenphp()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            app_profile: "laravel-octane".to_string(),
            web_server: "frankenphp".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.app_profile, "laravel-octane");
        assert_eq!(plan.app_profile_label, "Laravel Octane");
        assert_eq!(plan.web_server, "frankenphp");
        assert_eq!(plan.database_name, "laravel_example_com");
        assert!(
            plan.app_requirements
                .iter()
                .any(|requirement| requirement.name == "php-extension:pcntl")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "g7-frankenphp")
        );
        assert!(
            plan.provisioning
                .iter()
                .flat_map(|section| section.settings.iter())
                .any(|setting| {
                    setting.key == "rewrite_policy" && setting.value.contains("Octane")
                })
        );

        let invalid = build_with_options(
            "example.com".to_string(),
            PlanOptions {
                app_profile: "laravel-octane".to_string(),
                web_server: "nginx".to_string(),
                ..PlanOptions::default()
            },
        );
        assert!(invalid.is_err());
        Ok(())
    }

    #[test]
    fn plan_supports_gnuboard7_octane_only_with_frankenphp()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            app_profile: "gnuboard7-octane".to_string(),
            web_server: "frankenphp".to_string(),
            ..PlanOptions::default()
        };
        let plan = build_with_options("example.com".to_string(), options)?;

        assert_eq!(plan.app_profile, "gnuboard7-octane");
        assert_eq!(plan.app_profile_label, "Gnuboard 7 Octane");
        assert_eq!(plan.web_server, "frankenphp");
        assert_eq!(plan.database_name, "g7_example_com");
        assert!(
            plan.app_requirements
                .iter()
                .any(|requirement| requirement.name == "php-extension:pcntl")
        );
        assert!(
            plan.services
                .iter()
                .any(|service| service.name == "g7-frankenphp")
        );
        assert!(
            plan.provisioning
                .iter()
                .flat_map(|section| section.settings.iter())
                .any(|setting| {
                    setting.key == "rewrite_policy" && setting.value.contains("Gnuboard7")
                })
        );

        let invalid = build_with_options(
            "example.com".to_string(),
            PlanOptions {
                app_profile: "gnuboard7-octane".to_string(),
                web_server: "nginx".to_string(),
                ..PlanOptions::default()
            },
        );
        assert!(invalid.is_err());
        Ok(())
    }

    #[test]
    fn plan_rejects_php_85_with_ubuntu_source()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let options = PlanOptions {
            php_version: "8.5".to_string(),
            php_source: "ubuntu".to_string(),
            ..PlanOptions::default()
        };

        let err = match build_with_options("example.com".to_string(), options) {
            Ok(_) => return Err(std::io::Error::other("ubuntu php source should fail").into()),
            Err(err) => err,
        };

        assert!(matches!(
            err,
            Error::InvalidOption {
                field: "php-source",
                ..
            }
        ));
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
            smtp_username: Some("smtp-user".to_string()),
            smtp_password: Some("smtp-secret-123".to_string()),
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
