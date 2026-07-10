use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaravelRuntimeKind {
    Laravel,
    LaravelOctane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LaravelRuntimeOptions {
    run_migrations: bool,
    run_optimize: bool,
    verify_about: bool,
    write_services: bool,
    enable_services: bool,
}

impl LaravelRuntimeOptions {
    pub(super) fn full() -> Self {
        Self {
            run_migrations: true,
            run_optimize: true,
            verify_about: true,
            write_services: true,
            enable_services: true,
        }
    }
}

pub(super) struct AppSystemdUnit {
    name: &'static str,
    content: String,
    enable_now: bool,
}

pub(super) fn laravel_runtime_kind(plan: &plan::InstallPlan) -> LaravelRuntimeKind {
    if plan.app_profile == "laravel-octane" {
        LaravelRuntimeKind::LaravelOctane
    } else {
        LaravelRuntimeKind::Laravel
    }
}

pub(super) fn is_octane_runtime(kind: LaravelRuntimeKind) -> bool {
    matches!(kind, LaravelRuntimeKind::LaravelOctane)
}

pub(super) fn configure_laravel_runtime<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    kind: LaravelRuntimeKind,
    options: LaravelRuntimeOptions,
) -> Result<Vec<InstallCheck>> {
    let cwd = paths.resolve(&plan.web_root);
    let mut checks = Vec::new();

    let mut composer_output = probe.composer_install(&cwd).map_err(|err| {
        command_error(
            "composer-install",
            "composer install --no-dev --prefer-dist --optimize-autoloader --no-interaction",
            err,
        )
    })?;
    let mut composer_retried = false;
    if composer_output.status != 0 {
        composer_retried = true;
        composer_output = probe.composer_install(&cwd).map_err(|err| {
            command_error(
                "composer-install",
                "composer install --no-dev --prefer-dist --optimize-autoloader --no-interaction",
                err,
            )
        })?;
    }
    require_success(
        "composer-install",
        "composer install --no-dev --prefer-dist --optimize-autoloader --no-interaction",
        composer_output,
    )?;
    checks.push(InstallCheck::pass(
        "composer-install",
        if composer_retried {
            format!(
                "Installed PHP dependencies in {} after one retry.",
                plan.web_root
            )
        } else {
            format!("Installed PHP dependencies in {}.", plan.web_root)
        },
    ));

    if is_octane_runtime(kind) {
        let output = probe
            .composer_require(&cwd, "laravel/octane")
            .map_err(|err| {
                command_error(
                    "composer-require-octane",
                    "composer require laravel/octane --no-interaction",
                    err,
                )
            })?;
        require_success(
            "composer-require-octane",
            "composer require laravel/octane --no-interaction",
            output,
        )?;
        checks.push(InstallCheck::pass(
            "composer-require-octane",
            "Installed Laravel Octane through Composer.",
        ));

        run_artisan_step(
            probe,
            &cwd,
            "artisan-octane-install",
            ["octane:install", "--server=frankenphp", "--no-interaction"],
            &mut checks,
            "Installed Laravel Octane configuration for FrankenPHP.",
        )?;
    }

    let output = probe
        .npm_install(&cwd)
        .map_err(|err| command_error("npm-install", "npm install", err))?;
    require_success("npm-install", "npm install", output)?;
    checks.push(InstallCheck::pass(
        "npm-install",
        format!("Installed frontend dependencies in {}.", plan.web_root),
    ));

    let output = probe
        .npm_run_build(&cwd)
        .map_err(|err| command_error("npm-build", "npm run build", err))?;
    require_success("npm-build", "npm run build", output)?;
    checks.push(InstallCheck::pass(
        "npm-build",
        "Built frontend assets with npm run build.",
    ));

    run_artisan_step(
        probe,
        &cwd,
        "artisan-key-generate",
        ["key:generate", "--force"],
        &mut checks,
        "Generated Laravel APP_KEY.",
    )?;
    run_artisan_step(
        probe,
        &cwd,
        "artisan-storage-link",
        ["storage:link"],
        &mut checks,
        "Linked public storage.",
    )?;
    if options.run_migrations {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-migrate",
            ["migrate", "--force"],
            &mut checks,
            "Applied database migrations.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-migrate",
            "브라우저 설치 화면에서 앱 설치를 완료한 뒤 필요 시 `php artisan migrate --force`를 실행하세요.",
        ));
    }

    if options.run_optimize {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-optimize",
            ["optimize"],
            &mut checks,
            "Cached Laravel runtime metadata.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-optimize",
            "브라우저 설치 완료 후 `php artisan optimize`로 캐시를 갱신하세요.",
        ));
    }

    if options.verify_about {
        run_artisan_step(
            probe,
            &cwd,
            "artisan-about",
            ["about"],
            &mut checks,
            "Verified Laravel artisan runtime.",
        )?;
    } else {
        checks.push(InstallCheck::manual(
            "artisan-about",
            "브라우저 설치 완료 후 `php artisan about`으로 앱 런타임을 확인하세요.",
        ));
    }

    if options.write_services {
        let units = app_systemd_units(plan, kind);
        for unit in &units {
            let unit_path = systemd_unit_path(unit.name);
            write_new_file(paths, &unit_path, &unit.content, owned)?;
            checks.push(InstallCheck::pass(
                format!("app-service-file:{}", unit.name),
                format!("Wrote systemd unit `{unit_path}`."),
            ));
        }

        let output = probe.systemd_daemon_reload().map_err(|err| {
            command_error("systemd-daemon-reload", "systemctl daemon-reload", err)
        })?;
        require_success("systemd-daemon-reload", "systemctl daemon-reload", output)?;
        checks.push(InstallCheck::pass(
            "systemd-daemon-reload",
            "Reloaded systemd units after app service creation.",
        ));

        for unit in units
            .into_iter()
            .filter(|unit| unit.enable_now && options.enable_services)
        {
            let command = format!("systemctl enable --now {}", unit.name);
            let output = probe
                .enable_service_now(unit.name)
                .map_err(|err| command_error("app-service-enable", &command, err))?;
            require_success("app-service-enable", command, output)?;
            checks.push(InstallCheck::pass(
                format!("app-service:{}", unit.name),
                format!("Enabled and started `{}`.", unit.name),
            ));
        }

        if !options.enable_services {
            checks.push(InstallCheck::manual(
                "app-services-enable",
                "앱 브라우저 설치를 끝낸 뒤 필요한 queue/scheduler/Reverb 서비스를 `systemctl enable --now`로 시작하세요.",
            ));
        }
    }

    if is_octane_runtime(kind) {
        checks.extend(configure_laravel_octane_service(probe, paths, plan, owned)?);
    }

    Ok(checks)
}

pub(super) fn configure_laravel_octane_service<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    write_existing_file(
        paths,
        FRANKENPHP_SERVICE_PATH,
        &frankenphp_octane_service_content(plan),
    )?;
    if !owned.iter().any(|path| path == FRANKENPHP_SERVICE_PATH) {
        owned.push(FRANKENPHP_SERVICE_PATH.to_string());
    }

    let mut checks = vec![InstallCheck::pass(
        "frankenphp-octane-service-file",
        format!("Rewrote {FRANKENPHP_SERVICE_PATH} to run Laravel Octane on {FRANKENPHP_LISTEN}."),
    )];

    let output = probe.systemd_daemon_reload().map_err(|err| {
        command_error(
            "frankenphp-octane-daemon-reload",
            "systemctl daemon-reload",
            err,
        )
    })?;
    require_success(
        "frankenphp-octane-daemon-reload",
        "systemctl daemon-reload",
        output,
    )?;
    checks.push(InstallCheck::pass(
        "frankenphp-octane-daemon-reload",
        "Reloaded systemd after writing the Laravel Octane service.",
    ));

    let output = probe
        .restart_service(FRANKENPHP_SERVICE_NAME)
        .map_err(|err| {
            command_error(
                "frankenphp-octane-restart",
                format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
                err,
            )
        })?;
    require_success(
        "frankenphp-octane-restart",
        format!("systemctl restart {FRANKENPHP_SERVICE_NAME}"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        "frankenphp-octane-restart",
        format!("Restarted {FRANKENPHP_SERVICE_NAME} with Laravel Octane."),
    ));

    match probe.service_activity(FRANKENPHP_SERVICE_NAME) {
        Ok(ServiceActivity::Active) => checks.push(InstallCheck::pass(
            "frankenphp-octane-active",
            format!(
                "{} is active with Laravel Octane on {}.",
                FRANKENPHP_SERVICE_NAME, FRANKENPHP_LISTEN
            ),
        )),
        Ok(activity) => {
            return Err(Error::InstallVerificationFailed {
                checks: format!(
                    "{} Laravel Octane service is not active: {:?}",
                    FRANKENPHP_SERVICE_NAME, activity
                ),
            });
        }
        Err(err) => {
            return Err(command_error(
                "frankenphp-octane-active",
                format!("systemctl is-active {FRANKENPHP_SERVICE_NAME}"),
                err,
            ));
        }
    }

    Ok(checks)
}

pub(super) fn run_artisan_step<R: CommandRunner, const N: usize>(
    probe: &SystemProbe<R>,
    cwd: &Path,
    step: &'static str,
    args: [&'static str; N],
    checks: &mut Vec<InstallCheck>,
    message: &'static str,
) -> Result<()> {
    let command = format!("php artisan {}", args.join(" "));
    let output = probe
        .artisan(cwd, args)
        .map_err(|err| command_error(step, &command, err))?;
    require_success(step, command, output)?;
    checks.push(InstallCheck::pass(step, message));
    Ok(())
}

pub(super) fn app_systemd_units(
    plan: &plan::InstallPlan,
    _kind: LaravelRuntimeKind,
) -> Vec<AppSystemdUnit> {
    let prefix = "laravel";
    vec![
        AppSystemdUnit {
            name: "laravel-queue.service",
            content: queue_service_content(plan),
            enable_now: true,
        },
        AppSystemdUnit {
            name: "laravel-scheduler.service",
            content: scheduler_service_content(plan, prefix),
            enable_now: false,
        },
        AppSystemdUnit {
            name: "laravel-scheduler.timer",
            content: scheduler_timer_content(prefix),
            enable_now: true,
        },
    ]
}

pub(super) fn queue_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription={} queue worker\nAfter=network.target {}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan queue:work --sleep=3 --tries=3 --timeout=90\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n",
        plan.app_profile_label,
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

pub(super) fn scheduler_service_content(plan: &plan::InstallPlan, prefix: &str) -> String {
    format!(
        "[Unit]\nDescription={prefix} Laravel scheduler\nAfter=network.target {}\n\n[Service]\nType=oneshot\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan schedule:run\n",
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

pub(super) fn scheduler_timer_content(prefix: &str) -> String {
    format!(
        "[Unit]\nDescription={prefix} Laravel scheduler every minute\n\n[Timer]\nOnCalendar=*:0/1\nAccuracySec=10s\nPersistent=true\nUnit={prefix}-scheduler.service\n\n[Install]\nWantedBy=timers.target\n"
    )
}

pub(in crate::commands::install) fn systemd_unit_path(unit: &str) -> String {
    format!("/etc/systemd/system/{unit}")
}

pub(in crate::commands::install) fn app_runtime_unit_names(
    plan: &plan::InstallPlan,
) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "laravel" | "laravel-octane" => &[
            "laravel-queue.service",
            "laravel-scheduler.service",
            "laravel-scheduler.timer",
        ],
        _ => &[],
    }
}

pub(super) fn ensure_app_writable_dirs(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<()> {
    for writable_path in app_writable_paths(plan) {
        let target = format!("{}/{}", plan.web_root, writable_path);
        create_owned_dir_if_absent(paths, &target, owned)?;
    }
    Ok(())
}

pub(in crate::commands::install) fn app_writable_paths(
    plan: &plan::InstallPlan,
) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" | "laravel" | "laravel-octane" => {
            &["storage", "bootstrap/cache"]
        }
        "wordpress" => &["wp-content/uploads"],
        _ => &[],
    }
}

pub(in crate::commands::install) fn read_database_password(
    paths: &InstallPaths,
) -> Result<Option<String>> {
    read_secret_value(paths, "database_password")
}

pub(in crate::commands::install) fn read_smtp_password(
    paths: &InstallPaths,
) -> Result<Option<String>> {
    read_secret_value(paths, "smtp_password")
}

pub(super) fn read_secret_value(paths: &InstallPaths, key: &str) -> Result<Option<String>> {
    let target = paths.resolve(SECRETS_PATH);
    let content = match fs::read_to_string(&target) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: SECRETS_PATH.to_string(),
                source,
            });
        }
    };

    let prefix = format!("{key} = ");
    Ok(content.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().trim_matches('"').to_string())
    }))
}

pub(super) fn laravel_env_content(
    plan: &plan::InstallPlan,
    db_password: &str,
    app_url: &str,
    kind: LaravelRuntimeKind,
    smtp_password: Option<&str>,
) -> Result<String> {
    let app_key = random_laravel_app_key()?;
    let app_base_url = app_base_url_from_access_url(plan, app_url);
    let redis_enabled = plan.redis_mode == "enable";
    let db_host = if plan.web_server == "frankenphp" {
        "127.0.0.1"
    } else {
        "localhost"
    };
    let escaped_db_password = db_password.replace('"', "\\\"");
    let mut env = format!(
        "APP_NAME=\"{}\"\nAPP_ENV=production\nAPP_KEY=base64:{app_key}\nAPP_DEBUG=false\nAPP_URL={app_base_url}\n\nDB_CONNECTION=mysql\nDB_HOST={db_host}\nDB_PORT=3306\nDB_DATABASE={}\nDB_USERNAME={}\nDB_PASSWORD=\"{escaped_db_password}\"\nDB_READ_HOST={db_host}\nDB_READ_PORT=3306\nDB_READ_DATABASE={}\nDB_READ_USERNAME={}\nDB_READ_PASSWORD=\"{escaped_db_password}\"\nDB_WRITE_HOST={db_host}\nDB_WRITE_PORT=3306\nDB_WRITE_DATABASE={}\nDB_WRITE_USERNAME={}\nDB_WRITE_PASSWORD=\"{escaped_db_password}\"\n\nCACHE_STORE={}\nCACHE_DRIVER={}\nSESSION_DRIVER={}\nQUEUE_CONNECTION={}\nREDIS_CLIENT=phpredis\nREDIS_HOST=127.0.0.1\nREDIS_PORT=6379\nREDIS_PASSWORD=null\n\n",
        plan.app_profile_label,
        plan.database_name,
        plan.database_user,
        plan.database_name,
        plan.database_user,
        plan.database_name,
        plan.database_user,
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "file" },
        if redis_enabled { "redis" } else { "database" },
    );
    env.push_str(&mail_env_content(plan, smtp_password));
    if is_octane_runtime(kind) {
        let octane_https = if app_url.starts_with("https://") {
            "true"
        } else {
            "false"
        };
        env.push_str(&format!(
            "\nOCTANE_SERVER=frankenphp\nOCTANE_HTTPS={octane_https}\n"
        ));
    }
    Ok(env)
}

pub(super) fn mail_env_content(plan: &plan::InstallPlan, smtp_password: Option<&str>) -> String {
    match plan.mail_mode.as_str() {
        "local-postfix" => {
            let from = plan
                .smtp_from
                .clone()
                .unwrap_or_else(|| format!("noreply@{}", plan.domain));
            format!(
                "MAIL_MAILER=smtp\nMAIL_HOST=127.0.0.1\nMAIL_PORT=25\nMAIL_USERNAME=null\nMAIL_PASSWORD=null\nMAIL_ENCRYPTION=null\nMAIL_FROM_ADDRESS=\"{from}\"\nMAIL_FROM_NAME=\"{}\"\n",
                plan.app_profile_label
            )
        }
        "smtp-relay" => {
            let host = plan.smtp_host.clone().unwrap_or_default();
            let port = plan.smtp_port.unwrap_or(587);
            let encryption = plan
                .smtp_encryption
                .clone()
                .unwrap_or_else(|| "tls".to_string());
            let from = plan
                .smtp_from
                .clone()
                .unwrap_or_else(|| format!("noreply@{}", plan.domain));
            let username = plan.smtp_username.clone().unwrap_or_default();
            let password = smtp_password.unwrap_or_default().replace('"', "\\\"");
            format!(
                "MAIL_MAILER=smtp\nMAIL_HOST={host}\nMAIL_PORT={port}\nMAIL_USERNAME=\"{username}\"\nMAIL_PASSWORD=\"{password}\"\nMAIL_ENCRYPTION={encryption}\nMAIL_FROM_ADDRESS=\"{from}\"\nMAIL_FROM_NAME=\"{}\"\n",
                plan.app_profile_label
            )
        }
        _ => format!(
            "MAIL_MAILER=log\nMAIL_FROM_ADDRESS=\"noreply@{}\"\nMAIL_FROM_NAME=\"{}\"\n",
            plan.domain, plan.app_profile_label
        ),
    }
}
