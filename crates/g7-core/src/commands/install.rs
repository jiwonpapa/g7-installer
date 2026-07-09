//! Server install phase for G7 Installer.
//!
//! This module persists the canonical plan into state/config/report files before
//! performing server changes. Every applied package/service step must be
//! represented in `plan.rs`, `state.json`, `owned-files.json`, and the report.
//!
//! Current phase rule: package installation, site account/web root creation,
//! Nginx/Apache/FrankenPHP vhost setup, PHP runtime/DB tuning, DB user creation,
//! TLS vhost mutation, app source handoff, and setup reporting are implemented.
//! Riskier shared-server mutations such as firewall changes remain deferred
//! until their rollback surface is explicit.

use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::net::IpAddr;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands::doctor::{self, DoctorCheckStatus};
use crate::commands::plan;
use crate::defaults::*;
use crate::installer_paths::{
    BACKUP_DIR, BACKUP_MANIFEST_PATH, CONFIG_PATH, ETC_DIR, LIB_DIR, LOCAL_HOSTS_PATH, LOG_DIR,
    LOG_PATH, REPORT_PATH, ROLLBACK_PATH, SECRETS_PATH, SETUP_GUIDE_PATH,
};
use crate::{Error, Result};
use g7_state::owned_files::{OWNED_FILES_PATH, OwnedFiles, write_owned_files};
use g7_state::state::{InstallerPhase, InstallerState, STATE_PATH, write_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandRunner, CommandSpec};
use g7_system::database::DatabaseEngine;
use g7_system::package::PackageStatus;
use g7_system::port::PortStatus;
use g7_system::service::ServiceActivity;

mod apps;
mod database;
mod orchestrator;
mod packages;
mod report;
mod runtime;
mod site;
mod tls;
mod vhost;

pub use orchestrator::{InstallPaths, run, run_with_probe_and_paths};
pub use report::{InstallCheck, InstallReport};

use apps::*;
use database::*;
use orchestrator::*;
use packages::*;
use report::*;
use runtime::*;
use site::*;
use tls::*;
use vhost::*;

#[cfg(test)]
mod tests {
    use super::{InstallPaths, run_with_probe_and_paths};
    use crate::Error;
    use g7_state::owned_files::OWNED_FILES_PATH;
    use g7_state::state::STATE_PATH;
    use g7_system::SystemProbe;
    use g7_system::command::{CommandOutput, FakeCommandRunner};
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn install_writes_prepared_state_and_owned_files()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths(
            "Example.COM.".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        )?;

        assert_eq!(report.domain, "example.com");
        assert_eq!(report.deployment_mode, "public");
        assert_eq!(report.web_server, "nginx");
        assert_eq!(report.php_version, "8.5");
        assert_eq!(report.php_source, g7_system::php::PHP_SOURCE_ONDREJ);
        assert_eq!(report.database_engine, "mysql");
        assert_eq!(report.site_user, "g7");
        assert_eq!(report.web_root_mode, "public-html");
        assert_eq!(report.web_root, "/home/g7/public_html");
        assert_eq!(report.redis_mode, "enable");
        assert_eq!(report.security_profile, "standard");
        assert_eq!(report.ssh_policy, "audit-only");
        assert_eq!(report.phase, "completed");
        assert!(fs_root.join("etc/g7-installer/config.toml").exists());
        let config = fs::read_to_string(fs_root.join("etc/g7-installer/config.toml"))?;
        assert!(config.contains("deployment_mode = \"public\""));
        assert!(config.contains("web_server = \"nginx\""));
        assert!(config.contains("php_version = \"8.5\""));
        assert!(config.contains("database = \"mysql\""));
        assert!(config.contains("database_password_policy = \"generate-random-store-root-only\""));
        assert!(config.contains("site_user = \"g7\""));
        assert!(config.contains("web_root = \"/home/g7/public_html\""));
        assert!(config.contains("www_mode = \"redirect-to-www\""));
        assert!(config.contains("redis = \"enable\""));
        assert!(config.contains("mail_mode = \"local-postfix\""));
        assert!(config.contains("security_profile = \"standard\""));
        assert!(config.contains("ssh_policy = \"audit-only\""));
        assert!(fs_root.join("var/lib/g7-installer/rollback.json").exists());
        assert!(fs_root.join("var/log/g7-installer/report.json").exists());
        assert!(fs_root.join("var/backups/g7-installer").exists());
        assert!(fs_root.join(strip_root(STATE_PATH)).exists());
        assert!(fs_root.join(strip_root(OWNED_FILES_PATH)).exists());
        assert!(fs_root.join("home/g7/public_html").exists());
        assert!(fs_root.join("home/g7/public_html/public").exists());
        assert!(
            fs_root
                .join("home/g7/public_html/public/g7inst-ready.php")
                .exists()
        );
        assert!(fs_root.join("etc/nginx/sites-available/g7.conf").exists());
        assert!(fs_root.join("etc/nginx/sites-enabled/g7.conf").exists());
        let nginx_vhost = fs::read_to_string(fs_root.join("etc/nginx/sites-available/g7.conf"))?;
        assert!(nginx_vhost.contains("proxy_pass http://127.0.0.1:8080;"));
        assert!(nginx_vhost.contains("location /app"));
        assert!(nginx_vhost.contains("access_log /var/log/nginx/g7-access.log;"));
        assert!(!nginx_vhost.contains("g7_timing"));
        assert!(nginx_vhost.contains("client_max_body_size"));
        assert!(nginx_vhost.contains("fastcgi_buffers"));
        assert!(nginx_vhost.contains("location ^~ /build/"));
        assert!(nginx_vhost.contains("public, max-age=2592000, immutable"));
        assert!(
            !fs_root
                .join("etc/nginx/conf.d/g7-runtime-tuning.conf")
                .exists()
        );
        let configtest_index = report
            .vhost_checks
            .iter()
            .position(|check| check.name == "nginx-configtest")
            .ok_or_else(|| std::io::Error::other("missing nginx config test check"))?;
        assert!(configtest_index < report.vhost_checks.len());
        assert_eq!(
            report
                .owned_files
                .iter()
                .filter(|path| path.as_str() == "/etc/nginx/conf.d/g7-runtime-tuning.conf")
                .count(),
            0
        );
        assert!(fs_root.join("etc/php/8.5/fpm/pool.d/g7-g7.conf").exists());
        let php_pool = fs::read_to_string(fs_root.join("etc/php/8.5/fpm/pool.d/g7-g7.conf"))?;
        assert!(php_pool.contains("request_slowlog_timeout = 2s"));
        assert!(php_pool.contains("slowlog = /var/log/php8.5-fpm-g7-slow.log"));
        assert!(
            fs_root
                .join("etc/php/8.5/fpm/conf.d/99-g7-installer.ini")
                .exists()
        );
        assert!(fs_root.join("swapfile").exists());
        assert!(fs_root.join("etc/systemd/system/swapfile.swap").exists());
        assert!(
            fs_root
                .join("etc/sysctl.d/99-g7-installer-swap.conf")
                .exists()
        );
        assert!(fs_root.join("etc/mysql/conf.d/g7-installer.cnf").exists());
        let database_runtime =
            fs::read_to_string(fs_root.join("etc/mysql/conf.d/g7-installer.cnf"))?;
        assert!(database_runtime.contains("slow_query_log = ON"));
        assert!(database_runtime.contains("long_query_time = 0.5"));
        assert!(fs_root.join("etc/g7-installer/secrets.toml").exists());
        assert!(fs_root.join("var/log/g7-installer/setup-guide.md").exists());
        assert!(fs_root.join("home/g7/public_html/.env").exists());
        assert!(
            fs_root
                .join("home/g7/public_html/storage/app/settings/drivers.json")
                .exists()
        );
        assert!(fs_root.join("etc/systemd/system/g7-queue.service").exists());
        assert!(
            fs_root
                .join("etc/systemd/system/g7-scheduler.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/g7-scheduler.timer")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/g7-reverb.service")
                .exists()
        );
        let app_env = fs::read_to_string(fs_root.join("home/g7/public_html/.env"))?;
        assert!(app_env.contains("APP_URL=https://www.example.com"));
        assert!(!app_env.contains("APP_URL=https://www.example.com/install"));
        assert!(app_env.contains("DB_HOST=localhost"));
        assert!(app_env.contains("DB_READ_DATABASE=g7"));
        assert!(app_env.contains("DB_READ_USERNAME=g7"));
        assert!(app_env.contains("DB_WRITE_DATABASE=g7"));
        assert!(app_env.contains("DB_WRITE_USERNAME=g7"));
        assert!(!app_env.contains("DB_HOST=127.0.0.1"));
        assert!(app_env.contains("CACHE_STORE=redis"));
        assert!(app_env.contains("CACHE_DRIVER=redis"));
        assert!(app_env.contains("SESSION_DRIVER=redis"));
        assert!(app_env.contains("QUEUE_CONNECTION=redis"));
        assert!(app_env.contains("BROADCAST_CONNECTION=reverb"));
        assert!(app_env.contains("VITE_REVERB_HOST=www.example.com"));
        assert!(app_env.contains("VITE_REVERB_PORT=443"));
        let driver_settings = fs::read_to_string(
            fs_root.join("home/g7/public_html/storage/app/settings/drivers.json"),
        )?;
        assert!(driver_settings.contains("\"cache_driver\": \"redis\""));
        assert!(driver_settings.contains("\"session_driver\": \"redis\""));
        assert!(driver_settings.contains("\"queue_driver\": \"sync\""));
        let recorded = probe.runner().recorded();
        assert!(recorded.iter().any(|spec| {
            spec.program == std::ffi::OsStr::new("debconf-set-selections")
                && spec.stdin.as_ref().is_some_and(|stdin| {
                    String::from_utf8_lossy(stdin)
                        .contains("postfix postfix/main_mailer_type select Internet Site")
                })
        }));
        assert!(
            recorded
                .iter()
                .any(|spec| { spec.display() == "postconf -e inet_interfaces = loopback-only" })
        );
        assert!(
            recorded
                .iter()
                .any(|spec| spec.display() == "postconf -e inet_protocols = ipv4")
        );
        assert!(
            recorded
                .iter()
                .any(|spec| spec.display() == "systemctl restart postfix")
        );
        let app_copy_index = recorded
            .iter()
            .position(|spec| {
                spec.display()
                    == "cp -a /var/lib/g7-installer/app-source/gnuboard7/. /home/g7/public_html"
            })
            .ok_or_else(|| std::io::Error::other("missing gnuboard7 app copy command"))?;
        let app_chown_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chown -R g7:www-data /home/g7/public_html")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing app chown command after copy"))?;
        let storage_chmod_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chmod -R 0775 /home/g7/public_html/storage")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing storage chmod command after copy"))?;
        let env_chmod_index = recorded
            .iter()
            .enumerate()
            .skip(app_copy_index + 1)
            .find(|(_, spec)| spec.display() == "chmod 0640 /home/g7/public_html/.env")
            .map(|(index, _)| index)
            .ok_or_else(|| std::io::Error::other("missing .env chmod command after copy"))?;
        let composer_index = recorded
            .iter()
            .position(|spec| spec.display().starts_with("composer install "))
            .ok_or_else(|| std::io::Error::other("missing composer install command"))?;
        assert!(app_copy_index < app_chown_index);
        assert!(app_chown_index < composer_index);
        assert!(storage_chmod_index < composer_index);
        assert!(env_chmod_index < composer_index);
        assert!(
            report
                .owned_files
                .contains(&"/home/g7/public_html".to_string())
        );
        assert!(
            report
                .completed_steps
                .contains(&"vhost-enabled".to_string())
        );
        assert!(
            report
                .package_checks
                .iter()
                .any(|check| { check.name == "nginx" && check.status == "pass" })
        );
        let report_json = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;
        assert!(report_json.contains("\"preinstall_package_checks\""));
        assert!(report_json.contains("\"status\": \"not-installed\""));
        assert!(
            report
                .service_checks
                .iter()
                .any(|check| { check.name == "nginx" && check.status == "pass" })
        );
        assert!(
            report
                .port_checks
                .iter()
                .any(|check| { check.name == "80" && check.status == "pass" })
        );
        assert!(
            report
                .network_checks
                .iter()
                .any(|check| { check.name == "server-public-ipv4" && check.status == "pass" })
        );
        assert!(
            report
                .network_checks
                .iter()
                .any(|check| { check.name == "dns-a" && check.status == "pass" })
        );
        assert!(
            report
                .mail_checks
                .iter()
                .any(|check| { check.name == "local-postfix" && check.status == "pass" })
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| { check.name == "tls-certificate" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "swapfile" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-fpm-pool" && check.status == "pass" })
        );
        assert!(report.runtime_checks.iter().any(|check| {
            check.name == "phpinfo-summary" && check.message.contains("FPM ini 기준")
        }));
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-runtime-limits" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-extension:pdo_mysql" && check.status == "pass" })
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| { check.name == "php-fpm-pool-values" && check.status == "pass" })
        );
        assert!(
            report
                .database_checks
                .iter()
                .any(|check| { check.name == "database-user-created" && check.status == "pass" })
        );
        assert!(
            report
                .safety_checks
                .iter()
                .any(|check| { check.name == "provider-snapshot" && check.status == "warn" })
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| { check.name == "http-smoke" && check.status == "pass" })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "composer-install" && check.status == "pass" })
        );
        assert!(
            report.app_checks.iter().any(|check| {
                check.name == "gnuboard7-driver-settings" && check.status == "pass"
            })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "artisan-migrate" && check.status == "manual" })
        );
        assert!(report.app_checks.iter().any(|check| {
            check.name == "app-service-file:g7-queue.service" && check.status == "pass"
        }));
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "app-services-enable" && check.status == "manual" })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "app-writable:storage" && check.status == "pass" })
        );
        assert!(report_json.contains("\"network_checks\""));
        assert!(report_json.contains("\"mail_checks\""));
        assert!(report_json.contains("\"certbot_checks\""));
        assert!(report_json.contains("\"runtime_checks\""));
        assert!(report_json.contains("\"database_checks\""));
        assert!(report_json.contains("\"setup_guide_path\""));
        assert!(report_json.contains("\"safety_checks\""));
        assert!(report_json.contains("\"vhost_checks\""));

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_continues_app_phase_when_certbot_is_rate_limited()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions::default();
        let probe = clean_probe_with_uid_for_options_and_certbot(
            &os_release_path,
            &fs_root,
            "0\n",
            "example.com",
            &options,
            CommandOutput::failure(1, "too many certificates already issued"),
        )?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.phase, "app-configured");
        assert!(report.completed_steps.contains(&"tls-deferred".to_string()));
        assert!(
            report
                .completed_steps
                .contains(&"app-source-prepared".to_string())
        );
        assert_eq!(report.app_url, "http://www.example.com/install");
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| check.name == "tls-rate-limited"
                    && check.status == "warn"
                    && check.message.contains("too many certificates"))
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "app-source" && check.status == "pass")
        );
        assert!(
            !report
                .vhost_checks
                .iter()
                .any(|check| check.name == "nginx-https-vhost")
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn command_failure_message_includes_command_output_excerpt() {
        let err = Error::InstallCommandFailed {
            step: "composer-install",
            command: "composer install".to_string(),
            status: 1,
            stdout: "stdout line".to_string(),
            stderr: "composer stderr line".to_string(),
        };

        let message = super::command_failure_message("Application source setup failed", &err);

        assert!(message.contains("Application source setup failed"));
        assert!(message.contains("stdout: stdout line"));
        assert!(message.contains("stderr: composer stderr line"));
    }

    #[test]
    fn letsencrypt_rate_limit_detection_reads_stderr() {
        let err = Error::InstallCommandFailed {
            step: "certbot-certonly",
            command: "certbot certonly".to_string(),
            status: 1,
            stdout: String::new(),
            stderr: "too many certificates already issued".to_string(),
        };

        assert!(super::is_letsencrypt_rate_limited(&err));
    }

    #[test]
    fn tls_phase_reuses_existing_certificate_without_certonly()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/letsencrypt/live/example.com"))?;
        fs::write(
            fs_root.join("etc/letsencrypt/live/example.com/fullchain.pem"),
            "cert",
        )?;
        fs::write(
            fs_root.join("etc/letsencrypt/live/example.com/privkey.pem"),
            "key",
        )?;
        let plan = super::plan::build_with_options(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
        )?;
        let paths = InstallPaths::with_root(&fs_root);
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _host in super::certificate_hosts(&plan) {
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("renew ok\n"));
        let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
        let mut owned = Vec::new();

        let checks = super::apply_tls_phase(&probe, &paths, &plan, &mut owned, &[])?;

        assert!(checks.iter().any(|check| {
            check.name == "tls-certificate"
                && check.status == "pass"
                && check.message.contains("기존 Let's Encrypt 인증서")
        }));
        let recorded = probe.runner().recorded();
        assert!(!recorded.iter().any(|spec| {
            spec.program == std::ffi::OsStr::new("certbot")
                && spec.args.contains(&OsString::from("certonly"))
        }));
        assert!(recorded.iter().any(|spec| {
            spec.program == std::ffi::OsStr::new("certbot")
                && spec.args.contains(&OsString::from("renew"))
        }));

        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_adopts_existing_g7_managed_swap_files()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/systemd/system"))?;
        fs::create_dir_all(fs_root.join("etc/sysctl.d"))?;
        fs::write(
            fs_root.join("etc/systemd/system/swapfile.swap"),
            "[Unit]\nDescription=G7 Installer managed swapfile\n",
        )?;
        fs::write(
            fs_root.join("etc/sysctl.d/99-g7-installer-swap.conf"),
            "vm.swappiness=10\nvm.vfs_cache_pressure=50\n",
        )?;
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        )?;

        assert_eq!(
            fs::read_to_string(fs_root.join("etc/systemd/system/swapfile.swap"))?,
            super::swap_unit_content()
        );
        assert_eq!(
            fs::read_to_string(fs_root.join("etc/sysctl.d/99-g7-installer-swap.conf"))?,
            super::swap_sysctl_content()
        );
        assert!(
            report
                .owned_files
                .contains(&"/etc/systemd/system/swapfile.swap".to_string())
        );
        assert!(
            report
                .owned_files
                .contains(&"/etc/sysctl.d/99-g7-installer-swap.conf".to_string())
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_configures_frankenphp_edge_runtime()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            web_server: "frankenphp".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.web_server, "frankenphp");
        assert_eq!(report.php_version, "8.5");
        assert_eq!(report.php_source, "ondrej");
        assert!(
            report
                .owned_files
                .contains(&"/opt/g7-frankenphp/frankenphp".to_string())
        );
        assert!(
            report
                .owned_files
                .contains(&"/etc/systemd/system/g7-frankenphp.service".to_string())
        );
        assert!(
            !report
                .package_checks
                .iter()
                .any(|check| check.name == "php8.5-fpm")
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| check.name == "frankenphp-service" && check.status == "pass")
        );
        assert!(
            report
                .service_checks
                .iter()
                .any(|check| check.name == "g7-frankenphp" && check.status == "pass")
        );
        assert!(report.runtime_checks.iter().any(|check| {
            check.name == "frankenphp-runtime-boundary" && check.message.contains("127.0.0.1:7080")
        }));
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| check.name == "frankenphp-https-vhost" && check.status == "pass")
        );

        let unit = fs::read_to_string(fs_root.join("etc/systemd/system/g7-frankenphp.service"))?;
        assert!(unit.contains("User=g7"));
        assert!(unit.contains("--listen 127.0.0.1:7080"));
        assert!(unit.contains("--root /home/g7/public_html/public"));
        let vhost = fs::read_to_string(fs_root.join("etc/nginx/sites-available/g7.conf"))?;
        assert!(vhost.contains("proxy_pass http://127.0.0.1:7080;"));
        assert!(!vhost.contains("fastcgi_pass"));
        let app_env = fs::read_to_string(fs_root.join("home/g7/public_html/.env"))?;
        assert!(app_env.contains("APP_URL=https://www.example.com"));
        assert!(!app_env.contains("APP_URL=https://www.example.com/install"));
        assert!(app_env.contains("DB_HOST=127.0.0.1"));
        assert!(app_env.contains("DB_READ_HOST=127.0.0.1"));
        assert!(app_env.contains("DB_WRITE_HOST=127.0.0.1"));
        assert!(app_env.contains("DB_READ_USERNAME=g7"));
        assert!(app_env.contains("DB_WRITE_USERNAME=g7"));
        let setup_guide = fs::read_to_string(fs_root.join("var/log/g7-installer/setup-guide.md"))?;
        assert!(setup_guide.contains("FrankenPHP service"));
        assert!(setup_guide.contains("sudo systemctl restart g7-frankenphp"));

        let recorded = probe.runner().recorded();
        assert!(recorded.iter().any(|spec| {
            spec.display().contains("frankenphp-linux-x86_64")
                && spec.display().contains("/opt/g7-frankenphp/frankenphp")
        }));
        assert!(
            recorded
                .iter()
                .any(|spec| spec.display() == "systemctl enable --now g7-frankenphp")
        );
        assert!(
            recorded
                .iter()
                .all(|spec| !spec.display().contains("php8.5-fpm"))
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_requires_root() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let probe = clean_probe_with_uid(&os_release_path, &fs_root, "1000\n")?;
        let paths = InstallPaths::with_root(&fs_root);

        let err = match run_with_probe_and_paths(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        ) {
            Ok(_) => return Err(std::io::Error::other("install should require root").into()),
            Err(err) => err,
        };

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(matches!(err, Error::PrivilegeRequired));
        Ok(())
    }

    #[test]
    fn install_blocks_when_fresh_server_gate_fails()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("var/www/g7"))?;
        let probe = clean_root_probe(&os_release_path, &fs_root)?;
        let paths = InstallPaths::with_root(&fs_root);

        let err = match run_with_probe_and_paths(
            "example.com".to_string(),
            super::plan::PlanOptions::default(),
            &probe,
            &paths,
        ) {
            Ok(_) => return Err(std::io::Error::other("install should be blocked").into()),
            Err(err) => err,
        };

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;

        assert!(matches!(err, Error::InstallBlocked { .. }));
        Ok(())
    }

    #[test]
    fn install_writes_local_hosts_hint_for_local_test()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            local_test: true,
            dns_check: true,
            www_mode: "none".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "g7-test.local", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report =
            run_with_probe_and_paths("g7-test.local".to_string(), options, &probe, &paths)?;

        let local_hosts = fs::read_to_string(fs_root.join("etc/g7-installer/local-hosts.txt"))?;
        assert_eq!(report.deployment_mode, "local-test");
        assert!(local_hosts.contains("127.0.0.1 g7-test.local"));
        assert!(
            report
                .network_checks
                .iter()
                .any(|check| { check.name == "dns-public-ip" && check.status == "skipped" })
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| { check.name == "certbot" && check.status == "skipped" })
        );
        assert!(
            report
                .completed_steps
                .contains(&"local-hosts-suggestion-written".to_string())
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_applies_apache_vhost_runtime_tls_and_app_link()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            web_server: "apache".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.web_server, "apache");
        assert_eq!(report.app_url, "https://www.example.com/install");
        assert!(fs_root.join("etc/apache2/sites-available/g7.conf").exists());
        assert!(fs_root.join("etc/apache2/sites-enabled/g7.conf").exists());
        let apache_vhost = fs::read_to_string(fs_root.join("etc/apache2/sites-available/g7.conf"))?;
        assert!(apache_vhost.contains("ProxyPass /app ws://127.0.0.1:8080/app"));
        assert!(
            report
                .service_checks
                .iter()
                .any(|check| check.name == "apache2" && check.status == "pass")
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| check.name == "apache-vhost" && check.status == "pass")
        );
        assert!(
            report
                .runtime_checks
                .iter()
                .any(|check| check.name == "apache-runtime-reload" && check.status == "pass")
        );
        assert!(
            report
                .certbot_checks
                .iter()
                .any(|check| check.name == "apache-https-vhost" && check.status == "pass")
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "app-url" && check.status == "pass")
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn php_runtime_failures_block_app_phase() {
        let message = super::blocking_runtime_failure(&[
            super::InstallCheck::pass("phpinfo-summary", "parsed"),
            super::InstallCheck::fail("php-extension:redis", "redis missing"),
        ])
        .expect("php extension failure should block");

        assert!(message.contains("PHP 런타임 진단 실패"));
        assert!(message.contains("php-extension:redis"));
    }

    #[test]
    fn install_reports_smtp_relay_reachability()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            mail_mode: "smtp-relay".to_string(),
            smtp_host: Some("smtp.example.com".to_string()),
            smtp_from: Some("no-reply@example.com".to_string()),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.mail_mode, "smtp-relay");
        assert_eq!(report.smtp_host.as_deref(), Some("smtp.example.com"));
        assert_eq!(report.smtp_port, Some(587));
        assert!(
            report
                .mail_checks
                .iter()
                .any(|check| { check.name == "smtp-relay" && check.status == "pass" })
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_configures_laravel_octane_on_frankenphp()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            app_profile: "laravel-octane".to_string(),
            web_server: "frankenphp".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.app_profile, "laravel-octane");
        assert_eq!(report.app_url, "https://www.example.com/");
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "composer-require-octane" && check.status == "pass" })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "artisan-octane-install" && check.status == "pass" })
        );
        assert!(
            report.app_checks.iter().any(|check| {
                check.name == "frankenphp-octane-active" && check.status == "pass"
            })
        );

        let unit = fs::read_to_string(fs_root.join("etc/systemd/system/g7-frankenphp.service"))?;
        assert!(unit.contains("Description=G7 Laravel Octane on FrankenPHP"));
        assert!(unit.contains("artisan octane:frankenphp"));
        assert!(unit.contains("--host=127.0.0.1 --port=7080"));

        let env = fs::read_to_string(fs_root.join("home/g7/public_html/.env"))?;
        assert!(env.contains("OCTANE_SERVER=frankenphp"));
        assert!(env.contains("OCTANE_HTTPS=true"));

        let recorded = probe.runner().recorded();
        assert!(
            recorded.iter().any(|spec| {
                spec.display() == "composer require laravel/octane --no-interaction"
            })
        );
        assert!(recorded.iter().any(|spec| {
            spec.display() == "php artisan octane:install --server=frankenphp --no-interaction"
        }));
        assert!(
            recorded
                .iter()
                .any(|spec| { spec.display() == "systemctl restart g7-frankenphp" })
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_configures_gnuboard7_octane_on_frankenphp()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            app_profile: "gnuboard7-octane".to_string(),
            web_server: "frankenphp".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.app_profile, "gnuboard7-octane");
        assert_eq!(report.app_url, "https://www.example.com/install");
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "composer-require-octane" && check.status == "pass" })
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| { check.name == "artisan-octane-install" && check.status == "pass" })
        );
        assert!(
            report.app_checks.iter().any(|check| {
                check.name == "frankenphp-octane-active" && check.status == "pass"
            })
        );

        let unit = fs::read_to_string(fs_root.join("etc/systemd/system/g7-frankenphp.service"))?;
        assert!(unit.contains("Description=G7 Gnuboard 7 Octane on FrankenPHP"));
        assert!(unit.contains("artisan octane:frankenphp"));
        assert!(unit.contains("--host=127.0.0.1 --port=7080"));

        let env = fs::read_to_string(fs_root.join("home/g7/public_html/.env"))?;
        assert!(env.contains("OCTANE_SERVER=frankenphp"));
        assert!(env.contains("OCTANE_HTTPS=true"));
        assert!(env.contains("BROADCAST_CONNECTION=reverb"));

        let recorded = probe.runner().recorded();
        assert!(
            recorded.iter().any(|spec| {
                spec.display() == "composer require laravel/octane --no-interaction"
            })
        );
        assert!(recorded.iter().any(|spec| {
            spec.display() == "php artisan octane:install --server=frankenphp --no-interaction"
        }));
        assert!(
            recorded
                .iter()
                .any(|spec| { spec.display() == "systemctl restart g7-frankenphp" })
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_laravel_runs_runtime_pipeline_and_services()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            app_profile: "laravel".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert_eq!(report.app_profile, "laravel");
        assert_eq!(report.app_url, "https://www.example.com/");
        assert!(fs_root.join("home/g7/public_html/.env").exists());
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-queue.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-scheduler.service")
                .exists()
        );
        assert!(
            fs_root
                .join("etc/systemd/system/laravel-scheduler.timer")
                .exists()
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "composer-install" && check.status == "pass")
        );
        assert!(
            report
                .app_checks
                .iter()
                .any(|check| check.name == "artisan-migrate" && check.status == "pass")
        );
        assert!(report.app_checks.iter().any(|check| check.name
            == "app-service:laravel-queue.service"
            && check.status == "pass"));

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_sets_site_account_password_when_requested()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            site_user_password: Some("0808dong!!".to_string()),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

        assert!(
            report
                .completed_steps
                .contains(&"site-user-password-set".to_string())
        );
        assert!(
            report
                .vhost_checks
                .iter()
                .any(|check| check.name == "site-user-password" && check.status == "pass")
        );

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_fails_before_install_when_package_candidate_is_missing()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/sites-available"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let options = super::plan::PlanOptions {
            php_version: "8.5".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let install_plan =
            super::plan::build_with_options("example.com".to_string(), options.clone())?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("0\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _package in super::package_names(&install_plan) {
            runner.push_output(CommandOutput::failure(1, "no packages found"));
        }
        runner.push_output(CommandOutput::success("apt update ok\n"));
        runner.push_output(CommandOutput::success(
            "php source prerequisites installed\n",
        ));
        runner.push_output(CommandOutput::success("ondrej ppa added\n"));
        runner.push_output(CommandOutput::success("apt update after php source ok\n"));
        runner.push_output(CommandOutput::success("nginx:\n  Candidate: 1\n"));
        runner.push_output(CommandOutput::success("php8.5-fpm:\n  Candidate: (none)\n"));
        let probe = SystemProbe::new(runner)
            .with_os_release_path(&os_release_path)
            .with_fs_root(&fs_root);
        let paths = InstallPaths::with_root(&fs_root);

        let err = match run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)
        {
            Ok(_) => {
                return Err(std::io::Error::other("missing package should fail").into());
            }
            Err(err) => err,
        };

        let report = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;
        let state = fs::read_to_string(fs_root.join(strip_root(STATE_PATH)))?;

        assert!(matches!(err, Error::PackageUnavailable { package } if package == "php8.5-fpm"));
        assert!(report.contains("\"phase\": \"package-failed\""));
        assert!(report.contains("php8.5-fpm"));
        assert!(state.contains("\"phase\": \"package-failed\""));

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    #[test]
    fn install_adds_ondrej_source_for_php_85() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let os_release_path = write_temp_os_release()?;
        let fs_root = create_temp_fs_root()?;
        let options = super::plan::PlanOptions {
            php_version: "8.5".to_string(),
            ..super::plan::PlanOptions::default()
        };
        let probe =
            clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
        let paths = InstallPaths::with_root(&fs_root);

        let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;
        let report_json = fs::read_to_string(fs_root.join("var/log/g7-installer/report.json"))?;

        assert_eq!(report.php_version, "8.5");
        assert_eq!(report.php_source, "ondrej");
        assert!(
            report
                .completed_steps
                .contains(&"php-apt-source-added".to_string())
        );
        assert!(
            report
                .completed_steps
                .contains(&"apt-updated-after-php-source".to_string())
        );
        assert!(report_json.contains("\"php_source\": \"ondrej\""));

        fs::remove_file(os_release_path)?;
        fs::remove_dir_all(fs_root)?;
        Ok(())
    }

    fn clean_root_probe(
        os_release_path: &Path,
        fs_root: &Path,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid(os_release_path, fs_root, "0\n")
    }

    fn clean_probe_with_uid(
        os_release_path: &Path,
        fs_root: &Path,
        uid: &str,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid_for_options(
            os_release_path,
            fs_root,
            uid,
            "example.com",
            &super::plan::PlanOptions::default(),
        )
    }

    fn clean_root_probe_for_options(
        os_release_path: &Path,
        fs_root: &Path,
        domain: &str,
        options: &super::plan::PlanOptions,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid_for_options(os_release_path, fs_root, "0\n", domain, options)
    }

    fn clean_probe_with_uid_for_options(
        os_release_path: &Path,
        fs_root: &Path,
        uid: &str,
        domain: &str,
        options: &super::plan::PlanOptions,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        clean_probe_with_uid_for_options_and_certbot(
            os_release_path,
            fs_root,
            uid,
            domain,
            options,
            CommandOutput::success("cert issued\n"),
        )
    }

    fn clean_probe_with_uid_for_options_and_certbot(
        os_release_path: &Path,
        fs_root: &Path,
        uid: &str,
        domain: &str,
        options: &super::plan::PlanOptions,
        certbot_output: CommandOutput,
    ) -> std::result::Result<SystemProbe<FakeCommandRunner>, Box<dyn std::error::Error>> {
        fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/sites-available"))?;
        fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success(uid));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success("inactive\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        let plan = super::plan::build_with_options(domain.to_string(), options.clone())?;
        push_successful_apply_outputs_with_certbot(
            &runner,
            &plan,
            options.site_user_password.is_some(),
            certbot_output,
        );

        Ok(SystemProbe::new(runner)
            .with_os_release_path(os_release_path)
            .with_fs_root(fs_root))
    }

    fn push_successful_apply_outputs_with_certbot(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
        site_password_set: bool,
        certbot_output: CommandOutput,
    ) {
        let packages = super::package_names(install_plan);
        let services = super::managed_services(install_plan);
        let ports = super::managed_ports(install_plan);

        for _package in &packages {
            runner.push_output(CommandOutput::failure(1, "no packages found"));
        }
        runner.push_output(CommandOutput::success("apt update ok\n"));
        if install_plan.php_source == g7_system::php::PHP_SOURCE_ONDREJ {
            runner.push_output(CommandOutput::success(
                "php source prerequisites installed\n",
            ));
            runner.push_output(CommandOutput::success("ondrej ppa added\n"));
            runner.push_output(CommandOutput::success("apt update after php source ok\n"));
        }
        for package in &packages {
            runner.push_output(CommandOutput::success(format!(
                "{package}:\n  Candidate: 1\n"
            )));
        }
        if install_plan.mail_mode == "local-postfix" && packages.iter().any(|p| p == "postfix") {
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success("apt install ok\n"));
        for _service in &services {
            runner.push_output(CommandOutput::success(""));
        }
        if install_plan.mail_mode == "local-postfix" {
            for _setting in super::local_postfix_runtime_settings(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success(""));
        }
        for _package in &packages {
            runner.push_output(CommandOutput::success("install ok installed"));
        }
        for _service in &services {
            runner.push_output(CommandOutput::success("active\n"));
        }
        for port in &ports {
            runner.push_output(CommandOutput::success(format!(
                "tcp LISTEN 0 4096 127.0.0.1:{port} 0.0.0.0:*\n"
            )));
        }
        push_successful_network_outputs(runner, install_plan);
        push_successful_mail_outputs(runner, install_plan);
        push_successful_site_and_vhost_outputs(
            runner,
            install_plan,
            site_password_set,
            certbot_output,
        );
    }

    fn push_successful_network_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        if !install_plan.dns_check_required {
            return;
        }

        runner.push_output(CommandOutput::success("203.0.113.10\n"));
        for host in super::certificate_hosts(install_plan) {
            runner.push_output(CommandOutput::success(format!(
                "203.0.113.10 STREAM {host}\n203.0.113.10 DGRAM {host}\n"
            )));
        }
    }

    fn push_successful_mail_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        match install_plan.mail_mode.as_str() {
            "smtp-relay" => runner.push_output(CommandOutput::success("")),
            "local-postfix" => runner.push_output(CommandOutput::success("active\n")),
            _ => {}
        }
    }

    fn push_successful_site_and_vhost_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
        site_password_set: bool,
        certbot_output: CommandOutput,
    ) {
        runner.push_output(CommandOutput::failure(1, "no such user"));
        runner.push_output(CommandOutput::success(""));
        if site_password_set {
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        if install_plan.web_server == "apache" {
            for _module in super::apache_http_modules() {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            push_runtime_database_tls_outputs(runner, install_plan, certbot_output);
            return;
        }
        if install_plan.web_server == "frankenphp" {
            runner.push_output(CommandOutput::success("x86_64\n"));
            runner.push_output(CommandOutput::success("downloaded\n"));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("active\n"));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            push_runtime_database_tls_outputs(runner, install_plan, certbot_output);
            return;
        }

        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        push_runtime_database_tls_outputs(runner, install_plan, certbot_output);
    }

    fn push_runtime_database_tls_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
        certbot_output: CommandOutput,
    ) {
        runner.push_output(CommandOutput::success(""));
        if matches!(
            install_plan.web_server.as_str(),
            "nginx" | "apache" | "frankenphp"
        ) {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
        }
        runner.push_output(CommandOutput::success(successful_php_runtime_probe_output(
            install_plan,
        )));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));

        if install_plan.deployment_mode == "public" && install_plan.web_server == "nginx" {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            for _host in super::certificate_hosts(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            let certbot_succeeded = certbot_output.status == 0;
            runner.push_output(certbot_output);
            if certbot_succeeded {
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success("renew ok\n"));
            }
        } else if install_plan.deployment_mode == "public" && install_plan.web_server == "apache" {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            for _host in super::certificate_hosts(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            let certbot_succeeded = certbot_output.status == 0;
            runner.push_output(certbot_output);
            if certbot_succeeded {
                for _module in super::apache_tls_modules() {
                    runner.push_output(CommandOutput::success(""));
                }
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success("renew ok\n"));
            }
        } else if install_plan.deployment_mode == "public"
            && install_plan.web_server == "frankenphp"
        {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            for _host in super::certificate_hosts(install_plan) {
                runner.push_output(CommandOutput::success(""));
            }
            let certbot_succeeded = certbot_output.status == 0;
            runner.push_output(certbot_output);
            if certbot_succeeded {
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success("renew ok\n"));
            }
        }
        push_successful_app_outputs(runner, install_plan);
    }

    fn successful_php_runtime_probe_output(install_plan: &super::plan::InstallPlan) -> String {
        let sizing = super::plan::resolve_memory_sizing(1024 * 1024, 1);
        let extensions = super::required_php_extensions(install_plan).join(",");
        format!(
            "php_version={}\n\
             sapi=cli\n\
             loaded_ini=/etc/php/{}/fpm/php.ini\n\
             scan_dir=/etc/php/{}/fpm/conf.d\n\
             memory_limit={}\n\
             upload_max_filesize={}\n\
             post_max_size={}\n\
             max_execution_time=120\n\
             max_input_vars=3000\n\
             date.timezone=UTC\n\
             realpath_cache_size=4096K\n\
             realpath_cache_ttl=600\n\
             opcache.enable=1\n\
             opcache.memory_consumption={}\n\
             opcache.validate_timestamps=0\n\
             opcache.enable_file_override=1\n\
             extensions={}\n",
            install_plan.php_version,
            install_plan.php_version,
            install_plan.php_version,
            sizing.php_memory_limit,
            sizing.php_upload_limit,
            sizing.php_upload_limit,
            sizing.opcache_memory.trim_end_matches('M'),
            extensions
        )
    }

    fn push_successful_app_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        match install_plan.app_profile.as_str() {
            "gnuboard7" | "gnuboard7-octane" => {
                runner.push_output(CommandOutput::success("cloned\n"));
                push_successful_git_validation_outputs(runner, super::GNUBOARD7_REQUIRED_FILES);
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(runner, super::GNUBOARD7_REQUIRED_FILES, &[]);
                push_successful_app_permission_outputs(runner, install_plan);
                runner.push_output(CommandOutput::success("composer ok\n"));
                if install_plan.app_profile == "gnuboard7-octane" {
                    runner.push_output(CommandOutput::success("octane composer ok\n"));
                    runner.push_output(CommandOutput::success("octane installed\n"));
                }
                runner.push_output(CommandOutput::success("npm install ok\n"));
                runner.push_output(CommandOutput::success("npm build ok\n"));
                runner.push_output(CommandOutput::success("key generated\n"));
                runner.push_output(CommandOutput::success("storage linked\n"));
                runner.push_output(CommandOutput::success(""));
                if install_plan.app_profile == "gnuboard7-octane" {
                    runner.push_output(CommandOutput::success(""));
                    runner.push_output(CommandOutput::success(""));
                    runner.push_output(CommandOutput::success("active\n"));
                }
            }
            "wordpress" => {
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(
                    runner,
                    super::WORDPRESS_REQUIRED_FILES,
                    super::WORDPRESS_REQUIRED_DIRS,
                );
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(
                    runner,
                    super::WORDPRESS_REQUIRED_FILES,
                    super::WORDPRESS_REQUIRED_DIRS,
                );
                push_successful_app_permission_outputs(runner, install_plan);
            }
            "laravel" | "laravel-octane" => {
                runner.push_output(CommandOutput::success("cloned\n"));
                push_successful_git_validation_outputs(runner, super::LARAVEL_REQUIRED_FILES);
                runner.push_output(CommandOutput::success(""));
                push_successful_required_path_outputs(runner, super::LARAVEL_REQUIRED_FILES, &[]);
                push_successful_app_permission_outputs(runner, install_plan);
                runner.push_output(CommandOutput::success("composer ok\n"));
                if install_plan.app_profile == "laravel-octane" {
                    runner.push_output(CommandOutput::success("octane composer ok\n"));
                    runner.push_output(CommandOutput::success("octane installed\n"));
                }
                runner.push_output(CommandOutput::success("npm install ok\n"));
                runner.push_output(CommandOutput::success("npm build ok\n"));
                runner.push_output(CommandOutput::success("key generated\n"));
                runner.push_output(CommandOutput::success("storage linked\n"));
                runner.push_output(CommandOutput::success("migrated\n"));
                runner.push_output(CommandOutput::success("optimized\n"));
                runner.push_output(CommandOutput::success("artisan about\n"));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                runner.push_output(CommandOutput::success(""));
                if install_plan.app_profile == "laravel-octane" {
                    runner.push_output(CommandOutput::success(""));
                    runner.push_output(CommandOutput::success(""));
                    runner.push_output(CommandOutput::success("active\n"));
                }
            }
            _ => {
                push_successful_app_permission_outputs(runner, install_plan);
            }
        }
    }

    fn push_successful_git_validation_outputs(runner: &FakeCommandRunner, required_files: &[&str]) {
        runner.push_output(CommandOutput::success("deadbeef\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        push_successful_required_path_outputs(runner, required_files, &[]);
    }

    fn push_successful_required_path_outputs(
        runner: &FakeCommandRunner,
        files: &[&str],
        dirs: &[&str],
    ) {
        for _file in files {
            runner.push_output(CommandOutput::success(""));
        }
        for _dir in dirs {
            runner.push_output(CommandOutput::success(""));
        }
    }

    fn push_successful_app_permission_outputs(
        runner: &FakeCommandRunner,
        install_plan: &super::plan::InstallPlan,
    ) {
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _writable_path in super::app_writable_paths(install_plan) {
            runner.push_output(CommandOutput::success(""));
        }
        if matches!(
            install_plan.app_profile.as_str(),
            "gnuboard7" | "gnuboard7-octane" | "laravel" | "laravel-octane"
        ) {
            runner.push_output(CommandOutput::success(""));
        }
    }

    fn write_temp_os_release() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut path = std::env::temp_dir();
        path.push(format!("g7-install-os-release-{}", unique_temp_suffix()?));
        fs::write(
            &path,
            "ID=ubuntu\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\n",
        )?;
        Ok(path)
    }

    fn create_temp_fs_root() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
        let mut root = std::env::temp_dir();
        root.push(format!("g7-install-fs-root-{}", unique_temp_suffix()?));
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn unique_temp_suffix() -> std::result::Result<String, Box<dyn std::error::Error>> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        Ok(format!("{}-{nanos}-{count}", std::process::id()))
    }

    fn strip_root(path: &str) -> &str {
        match path.strip_prefix('/') {
            Some(stripped) => stripped,
            None => path,
        }
    }
}
