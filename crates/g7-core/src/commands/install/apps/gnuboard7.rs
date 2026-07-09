use super::*;

pub(super) fn install_gnuboard7_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    app_url: &str,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, GNUBOARD7_SOURCE_DIR)?;
    let output = probe
        .git_clone(GNUBOARD7_REPO_URL, GNUBOARD7_RELEASE_REF, GNUBOARD7_SOURCE_DIR)
        .map_err(|err| {
            command_error(
                "gnuboard7-source",
                format!(
                    "git clone --depth 1 --branch {GNUBOARD7_RELEASE_REF} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
                ),
                err,
            )
        })?;
    require_success(
        "gnuboard7-source",
        format!(
            "git clone --depth 1 --branch {GNUBOARD7_RELEASE_REF} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
        ),
        output,
    )?;
    let source_checks = verify_git_checkout(
        probe,
        "gnuboard7",
        GNUBOARD7_SOURCE_DIR,
        GNUBOARD7_REQUIRED_FILES,
    )?;

    let output = probe
        .copy_dir_contents(GNUBOARD7_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "gnuboard7-copy",
                format!("cp -a {GNUBOARD7_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "gnuboard7-copy",
        format!("cp -a {GNUBOARD7_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "gnuboard7-deployed",
        &plan.web_root,
        GNUBOARD7_REQUIRED_FILES,
        &[],
    )?;

    let db_password =
        read_database_password(paths)?.ok_or_else(|| Error::InstallVerificationFailed {
            checks: format!("database password was not found at {SECRETS_PATH}"),
        })?;
    write_existing_file(
        paths,
        &format!("{}/.env", plan.web_root),
        &laravel_env_content(plan, &db_password, app_url, gnuboard7_runtime_kind(plan))?,
    )?;

    let mut checks = vec![
        InstallCheck::pass(
            "app-source",
            format!(
                "Checked out Gnuboard7 {GNUBOARD7_RELEASE_REF} from GitHub into {}.",
                plan.web_root
            ),
        ),
        InstallCheck::pass(
            "app-env",
            format!(
                "Wrote application .env with DB name `{}` and user `{}`; password remains in {SECRETS_PATH}.",
                plan.database_name, plan.database_user
            ),
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(write_gnuboard7_driver_settings(paths, plan, owned)?);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend(configure_laravel_runtime(
        probe,
        paths,
        plan,
        owned,
        gnuboard7_runtime_kind(plan),
        LaravelRuntimeOptions::browser_installer(),
    )?);
    checks.push(InstallCheck::pass(
        "app-install-screen",
        format!("그누보드7 브라우저 설치 화면을 {app_url} 에 준비했습니다."),
    ));
    checks.push(InstallCheck::manual(
        "app-post-install",
        "브라우저 설치를 끝낸 뒤 마이그레이션, 최적화, queue/scheduler/Reverb 서비스 시작 여부를 후속 점검하세요.",
    ));

    Ok(checks)
}

pub(super) fn write_gnuboard7_driver_settings(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let settings_dir = format!("{}/storage/app/settings", plan.web_root);
    create_owned_dir_if_absent(paths, &settings_dir, owned)?;
    let path = format!("{}/{}", plan.web_root, GNUBOARD7_DRIVER_SETTINGS_PATH);
    write_tracked_file(
        paths,
        &path,
        &gnuboard7_driver_settings_content(plan)?,
        owned,
    )?;

    let (cache_driver, session_driver) = gnuboard7_runtime_drivers(plan);
    Ok(vec![InstallCheck::pass(
        "gnuboard7-driver-settings",
        format!(
            "Preseeded Gnuboard7 driver settings at {path}; cache={cache_driver}, session={session_driver}, queue=sync."
        ),
    )])
}

pub(super) fn gnuboard7_runtime_drivers(plan: &plan::InstallPlan) -> (&'static str, &'static str) {
    if plan.redis_mode == "enable" {
        ("redis", "redis")
    } else {
        ("file", "file")
    }
}

pub(super) fn gnuboard7_driver_settings_content(plan: &plan::InstallPlan) -> Result<String> {
    let (cache_driver, session_driver) = gnuboard7_runtime_drivers(plan);
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    };
    let value = serde_json::json!({
        "_meta": {
            "version": "1.0.0",
            "updated_at": format!("g7inst-{seconds}")
        },
        "storage_driver": "local",
        "s3_bucket": "",
        "s3_region": "ap-northeast-2",
        "s3_access_key": "",
        "s3_secret_key": "",
        "s3_url": "",
        "cache_driver": cache_driver,
        "redis_host": "127.0.0.1",
        "redis_port": 6379,
        "redis_password": "",
        "redis_database": 0,
        "memcached_host": "127.0.0.1",
        "memcached_port": 11211,
        "session_driver": session_driver,
        "session_lifetime": 120,
        "queue_driver": "sync",
        "log_driver": "daily",
        "log_level": "error",
        "log_days": 14,
        "websocket_enabled": false,
        "websocket_app_id": "",
        "websocket_app_key": "",
        "websocket_app_secret": "",
        "websocket_host": "localhost",
        "websocket_port": 8080,
        "websocket_scheme": "https",
        "websocket_verify_ssl": true,
        "websocket_server_host": "127.0.0.1",
        "websocket_server_port": 8080,
        "websocket_server_scheme": "http",
        "search_engine_driver": "mysql-fulltext"
    });

    let mut content = serde_json::to_string_pretty(&value).map_err(|source| {
        Error::InstallVerificationFailed {
            checks: format!("failed to render Gnuboard7 driver settings: {source}"),
        }
    })?;
    content.push('\n');
    Ok(content)
}
