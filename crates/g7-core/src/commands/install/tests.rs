use super::{InstallPaths, http_host_path_smoke_with_reload_grace, run_with_probe_and_paths};
use crate::Error;
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::STATE_PATH;
use g7_system::SystemProbe;
use g7_system::command::{
    CommandError, CommandOutput, CommandRunner, CommandSpec, FakeCommandRunner,
};
use std::cell::Cell;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn vhost_smoke_retries_while_reload_workers_converge()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(22, "not active yet"));
    runner.push_output(CommandOutput::success(""));
    let probe = SystemProbe::new(runner);

    assert!(http_host_path_smoke_with_reload_grace(
        &probe,
        "www.example.com",
        "/g7inst-ready.php"
    )?);
    assert_eq!(probe.runner().recorded().len(), 2);
    Ok(())
}

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
    assert!(config.contains("phase = \"completed\""));
    assert!(config.contains("web_server = \"nginx\""));
    assert!(config.contains("php_version = \"8.5\""));
    assert!(config.contains("database = \"mysql\""));
    assert!(config.contains("database_password_policy = \"generate-random-store-root-only\""));
    assert!(config.contains("site_user = \"g7\""));
    assert!(config.contains("web_root = \"/home/g7/public_html\""));
    assert!(config.contains("www_mode = \"redirect-to-www\""));
    assert!(config.contains("redis = \"enable\""));
    assert!(config.contains("mail_mode = \"none\""));
    assert!(config.contains("security_profile = \"standard\""));
    assert!(config.contains("ssh_policy = \"audit-only\""));
    assert!(fs_root.join("var/lib/g7-installer/rollback.json").exists());
    assert!(fs_root.join("var/log/g7-installer/report.json").exists());
    assert!(fs_root.join("var/backups/g7-installer").exists());
    assert!(fs_root.join(strip_root(STATE_PATH)).exists());
    let state = g7_state::state::read_state_file(&fs_root.join(strip_root(STATE_PATH)))?;
    assert_eq!(state.version, g7_state::state::STATE_VERSION);
    assert!(state.current_step.is_none());
    for step in [
        "packages", "site", "vhost", "runtime", "database", "tls", "app",
    ] {
        assert!(state.step_is_completed(step), "incomplete step: {step}");
    }
    assert!(fs_root.join(strip_root(OWNED_FILES_PATH)).exists());
    assert!(fs_root.join("home/g7/public_html").exists());
    assert!(fs_root.join("home/g7/public_html/public").exists());
    assert!(
        !fs_root
            .join("home/g7/public_html/public/g7inst-ready.php")
            .exists()
    );
    assert!(
        !report
            .owned_files
            .contains(&"/home/g7/public_html/public/g7inst-ready.php".to_string())
    );
    assert!(fs_root.join("etc/nginx/sites-available/g7.conf").exists());
    assert!(fs_root.join("etc/nginx/sites-enabled/g7.conf").exists());
    let nginx_vhost = fs::read_to_string(fs_root.join("etc/nginx/sites-available/g7.conf"))?;
    assert!(nginx_vhost.contains("access_log /var/log/nginx/g7-access.log;"));
    assert!(!nginx_vhost.contains("g7_timing"));
    assert!(nginx_vhost.contains("client_max_body_size"));
    assert!(nginx_vhost.contains("fastcgi_buffers"));
    assert!(nginx_vhost.contains("try_files $uri $uri/ /index.php?$query_string;"));
    assert!(nginx_vhost.contains("fastcgi_pass unix:/run/php/php8.5-fpm-g7.sock;"));
    assert!(!nginx_vhost.contains("fastcgi_pass unix:/run/php/php8.5-fpm.sock;"));
    assert!(!nginx_vhost.contains("location ~*"));
    assert!(!nginx_vhost.contains("location /app"));
    assert!(!nginx_vhost.contains("location /apps"));
    assert!(nginx_vhost.contains("location ~ ^/apps?(/|$)"));
    assert!(nginx_vhost.contains("proxy_pass http://127.0.0.1:8080;"));
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
    let database_runtime = fs::read_to_string(fs_root.join("etc/mysql/conf.d/g7-installer.cnf"))?;
    assert!(database_runtime.contains("slow_query_log = ON"));
    assert!(database_runtime.contains("long_query_time = 1"));
    assert!(database_runtime.contains("min_examined_row_limit = 100"));
    assert!(fs_root.join("etc/g7-installer/secrets.toml").exists());
    assert!(
        !fs_root
            .join("var/lib/g7-installer/pending-secrets.toml")
            .exists()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(fs_root.join("etc/g7-installer/secrets.toml"))?
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    assert!(fs_root.join("var/log/g7-installer/setup-guide.md").exists());
    let setup_guide = fs::read_to_string(fs_root.join("var/log/g7-installer/setup-guide.md"))?;
    for expected in [
        "/etc/nginx/sites-available/g7.conf",
        "/etc/nginx/nginx.conf",
        "/etc/php/8.5/fpm/pool.d/g7-g7.conf",
        "/run/php/php8.5-fpm-g7.sock",
        "/etc/php/8.5/fpm/conf.d/99-g7-installer.ini",
        "/etc/mysql/conf.d/g7-installer.cnf",
        "/swapfile",
        "/etc/systemd/system/swapfile.swap",
        "/etc/sysctl.d/99-g7-installer-swap.conf",
        "/etc/letsencrypt/renewal/example.com.conf",
        "/var/log/php8.5-fpm-g7-slow.log",
        "/home/g7/public_html/.env",
        "/var/log/g7-installer/commands.jsonl",
    ] {
        assert!(
            setup_guide.contains(expected),
            "setup guide omitted standard path: {expected}"
        );
    }
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "app-env-created" && check.status == "pass")
    );
    assert!(
        !fs_root
            .join("home/g7/public_html/storage/app/settings/drivers.json")
            .exists()
    );
    assert!(!fs_root.join("etc/systemd/system/g7-queue.service").exists());
    assert!(
        !fs_root
            .join("etc/systemd/system/g7-reverb.service")
            .exists()
    );
    let recorded = probe.runner().recorded();
    let fpm_reload_index = recorded
        .iter()
        .position(|spec| spec.display() == "systemctl reload php8.5-fpm")
        .ok_or_else(|| std::io::Error::other("missing PHP-FPM reload"))?;
    let http_smoke_index = recorded
        .iter()
        .position(|spec| {
            spec.program == "curl"
                && spec
                    .args
                    .contains(&OsString::from("http://127.0.0.1/g7inst-ready.php"))
        })
        .ok_or_else(|| std::io::Error::other("missing HTTP smoke"))?;
    assert!(
        fpm_reload_index < http_smoke_index,
        "PHP-FPM must be configured before the vhost HTTP smoke"
    );
    assert!(!recorded.iter().any(|spec| {
        spec.program == std::ffi::OsStr::new("debconf-set-selections")
            && spec.stdin.as_ref().is_some_and(|stdin| {
                String::from_utf8_lossy(stdin)
                    .contains("postfix postfix/main_mailer_type select Internet Site")
            })
    }));
    assert!(
        !recorded
            .iter()
            .any(|spec| { spec.display() == "postconf -e inet_interfaces = loopback-only" })
    );
    assert!(
        !recorded
            .iter()
            .any(|spec| spec.display() == "postconf -e inet_protocols = ipv4")
    );
    assert!(
        !recorded
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
    let env_copy_index = recorded
        .iter()
        .enumerate()
        .skip(app_copy_index + 1)
        .find(|(_, spec)| {
            spec.display() == "cp -- /home/g7/public_html/.env.example /home/g7/public_html/.env"
        })
        .map(|(index, _)| index)
        .ok_or_else(|| std::io::Error::other("missing G7 .env copy command"))?;
    let env_chmod_index = recorded
        .iter()
        .enumerate()
        .skip(env_copy_index + 1)
        .find(|(_, spec)| spec.display() == "chmod 0600 /home/g7/public_html/.env")
        .map(|(index, _)| index)
        .ok_or_else(|| std::io::Error::other("missing G7 .env permission command"))?;
    let storage_chmod_index = recorded
        .iter()
        .enumerate()
        .skip(app_copy_index + 1)
        .find(|(_, spec)| spec.display() == "chmod 0755 /home/g7/public_html/storage")
        .map(|(index, _)| index)
        .ok_or_else(|| std::io::Error::other("missing storage chmod command after copy"))?;
    let final_git_check_index = recorded
        .iter()
        .enumerate()
        .skip(app_copy_index + 1)
        .find(|(_, spec)| {
            spec.display()
                == "git --no-optional-locks -c safe.directory=/home/g7/public_html -C /home/g7/public_html status --porcelain=v1 --untracked-files=no"
        })
        .map(|(index, _)| index)
        .ok_or_else(|| std::io::Error::other("missing final deployed Git check"))?;
    assert!(app_copy_index < final_git_check_index);
    assert!(final_git_check_index < env_copy_index);
    assert!(env_copy_index < app_chown_index);
    assert!(app_chown_index < storage_chmod_index);
    assert!(storage_chmod_index < env_chmod_index);
    assert!(recorded.iter().any(|spec| {
        spec.display()
            .contains("api.github.com/repos/gnuboard/g7/releases/latest")
    }));
    assert!(recorded.iter().any(|spec| {
        spec.display()
            .contains("git clone --depth 1 --branch 7.0.2")
    }));
    assert!(
        !recorded.iter().any(|spec| {
            spec.program == "npm" || spec.display().starts_with("composer install ")
        })
    );
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
            .any(|check| { check.name == "mail-delivery" && check.status == "skipped" })
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
        check.name == "phpinfo-summary"
            && check.message.contains("FPM ini 기준")
            && check.message.contains("SAPI=FPM/FastCGI")
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
            .any(|check| { check.name == "app-official-installer" && check.status == "manual" })
    );
    assert!(
        report.app_checks.iter().any(|check| {
            check.name == "gnuboard7-deployed-git-clean" && check.status == "pass"
        })
    );
    assert!(!report.app_checks.iter().any(|check| {
        matches!(
            check.name.as_str(),
            "composer-install" | "npm-install" | "npm-build"
        )
    }));
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
    assert_eq!(report.app_url, "http://www.example.com/install/");
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
fn command_failure_message_keeps_the_end_of_long_command_output() {
    let err = Error::InstallCommandFailed {
        step: "apt-install",
        command: "apt-get install".to_string(),
        status: 100,
        stdout: "package output ".repeat(100),
        stderr: format!(
            "{}Invalid MySQL server downgrade: Cannot downgrade from 80410 to 80046",
            "debconf noise ".repeat(100)
        ),
    };

    let message = super::command_failure_message("패키지 설치 단계 실패", &err);

    assert!(message.contains("Cannot downgrade from 80410 to 80046"));
}

#[test]
fn package_failure_collects_mysql_error_log_tail()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let error_log = fs_root.join("var/log/mysql/error.log");
    fs::create_dir_all(error_log.parent().expect("mysql log parent"))?;
    fs::write(
        &error_log,
        format!(
            "{}Invalid MySQL server downgrade: Cannot downgrade from 80410 to 80046\n",
            "old mysql log line\n".repeat(100)
        ),
    )?;
    let err = Error::InstallCommandFailed {
        step: "apt-install",
        command: "apt-get install mysql-server".to_string(),
        status: 100,
        stdout: String::new(),
        stderr: "mysql.service failed".to_string(),
    };

    let enriched =
        super::attach_package_failure_diagnostics(&InstallPaths::with_root(&fs_root), err);
    let message = super::command_failure_message("패키지 설치 단계 실패", &enriched);

    assert!(message.contains("MySQL error log"));
    assert!(message.contains("Cannot downgrade from 80410 to 80046"));
    fs::remove_dir_all(fs_root)?;
    Ok(())
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
    fs::create_dir_all(fs_root.join("etc/letsencrypt/renewal"))?;
    fs::write(
        fs_root.join("etc/letsencrypt/renewal/example.com.conf"),
        "authenticator = webroot\nwebroot_path = /home/g7/public_html/public,\n",
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
    push_successful_certificate_validation_outputs(&runner, &plan);
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
    assert!(super::tls_certificate_was_reused(&checks));
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
fn preserved_certificate_renewal_webroot_must_match_current_site()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    fs::create_dir_all(fs_root.join("etc/letsencrypt/renewal"))?;
    fs::write(
        fs_root.join("etc/letsencrypt/renewal/example.com.conf"),
        "authenticator = webroot\nwebroot_path = /home/old/public_html/public,\n",
    )?;
    let paths = InstallPaths::with_root(&fs_root);
    assert!(!super::renewal_webroot_matches(
        &paths,
        "example.com",
        "/home/g7/public_html/public"
    ));
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn certificate_san_matching_does_not_accept_hostname_prefixes() {
    let output = "X509v3 Subject Alternative Name:\n DNS:example.com.evil, DNS:www.example.com";
    assert!(!super::certificate_san_contains(output, "example.com"));
    assert!(super::certificate_san_contains(output, "www.example.com"));
}

#[test]
fn apache_www_none_does_not_add_unrequested_alias()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions {
            web_server: "apache".to_string(),
            www_mode: "none".to_string(),
            ..super::plan::PlanOptions::default()
        },
    )?;
    let vhost = super::apache_vhost_content_with_socket(&plan, "/run/php/g7.sock");
    assert!(vhost.contains("ServerName example.com"));
    assert!(!vhost.contains("ServerAlias www.example.com"));
    assert!(!vhost.contains("ProxyPass /app"));
    assert!(!vhost.contains("ProxyPass /apps"));
    Ok(())
}

#[test]
fn apache_g7_websocket_routes_api_before_socket_prefix()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions {
            app_profile: "gnuboard7".to_string(),
            web_server: "apache".to_string(),
            redis_mode: "enable".to_string(),
            ..super::plan::PlanOptions::default()
        },
    )?;
    let vhost = super::apache_vhost_content_with_socket(&plan, "/run/php/g7.sock");
    let api = vhost
        .find("ProxyPass \"/apps\"")
        .expect("Reverb HTTP API proxy should exist");
    let socket = vhost
        .find("ProxyPass \"/app\"")
        .expect("Reverb WebSocket proxy should exist");
    assert!(api < socket, "/apps must be matched before the /app prefix");
    assert!(vhost.contains("ws://127.0.0.1:8080/app"));
    assert!(vhost.contains("http://127.0.0.1:8080/apps"));
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
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
            .runtime_checks
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
    assert!(!vhost.contains("location ~*"));
    assert!(!vhost.contains("location /app"));
    assert!(!vhost.contains("location /apps"));
    assert!(vhost.contains("location ~ ^/apps?(/|$)"));
    assert!(vhost.contains("proxy_pass http://127.0.0.1:8080;"));
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "app-env-created" && check.status == "pass")
    );
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

    let report = run_with_probe_and_paths("g7-test.local".to_string(), options, &probe, &paths)?;

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
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
    let paths = InstallPaths::with_root(&fs_root);

    let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;
    let commands = probe.runner().recorded();

    assert_eq!(report.web_server, "apache");
    assert_eq!(report.app_url, "https://www.example.com/install/");
    assert!(fs_root.join("etc/apache2/sites-available/g7.conf").exists());
    assert!(fs_root.join("etc/apache2/sites-enabled/g7.conf").exists());
    assert!(commands.iter().any(|command| {
        command.program == "curl"
            && command
                .args
                .contains(&OsString::from("Host: www.example.com"))
            && command
                .args
                .contains(&OsString::from("http://127.0.0.1/g7inst-ready.php"))
    }));
    let fpm_reload_index = commands
        .iter()
        .position(|command| command.display() == "systemctl reload php8.5-fpm")
        .ok_or_else(|| std::io::Error::other("missing Apache PHP-FPM reload"))?;
    let http_smoke_index = commands
        .iter()
        .position(|command| {
            command.program == "curl"
                && command
                    .args
                    .contains(&OsString::from("http://127.0.0.1/g7inst-ready.php"))
        })
        .ok_or_else(|| std::io::Error::other("missing Apache HTTP smoke"))?;
    assert!(
        fpm_reload_index < http_smoke_index,
        "Apache HTTP smoke must run after the site PHP-FPM pool is active"
    );
    let apache_vhost = fs::read_to_string(fs_root.join("etc/apache2/sites-available/g7.conf"))?;
    assert!(
        apache_vhost
            .contains("SetHandler \"proxy:unix:/run/php/php8.5-fpm-g7.sock|fcgi://localhost/\"")
    );
    assert!(!apache_vhost.contains("ProxyPass /app"));
    assert!(!apache_vhost.contains("ProxyPass /apps"));
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

    assert!(message.contains("서버 런타임 검증 실패"));
    assert!(message.contains("php-extension:redis"));
}

#[test]
fn php_candidate_failure_never_writes_active_runtime_files()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions::default(),
    )?;
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(78, "invalid pool directive"));
    let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
    let mut owned = Vec::new();

    let error = super::apply_runtime_phase(&probe, &paths, &plan, &mut owned, &[])
        .expect_err("candidate validation must fail");

    assert!(matches!(
        error,
        Error::InstallCommandFailed {
            step: "php-fpm-candidate-test",
            ..
        }
    ));
    assert!(!paths.resolve(&super::php_ini_override_path(&plan)).exists());
    assert!(!paths.resolve(&super::php_pool_path(&plan)).exists());
    assert!(
        fs::read_dir(paths.resolve(crate::installer_paths::CANDIDATE_DIR))?
            .next()
            .is_none()
    );
    assert_eq!(probe.runner().recorded().len(), 1);

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn database_candidate_failure_never_writes_active_config()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions::default(),
    )?;
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(1, "unknown variable"));
    let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
    let mut owned = Vec::new();

    let error = super::apply_database_phase(
        &probe,
        &paths,
        &plan,
        &mut owned,
        Some("database-secret"),
        None,
    )
    .expect_err("candidate validation must fail");

    assert!(matches!(
        error,
        Error::InstallCommandFailed {
            step: "database-candidate-test",
            ..
        }
    ));
    assert!(!paths.resolve(super::database_config_path(&plan)).exists());
    assert!(
        !paths
            .resolve(crate::installer_paths::MYSQL_CONFIG_CANDIDATE_PATH)
            .exists()
    );
    assert_eq!(probe.runner().recorded().len(), 1);

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn database_candidate_uses_mysql_apparmor_readable_path()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions {
            database_version: "8.0".to_string(),
            ..super::plan::PlanOptions::default()
        },
    )?;
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(1, "candidate rejected"));
    let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
    let mut owned = Vec::new();

    let _ = super::apply_database_phase(
        &probe,
        &paths,
        &plan,
        &mut owned,
        Some("database-secret"),
        None,
    );
    let command = &probe.runner().recorded()[0];
    let expected = paths
        .resolve(crate::installer_paths::MYSQL_CONFIG_CANDIDATE_PATH)
        .display()
        .to_string();

    assert!(
        command
            .args
            .contains(&OsString::from(format!("--defaults-file={expected}")))
    );
    assert!(!expected.contains("/var/lib/g7-installer/candidates/database.cnf"));

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn selected_database_version_must_match_installed_server() {
    assert!(super::verify_selected_database_version("mysqld Ver 8.4.9", "8.4").is_ok());
    assert!(super::verify_selected_database_version("8.0.46-0ubuntu0.24.04.3", "8.0").is_ok());
    assert!(super::verify_selected_database_version("mysqld Ver 8.0.46", "8.4").is_err());
}

#[test]
fn mysql_84_source_is_downloaded_verified_and_selected_noninteractively()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(format!(
        "{}  package.deb\n",
        crate::defaults::MYSQL_APT_CONFIG_SHA256
    )));
    runner.push_output(CommandOutput::success(""));
    let probe = SystemProbe::new(runner).with_fs_root(&fs_root);

    super::configure_mysql_apt_source(&probe, &paths)?;
    let commands = probe.runner().recorded();

    assert_eq!(commands[0].program, OsString::from("curl"));
    assert_eq!(commands[1].program, OsString::from("sha256sum"));
    assert_eq!(commands[2].program, OsString::from("env"));
    assert!(
        commands[2]
            .args
            .contains(&OsString::from("MYSQL_SERVER_VERSION=mysql-8.4-lts"))
    );
    assert!(
        !paths
            .resolve(crate::defaults::MYSQL_APT_CONFIG_ARCHIVE_PATH)
            .exists()
    );

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn pending_secrets_escape_quotes_without_exposing_plain_toml_syntax() {
    let db = "db\\\"secret\nnext";
    let site = "site\\\"secret";
    let smtp = "smtp\\\"secret";
    let content = super::pending_secrets_content(db, Some(site), Some(smtp));
    let parsed = content.parse::<toml::Table>().expect("valid secrets TOML");

    assert_eq!(parsed["database_password"].as_str(), Some(db));
    assert_eq!(parsed["site_password"].as_str(), Some(site));
    assert_eq!(parsed["smtp_password"].as_str(), Some(smtp));
}

#[test]
fn pending_secret_reader_round_trips_special_characters()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let db = "db\\\"secret\nnext";
    let site = "site\\\"secret";
    let target = paths.resolve(crate::installer_paths::PENDING_SECRETS_PATH);
    fs::create_dir_all(target.parent().expect("pending secret parent"))?;
    fs::write(
        &target,
        super::pending_secrets_content(db, Some(site), None),
    )?;

    assert_eq!(super::read_database_password(&paths)?.as_deref(), Some(db));
    assert_eq!(super::read_site_password(&paths)?.as_deref(), Some(site));

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn package_retry_keeps_original_preinstall_baseline()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions::default(),
    )?;
    let baseline = super::package_names(&plan)
        .into_iter()
        .map(|name| super::InstallCheck {
            name,
            status: "not-installed".to_string(),
            message: "original baseline".to_string(),
        })
        .collect::<Vec<_>>();
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::failure(100, "apt update failed"));
    let probe = SystemProbe::new(runner);

    let failure = super::apply_package_phase_with_baseline(&probe, &paths, &plan, Some(&baseline))
        .expect_err("package phase must fail");

    assert_eq!(failure.summary.preinstall_package_checks, baseline);
    assert_eq!(probe.runner().recorded().len(), 1);
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn runtime_failure_restores_then_resume_completes_from_failed_step()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let os_release_path = write_temp_os_release()?;
    let fs_root = create_temp_fs_root()?;
    prepare_webserver_test_files(&fs_root)?;
    let options = super::plan::PlanOptions::default();
    let plan = super::plan::build_with_options("example.com".to_string(), options.clone())?;
    let runner = FakeCommandRunner::default();
    runner.push_output(CommandOutput::success("0\n"));
    runner.push_output(CommandOutput::success("inactive\n"));
    runner.push_output(CommandOutput::success("inactive\n"));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    push_successful_apply_outputs_with_certbot(
        &runner,
        &plan,
        false,
        CommandOutput::success("cert issued\n"),
    );
    let probe = SystemProbe::new(FailOnceRunner::new(
        runner,
        "php-fpm8.5 -y",
        CommandOutput::failure(78, "invalid generated pool"),
    ))
    .with_os_release_path(&os_release_path)
    .with_fs_root(&fs_root);
    let paths = InstallPaths::with_root(&fs_root);

    let error = match run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths) {
        Ok(_) => return Err(std::io::Error::other("runtime candidate should fail").into()),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        Error::InstallCommandFailed {
            step: "php-fpm-candidate-test",
            ..
        }
    ));
    let failed_state = g7_state::state::read_state_file(&fs_root.join(strip_root(STATE_PATH)))?;
    assert_eq!(failed_state.current_step.as_deref(), Some("runtime"));
    assert_eq!(
        failed_state
            .steps
            .iter()
            .find(|step| step.id == "runtime")
            .and_then(|step| step.restore_status.as_deref()),
        Some("restored")
    );

    let resume_runner = FakeCommandRunner::default();
    resume_runner.push_output(CommandOutput::success("0\n"));
    resume_runner.push_output(CommandOutput::success("active\n"));
    resume_runner.push_output(CommandOutput::success("inactive\n"));
    resume_runner.push_output(CommandOutput::success(""));
    resume_runner.push_output(CommandOutput::success(""));
    push_runtime_outputs(&resume_runner, &plan);
    push_successful_vhost_outputs(&resume_runner, &plan);
    push_database_tls_outputs(
        &resume_runner,
        &plan,
        CommandOutput::success("cert issued\n"),
    );
    let resume_probe = SystemProbe::new(resume_runner)
        .with_os_release_path(&os_release_path)
        .with_fs_root(&fs_root);
    let report = match super::resume_with_probe_and_paths(&resume_probe, &paths) {
        Ok(report) => report,
        Err(error) => {
            let commands = resume_probe
                .runner()
                .recorded()
                .into_iter()
                .map(|spec| spec.display())
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(std::io::Error::other(format!(
                "resume failed: {error}; commands: {commands}"
            ))
            .into());
        }
    };

    assert_eq!(report.phase, "completed");
    let completed_state = g7_state::state::read_state_file(&fs_root.join(strip_root(STATE_PATH)))?;
    let runtime = completed_state
        .steps
        .iter()
        .find(|step| step.id == "runtime")
        .ok_or_else(|| std::io::Error::other("runtime step missing"))?;
    assert_eq!(runtime.status, "completed");
    assert_eq!(runtime.attempts, 2);
    assert!(completed_state.current_step.is_none());

    fs::remove_file(os_release_path)?;
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn redis_runtime_uses_decimal_memory_units() {
    assert_eq!(super::redis_memory_value_bytes("128M"), Some(128_000_000));
    assert_eq!(super::redis_memory_value_bytes("1G"), Some(1_000_000_000));
    assert_eq!(super::memory_value_bytes("128M"), Some(134_217_728));
}

#[test]
fn existing_certificate_defers_renewal_until_tls_webroot_is_repaired()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    fs::create_dir_all(fs_root.join("etc/letsencrypt/live/example.com"))?;
    let plan = super::plan::build_with_options(
        "example.com".to_string(),
        super::plan::PlanOptions::default(),
    )?;
    let runner = FakeCommandRunner::default();
    let probe = SystemProbe::new(runner).with_fs_root(&fs_root);
    let checks = super::verify_certbot_readiness(
        &probe,
        &plan,
        &[super::InstallCheck::pass("certbot.timer", "active")],
    );

    assert!(
        checks
            .iter()
            .any(|check| { check.name == "certbot-certificate" && check.status == "pass" })
    );
    assert!(
        checks
            .iter()
            .any(|check| { check.name == "certbot-renew-dry-run" && check.status == "deferred" })
    );
    assert!(probe.runner().recorded().is_empty());

    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn runtime_policy_keeps_upload_headroom_and_safe_opcache_refresh() {
    let sizing = super::plan::resolve_memory_sizing(2 * 1024 * 1024, 2);
    let ini = super::php_ini_override_content(&sizing);
    assert!(ini.contains("upload_max_filesize = 64M"));
    assert!(ini.contains("post_max_size = 80M"));
    assert!(ini.contains("max_input_vars = 5000"));
    assert!(ini.contains("opcache.validate_timestamps = 1"));
    assert!(ini.contains("opcache.enable_file_override = 0"));
    assert!(!ini.contains("open_basedir"));
}

#[test]
fn nginx_worker_tuning_updates_only_expected_main_directives()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let sizing = super::plan::resolve_memory_sizing(4 * 1024 * 1024, 2);
    let source = "user www-data;\nworker_processes auto;\nevents {\n    worker_connections 768;\n}\nhttp {\n    include mime.types;\n}\n";
    let tuned = super::nginx_main_runtime_content(source, &sizing)?;
    assert!(tuned.contains("worker_processes 2;"));
    assert!(tuned.contains("worker_rlimit_nofile 8192;"));
    assert!(tuned.contains("worker_connections 2048;"));
    assert!(tuned.contains("server_tokens off;"));
    assert!(tuned.contains("include mime.types;"));
    Ok(())
}

#[test]
fn nginx_http_redirect_goes_directly_to_canonical_https_host()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let plan = super::plan::build("example.com".to_string())?;
    let content = super::nginx_tls_vhost_content(&plan, "/run/php/example.sock", None);

    assert!(content.contains("return 301 https://www.example.com$request_uri;"));
    assert!(!content.contains("return 301 https://$host$request_uri;"));
    Ok(())
}

#[test]
fn fresh_nginx_default_site_is_disabled() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let enabled = fs_root.join("etc/nginx/sites-enabled/default");
    fs::create_dir_all(enabled.parent().expect("default site parent"))?;
    std::os::unix::fs::symlink("../sites-available/default", &enabled)?;

    let check = super::disable_default_nginx_site(&paths)?;

    assert_eq!(check.status, "pass");
    assert!(!enabled.exists());
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn fresh_php_default_pool_is_disabled() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fs_root = create_temp_fs_root()?;
    let paths = InstallPaths::with_root(&fs_root);
    let plan = super::plan::build("example.com".to_string())?;
    let pool = fs_root.join(format!("etc/php/{}/fpm/pool.d/www.conf", plan.php_version));
    fs::create_dir_all(pool.parent().expect("pool parent"))?;
    fs::write(&pool, "[www]\n")?;

    let check = super::disable_default_php_fpm_pool(&paths, &plan)?;

    assert_eq!(check.status, "pass");
    assert!(!pool.exists());
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn redis_effective_output_parser_requires_exact_key_value_pairs() {
    let output = "bind\n127.0.0.1\nprotected-mode\nyes\n";
    assert_eq!(
        super::redis_config_value(output, "bind").as_deref(),
        Some("127.0.0.1")
    );
    assert_eq!(
        super::redis_config_value(output, "protected-mode").as_deref(),
        Some("yes")
    );
}

#[test]
fn install_reports_smtp_relay_reachability() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let os_release_path = write_temp_os_release()?;
    let fs_root = create_temp_fs_root()?;
    let options = super::plan::PlanOptions {
        mail_mode: "smtp-relay".to_string(),
        smtp_host: Some("smtp.example.com".to_string()),
        smtp_from: Some("no-reply@example.com".to_string()),
        smtp_username: Some("smtp-user".to_string()),
        smtp_password: Some("smtp-secret-123".to_string()),
        ..super::plan::PlanOptions::default()
    };
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
        report
            .app_checks
            .iter()
            .any(|check| { check.name == "frankenphp-octane-active" && check.status == "pass" })
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
        recorded
            .iter()
            .any(|spec| { spec.display() == "composer require laravel/octane --no-interaction" })
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
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
    let paths = InstallPaths::with_root(&fs_root);

    let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

    assert_eq!(report.app_profile, "gnuboard7-octane");
    assert_eq!(report.app_url, "https://www.example.com/install/");
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| { check.name == "app-official-installer" && check.status == "manual" })
    );

    let unit = fs::read_to_string(fs_root.join("etc/systemd/system/g7-frankenphp.service"))?;
    assert!(unit.contains("Description=G7 FrankenPHP app server"));
    assert!(unit.contains("php-server --listen 127.0.0.1:7080"));
    assert!(!unit.contains("artisan octane:frankenphp"));
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "app-env-created" && check.status == "pass")
    );

    let recorded = probe.runner().recorded();
    assert!(!recorded.iter().any(|spec| {
        spec.program == "npm"
            || spec.display().starts_with("composer install ")
            || spec.display().contains("octane:install")
    }));

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
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
    assert!(
        report.app_checks.iter().any(
            |check| check.name == "app-service:laravel-queue.service" && check.status == "pass"
        )
    );

    fs::remove_file(os_release_path)?;
    fs::remove_dir_all(fs_root)?;
    Ok(())
}

#[test]
fn install_wordpress_prepares_browser_installer_and_permissions()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let os_release_path = write_temp_os_release()?;
    let fs_root = create_temp_fs_root()?;
    let options = super::plan::PlanOptions {
        app_profile: "wordpress".to_string(),
        ..super::plan::PlanOptions::default()
    };
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
    let paths = InstallPaths::with_root(&fs_root);

    let report = run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths)?;

    assert_eq!(report.app_profile, "wordpress");
    assert_eq!(
        report.app_url,
        "https://www.example.com/wp-admin/install.php"
    );
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "wordpress-archive-test" && check.status == "pass")
    );
    assert!(report.app_checks.iter().any(|check| {
        check.name == "wordpress-source-file-wp-settings-php" && check.status == "pass"
    }));
    assert!(report.app_checks.iter().any(|check| {
        check.name == "wordpress-source-file-wp-admin-install-php" && check.status == "pass"
    }));
    assert!(
        report.app_checks.iter().any(
            |check| check.name == "wordpress-deployed-dir-wp-content" && check.status == "pass"
        )
    );
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "app-writable:wp-content/uploads" && check.status == "pass")
    );
    assert!(
        report
            .app_checks
            .iter()
            .any(|check| check.name == "app-db-handoff"
                && check.status == "info"
                && check.message.contains("g7"))
    );

    let recorded = probe.runner().recorded();
    assert!(recorded.iter().any(|spec| {
        spec.display()
            == "curl -fsSL --max-time 120 -o /var/lib/g7-installer/app-source/wordpress.zip https://wordpress.org/latest.zip"
    }));
    assert!(recorded.iter().any(|spec| {
        spec.display() == "unzip -tq /var/lib/g7-installer/app-source/wordpress.zip"
    }));
    assert!(recorded.iter().any(|spec| {
        spec.display()
            == "unzip -q /var/lib/g7-installer/app-source/wordpress.zip -d /var/lib/g7-installer/app-source/wordpress-extract"
    }));
    assert!(recorded.iter().any(|spec| {
        spec.display()
            == "cp -a /var/lib/g7-installer/app-source/wordpress-extract/wordpress/. /home/g7/public_html"
    }));
    assert!(
        recorded
            .iter()
            .any(|spec| spec.display() == "chmod 0755 /home/g7/public_html/wp-content/uploads")
    );

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
        site_user_password: Some("Test-only_9x!".to_string()),
        ..super::plan::PlanOptions::default()
    };
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
    assert!(report.vhost_checks.iter().any(|check| {
        check.name == "ssh-password-auth"
            && check.status == "warn"
            && check.message.contains("SSH 키")
    }));

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
    fs::write(
        fs_root.join("etc/nginx/nginx.conf"),
        "user www-data;\nworker_processes auto;\npid /run/nginx.pid;\nevents {\n    worker_connections 768;\n}\nhttp {\n    include /etc/nginx/conf.d/*.conf;\n    include /etc/nginx/sites-enabled/*;\n}\n",
    )?;
    fs::create_dir_all(fs_root.join("etc/apache2/conf-available"))?;
    fs::create_dir_all(fs_root.join("etc/apache2/conf-enabled"))?;
    let options = super::plan::PlanOptions {
        php_version: "8.5".to_string(),
        ..super::plan::PlanOptions::default()
    };
    let install_plan = super::plan::build_with_options("example.com".to_string(), options.clone())?;
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

    let err = match run_with_probe_and_paths("example.com".to_string(), options, &probe, &paths) {
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
fn install_adds_ondrej_source_for_php_85() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let os_release_path = write_temp_os_release()?;
    let fs_root = create_temp_fs_root()?;
    let options = super::plan::PlanOptions {
        php_version: "8.5".to_string(),
        ..super::plan::PlanOptions::default()
    };
    let probe = clean_root_probe_for_options(&os_release_path, &fs_root, "example.com", &options)?;
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
    prepare_webserver_test_files(fs_root)?;
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

fn prepare_webserver_test_files(
    fs_root: &Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(fs_root.join("etc/nginx/sites-enabled"))?;
    fs::create_dir_all(fs_root.join("etc/nginx/sites-available"))?;
    fs::create_dir_all(fs_root.join("etc/nginx/conf.d"))?;
    fs::write(
        fs_root.join("etc/nginx/nginx.conf"),
        "user www-data;\nworker_processes auto;\npid /run/nginx.pid;\nevents {\n    worker_connections 768;\n}\nhttp {\n    include /etc/nginx/conf.d/*.conf;\n    include /etc/nginx/sites-enabled/*;\n}\n",
    )?;
    fs::create_dir_all(fs_root.join("etc/apache2/conf-available"))?;
    fs::create_dir_all(fs_root.join("etc/apache2/conf-enabled"))?;
    Ok(())
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
    push_successful_site_and_vhost_outputs(runner, install_plan, site_password_set, certbot_output);
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
        runner.push_output(CommandOutput::success(
            "passwordauthentication no\npubkeyauthentication yes\n",
        ));
    }
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));

    push_runtime_outputs(runner, install_plan);
    push_successful_vhost_outputs(runner, install_plan);
    push_database_tls_outputs(runner, install_plan, certbot_output);
}

fn push_successful_vhost_outputs(
    runner: &FakeCommandRunner,
    install_plan: &super::plan::InstallPlan,
) {
    if install_plan.web_server == "apache" {
        for _module in super::apache_http_modules() {
            runner.push_output(CommandOutput::success(""));
        }
    }
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
}

fn push_runtime_outputs(runner: &FakeCommandRunner, install_plan: &super::plan::InstallPlan) {
    if install_plan.web_server == "frankenphp" {
        runner.push_output(CommandOutput::success("x86_64\n"));
        runner.push_output(CommandOutput::success("downloaded\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success("active\n"));
    }
    if install_plan.web_server == "frankenphp" {
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
    } else {
        runner.push_output(CommandOutput::success(
            "configuration file test is successful\n",
        ));
        runner.push_output(CommandOutput::success(
            "configuration file test is successful\n",
        ));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
    }
    if install_plan.redis_mode == "enable" {
        for value in [
            "bind\n127.0.0.1\n",
            "protected-mode\nyes\n",
            "maxmemory\n0\n",
            "maxmemory-policy\nnoeviction\n",
        ] {
            runner.push_output(CommandOutput::success(value));
        }
        for _ in 0..6 {
            runner.push_output(CommandOutput::success("OK\n"));
        }
        let sizing = super::plan::resolve_memory_sizing(1024 * 1024, 1);
        let redis_values = [
            "bind\n127.0.0.1\n".to_string(),
            "protected-mode\nyes\n".to_string(),
            format!(
                "maxmemory\n{}\n",
                super::redis_memory_value_bytes(&sizing.redis_maxmemory).unwrap()
            ),
            "maxmemory-policy\nvolatile-lru\n".to_string(),
        ];
        for value in redis_values {
            runner.push_output(CommandOutput::success(value));
        }
    }
    if install_plan.web_server == "frankenphp" {
        runner.push_output(CommandOutput::success(successful_php_runtime_probe_output(
            install_plan,
        )));
    } else {
        runner.push_output(CommandOutput::success(successful_php_fpm_info_output(
            install_plan,
        )));
        runner.push_output(CommandOutput::success(format!(
            "extensions={}\n",
            super::required_php_extensions(install_plan).join(",")
        )));
    }
}

fn push_database_tls_outputs(
    runner: &FakeCommandRunner,
    install_plan: &super::plan::InstallPlan,
    certbot_output: CommandOutput,
) {
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(""));
    runner.push_output(CommandOutput::success(
        successful_database_effective_output(),
    ));
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
            push_successful_certificate_validation_outputs(runner, install_plan);
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("renew ok\n"));
        } else {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
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
            push_successful_certificate_validation_outputs(runner, install_plan);
            for _module in super::apache_tls_modules() {
                runner.push_output(CommandOutput::success(""));
            }
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("renew ok\n"));
        } else {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
        }
    } else if install_plan.deployment_mode == "public" && install_plan.web_server == "frankenphp" {
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(""));
        for _host in super::certificate_hosts(install_plan) {
            runner.push_output(CommandOutput::success(""));
        }
        let certbot_succeeded = certbot_output.status == 0;
        runner.push_output(certbot_output);
        if certbot_succeeded {
            push_successful_certificate_validation_outputs(runner, install_plan);
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("renew ok\n"));
        } else {
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success(""));
        }
    }
    push_successful_app_outputs(runner, install_plan);
}

fn successful_database_effective_output() -> String {
    let sizing = super::plan::resolve_memory_sizing(1024 * 1024, 1);
    format!(
        "g7_value\nversion=8.0.42\ng7_value\ninnodb_buffer_pool_size={}\ng7_value\nmax_connections={}\ng7_value\ntmp_table_size={}\ng7_value\nmax_heap_table_size={}\ng7_value\nbind_address=127.0.0.1\n",
        super::memory_value_bytes(&sizing.db_buffer_pool).unwrap(),
        sizing.db_max_connections,
        super::memory_value_bytes(&sizing.db_tmp_table_size).unwrap(),
        super::memory_value_bytes(&sizing.db_tmp_table_size).unwrap(),
    )
}

fn push_successful_certificate_validation_outputs(
    runner: &FakeCommandRunner,
    install_plan: &super::plan::InstallPlan,
) {
    runner.push_output(CommandOutput::success("Certificate will not expire\n"));
    let sans = super::certificate_hosts(install_plan)
        .iter()
        .map(|host| format!("DNS:{host}"))
        .collect::<Vec<_>>()
        .join(", ");
    runner.push_output(CommandOutput::success(format!(
        "X509v3 Subject Alternative Name:\n    {sans}\n"
    )));
    runner.push_output(CommandOutput::success("PUBLIC-KEY\n"));
    runner.push_output(CommandOutput::success("PUBLIC-KEY\n"));
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
             max_input_vars=5000\n\
             date.timezone=UTC\n\
             realpath_cache_size=4096K\n\
             realpath_cache_ttl=600\n\
             opcache.enable=1\n\
             opcache.memory_consumption={}\n\
             opcache.validate_timestamps=1\n\
             opcache.enable_file_override=0\n\
             extensions={}\n",
        install_plan.php_version,
        install_plan.php_version,
        install_plan.php_version,
        sizing.php_memory_limit,
        sizing.php_upload_limit,
        sizing.php_post_limit,
        sizing.opcache_memory.trim_end_matches('M'),
        extensions
    )
}

fn successful_php_fpm_info_output(install_plan: &super::plan::InstallPlan) -> String {
    let sizing = super::plan::resolve_memory_sizing(1024 * 1024, 1);
    format!(
        "PHP Version => {}.0\n\
         Server API => FPM/FastCGI\n\
         Loaded Configuration File => /etc/php/{}/fpm/php.ini\n\
         Scan this dir for additional .ini files => /etc/php/{}/fpm/conf.d\n\
         memory_limit => {} => {}\n\
         upload_max_filesize => {} => {}\n\
         post_max_size => {} => {}\n\
         max_execution_time => 120 => 120\n\
         max_input_vars => 5000 => 5000\n\
         date.timezone => UTC => UTC\n\
         realpath_cache_size => 4096K => 4096K\n\
         realpath_cache_ttl => 600 => 600\n\
         opcache.enable => On => On\n\
         opcache.memory_consumption => {} => {}\n\
         opcache.validate_timestamps => On => On\n\
         opcache.enable_file_override => Off => Off\n",
        install_plan.php_version,
        install_plan.php_version,
        install_plan.php_version,
        sizing.php_memory_limit,
        sizing.php_memory_limit,
        sizing.php_upload_limit,
        sizing.php_upload_limit,
        sizing.php_post_limit,
        sizing.php_post_limit,
        sizing.opcache_memory.trim_end_matches('M'),
        sizing.opcache_memory.trim_end_matches('M'),
    )
}

fn push_successful_app_outputs(
    runner: &FakeCommandRunner,
    install_plan: &super::plan::InstallPlan,
) {
    match install_plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" => {
            runner.push_output(CommandOutput::success(r#"{"tag_name":"7.0.2"}"#));
            runner.push_output(CommandOutput::success("cloned\n"));
            push_successful_git_validation_outputs(runner, super::GNUBOARD7_REQUIRED_FILES);
            runner.push_output(CommandOutput::success(""));
            push_successful_required_path_outputs(runner, super::GNUBOARD7_REQUIRED_FILES, &[]);
            push_successful_git_validation_outputs(runner, super::GNUBOARD7_REQUIRED_FILES);
            runner.push_output(CommandOutput::success(""));
            push_successful_app_permission_outputs(runner, install_plan);
            runner.push_output(CommandOutput::success(""));
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
            runner.push_output(CommandOutput::success(""));
            runner.push_output(CommandOutput::success("composer ok\n"));
            if install_plan.app_profile == "laravel-octane" {
                runner.push_output(CommandOutput::success("octane composer ok\n"));
                runner.push_output(CommandOutput::success("octane installed\n"));
            }
            runner.push_output(CommandOutput::success("npm install ok\n"));
            runner.push_output(CommandOutput::success("npm build ok\n"));
            runner.push_output(CommandOutput::success("storage linked\n"));
            runner.push_output(CommandOutput::success("migrated\n"));
            runner.push_output(CommandOutput::success("optimized\n"));
            runner.push_output(CommandOutput::success("artisan about\n"));
            runner.push_output(CommandOutput::success(""));
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
    for _writable_path in super::app_writable_paths(install_plan) {
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

struct FailOnceRunner {
    inner: FakeCommandRunner,
    needle: String,
    replacement: CommandOutput,
    failed: Cell<bool>,
}

impl FailOnceRunner {
    fn new(inner: FakeCommandRunner, needle: &str, replacement: CommandOutput) -> Self {
        Self {
            inner,
            needle: needle.to_string(),
            replacement,
            failed: Cell::new(false),
        }
    }
}

impl CommandRunner for FailOnceRunner {
    fn run(&self, spec: &CommandSpec) -> std::result::Result<CommandOutput, CommandError> {
        let output = self.inner.run(spec)?;
        if !self.failed.get() && spec.display().contains(&self.needle) {
            self.failed.set(true);
            Ok(self.replacement.clone())
        } else {
            Ok(output)
        }
    }
}

#[test]
fn resume_reconstructs_install_options_from_report() {
    let report = serde_json::json!({
        "deployment_mode": "public",
        "app_profile": "wordpress",
        "web_server": "apache",
        "php_version": "8.3",
        "php_source": "ubuntu",
        "database": "mariadb",
        "database_name": "site_db",
        "database_user": "site_user",
        "site_user": "site",
        "web_root_mode": "custom",
        "web_root": "/srv/site",
        "www_mode": "redirect-to-root",
        "redis": "disable",
        "mail_mode": "none",
        "dns_check": true,
        "security_profile": "standard",
        "ssh_policy": "audit-only"
    });

    let options = super::plan_options_from_report(&report).expect("resume options");

    assert_eq!(options.app_profile, "wordpress");
    assert_eq!(options.web_server, "apache");
    assert_eq!(options.database_engine, "mariadb");
    assert_eq!(options.custom_web_root.as_deref(), Some("/srv/site"));
    assert!(!options.local_test);
}

#[test]
fn resume_rejects_report_without_required_identity() {
    let report = serde_json::json!({"deployment_mode": "public"});

    let error = super::plan_options_from_report(&report).expect_err("missing app profile");

    assert!(matches!(error, Error::ResumeUnavailable { .. }));
}

#[test]
fn resume_completion_requires_both_step_and_app_source_pass() {
    let step = vec!["app-source-prepared".to_string()];
    let passed = vec![super::InstallCheck::pass("app-source", "ready")];
    let failed = vec![super::InstallCheck::fail("app-source", "failed")];

    assert!(super::app_is_ready(&step, &passed));
    assert!(!super::app_is_ready(&[], &passed));
    assert!(!super::app_is_ready(&step, &failed));
}
