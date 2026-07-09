use super::*;

pub(super) trait AppInstaller {
    fn install<R: CommandRunner>(
        &self,
        probe: &SystemProbe<R>,
        paths: &InstallPaths,
        plan: &plan::InstallPlan,
        owned: &mut Vec<String>,
        app_url: &str,
    ) -> Result<Vec<InstallCheck>>;
}

pub(super) struct Gnuboard7Installer;
pub(super) struct WordpressInstaller;
pub(super) struct LaravelInstaller;
pub(super) struct PlaceholderInstaller;

impl AppInstaller for Gnuboard7Installer {
    fn install<R: CommandRunner>(
        &self,
        probe: &SystemProbe<R>,
        paths: &InstallPaths,
        plan: &plan::InstallPlan,
        owned: &mut Vec<String>,
        app_url: &str,
    ) -> Result<Vec<InstallCheck>> {
        install_gnuboard7_app(probe, paths, plan, owned, app_url)
    }
}

impl AppInstaller for WordpressInstaller {
    fn install<R: CommandRunner>(
        &self,
        probe: &SystemProbe<R>,
        paths: &InstallPaths,
        plan: &plan::InstallPlan,
        owned: &mut Vec<String>,
        _app_url: &str,
    ) -> Result<Vec<InstallCheck>> {
        install_wordpress_app(probe, paths, plan, owned)
    }
}

impl AppInstaller for LaravelInstaller {
    fn install<R: CommandRunner>(
        &self,
        probe: &SystemProbe<R>,
        paths: &InstallPaths,
        plan: &plan::InstallPlan,
        owned: &mut Vec<String>,
        app_url: &str,
    ) -> Result<Vec<InstallCheck>> {
        install_laravel_app(probe, paths, plan, owned, app_url)
    }
}

impl AppInstaller for PlaceholderInstaller {
    fn install<R: CommandRunner>(
        &self,
        probe: &SystemProbe<R>,
        paths: &InstallPaths,
        plan: &plan::InstallPlan,
        owned: &mut Vec<String>,
        _app_url: &str,
    ) -> Result<Vec<InstallCheck>> {
        let mut checks = install_placeholder_app(paths, plan, owned)?;
        checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
        Ok(checks)
    }
}

pub(super) fn apply_app_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    summary: &ApplySummary,
) -> Result<Vec<InstallCheck>> {
    fs::create_dir_all(paths.resolve(APP_SOURCE_DIR)).map_err(|source| Error::FileWriteFailed {
        path: APP_SOURCE_DIR.to_string(),
        source,
    })?;

    let app_url = app_access_url(plan, summary);
    let mut checks = match plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" => {
            Gnuboard7Installer.install(probe, paths, plan, owned, &app_url)?
        }
        "wordpress" => WordpressInstaller.install(probe, paths, plan, owned, &app_url)?,
        "laravel" | "laravel-octane" => {
            LaravelInstaller.install(probe, paths, plan, owned, &app_url)?
        }
        _ => PlaceholderInstaller.install(probe, paths, plan, owned, &app_url)?,
    };

    checks.push(InstallCheck::pass(
        "app-url",
        format!("Open {app_url} to continue or verify the selected app install."),
    ));
    Ok(checks)
}

pub(super) fn apply_app_permissions<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    ensure_app_writable_dirs(paths, plan, owned)?;
    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {}", plan.web_root);
    let output = probe
        .chown_recursive(&owner_group, &plan.web_root)
        .map_err(|err| command_error("app-web-root-owner", &command, err))?;
    require_success("app-web-root-owner", command, output)?;
    let command = format!("chmod -R 0755 {}", plan.web_root);
    let output = probe
        .chmod_recursive("0755", &plan.web_root)
        .map_err(|err| command_error("app-web-root-permissions", &command, err))?;
    require_success("app-web-root-permissions", command, output)?;
    checks.push(InstallCheck::pass(
        "app-file-permissions",
        format!(
            "Applied {} ownership and 0755 mode to {} after app placement.",
            owner_group, plan.web_root
        ),
    ));

    for writable_path in app_writable_paths(plan) {
        let target = format!("{}/{}", plan.web_root, writable_path);
        let command = format!("chmod -R 0775 {target}");
        let output = probe
            .chmod_recursive("0775", &target)
            .map_err(|err| command_error("app-writable-permissions", &command, err))?;
        require_success("app-writable-permissions", command, output)?;
        checks.push(InstallCheck::pass(
            format!("app-writable:{writable_path}"),
            format!("Set writable runtime path `{target}` to mode 0775."),
        ));
    }
    if let Some(check) = apply_app_env_permissions(probe, paths, plan)? {
        checks.push(check);
    }

    Ok(checks)
}

pub(super) fn apply_app_env_permissions<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
) -> Result<Option<InstallCheck>> {
    let env_path = format!("{}/.env", plan.web_root);
    if !paths.resolve(&env_path).exists() {
        return Ok(None);
    }

    let command = format!("chmod 0640 {env_path}");
    let output = probe
        .chmod_path("0640", &env_path)
        .map_err(|err| command_error("app-env-permissions", &command, err))?;
    require_success("app-env-permissions", command, output)?;
    Ok(Some(InstallCheck::pass(
        "app-env-permissions",
        format!("Set `{env_path}` to mode 0640 after web-root permission normalization."),
    )))
}

pub(super) fn verify_git_checkout<R: CommandRunner>(
    probe: &SystemProbe<R>,
    app_key: &str,
    source_dir: &str,
    required_files: &[&str],
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let error_step = git_verify_error_step(app_key);
    let head_output = probe.git_rev_parse_head(source_dir).map_err(|err| {
        command_error(
            error_step,
            format!("git -C {source_dir} rev-parse --verify HEAD"),
            err,
        )
    })?;
    let commit = head_output.stdout.trim().to_string();
    require_success(
        error_step,
        format!("git -C {source_dir} rev-parse --verify HEAD"),
        head_output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-head"),
        if commit.is_empty() {
            format!("{app_key} Git HEAD를 확인했습니다.")
        } else {
            format!("{app_key} Git HEAD `{commit}`를 확인했습니다.")
        },
    ));

    let output = probe.git_fsck_full(source_dir).map_err(|err| {
        command_error(error_step, format!("git -C {source_dir} fsck --full"), err)
    })?;
    require_success(
        error_step,
        format!("git -C {source_dir} fsck --full"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-fsck"),
        format!("{app_key} Git object 무결성을 확인했습니다."),
    ));

    let output = probe.git_diff_index_clean(source_dir).map_err(|err| {
        command_error(
            error_step,
            format!("git -C {source_dir} diff-index --quiet HEAD --"),
            err,
        )
    })?;
    require_success(
        error_step,
        format!("git -C {source_dir} diff-index --quiet HEAD --"),
        output,
    )?;
    checks.push(InstallCheck::pass(
        format!("{app_key}-git-clean"),
        format!("{app_key} checkout 작업트리가 HEAD와 일치합니다."),
    ));

    for required_file in required_files {
        let output = probe
            .git_ls_files_error_unmatch(source_dir, required_file)
            .map_err(|err| {
                command_error(
                    error_step,
                    format!("git -C {source_dir} ls-files --error-unmatch {required_file}"),
                    err,
                )
            })?;
        require_success(
            error_step,
            format!("git -C {source_dir} ls-files --error-unmatch {required_file}"),
            output,
        )?;
        checks.push(InstallCheck::pass(
            format!("{app_key}-git-tracked-{}", check_key(required_file)),
            format!("{app_key} Git index에서 `{required_file}` 파일을 확인했습니다."),
        ));
    }

    Ok(checks)
}

pub(super) fn verify_zip_archive<R: CommandRunner>(
    probe: &SystemProbe<R>,
    app_key: &str,
    archive_path: &str,
) -> Result<InstallCheck> {
    let error_step = archive_verify_error_step(app_key);
    let output = probe
        .unzip_test(archive_path)
        .map_err(|err| command_error(error_step, format!("unzip -tq {archive_path}"), err))?;
    require_success(error_step, format!("unzip -tq {archive_path}"), output)?;
    Ok(InstallCheck::pass(
        format!("{app_key}-archive-test"),
        format!("{app_key} zip archive 무결성을 확인했습니다."),
    ))
}

pub(super) fn verify_required_app_paths<R: CommandRunner>(
    probe: &SystemProbe<R>,
    check_prefix: &str,
    base_dir: &str,
    files: &[&str],
    dirs: &[&str],
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    let error_step = app_path_verify_error_step(check_prefix);
    for file in files {
        let target = join_unix_path(base_dir, file);
        let output = probe
            .test_file(&target)
            .map_err(|err| command_error(error_step, format!("test -f {target}"), err))?;
        require_success(error_step, format!("test -f {target}"), output)?;
        checks.push(InstallCheck::pass(
            format!("{check_prefix}-file-{}", check_key(file)),
            format!("`{target}` 파일을 확인했습니다."),
        ));
    }
    for dir in dirs {
        let target = join_unix_path(base_dir, dir);
        let output = probe
            .test_dir(&target)
            .map_err(|err| command_error(error_step, format!("test -d {target}"), err))?;
        require_success(error_step, format!("test -d {target}"), output)?;
        checks.push(InstallCheck::pass(
            format!("{check_prefix}-dir-{}", check_key(dir)),
            format!("`{target}` 디렉터리를 확인했습니다."),
        ));
    }
    Ok(checks)
}

pub(super) fn join_unix_path(base_dir: &str, relative: &str) -> String {
    format!(
        "{}/{}",
        base_dir.trim_end_matches('/'),
        relative.trim_start_matches('/')
    )
}

pub(super) fn check_key(path: &str) -> String {
    path.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

pub(super) fn git_verify_error_step(app_key: &str) -> &'static str {
    match app_key {
        "gnuboard7" => "gnuboard7-source-verify",
        "laravel" => "laravel-source-verify",
        _ => "app-source-verify",
    }
}

pub(super) fn archive_verify_error_step(app_key: &str) -> &'static str {
    match app_key {
        "wordpress" => "wordpress-archive-verify",
        _ => "app-archive-verify",
    }
}

pub(super) fn app_path_verify_error_step(check_prefix: &str) -> &'static str {
    if check_prefix.starts_with("gnuboard7") {
        "gnuboard7-path-verify"
    } else if check_prefix.starts_with("laravel") {
        "laravel-path-verify"
    } else if check_prefix.starts_with("wordpress") {
        "wordpress-path-verify"
    } else {
        "app-path-verify"
    }
}

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

pub(super) fn install_laravel_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    app_url: &str,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, LARAVEL_SOURCE_DIR)?;
    let output = probe
        .git_clone(LARAVEL_REPO_URL, LARAVEL_RELEASE_REF, LARAVEL_SOURCE_DIR)
        .map_err(|err| {
            command_error(
                "laravel-source",
                format!(
                    "git clone --depth 1 --branch {LARAVEL_RELEASE_REF} {LARAVEL_REPO_URL} {LARAVEL_SOURCE_DIR}"
                ),
                err,
            )
        })?;
    require_success(
        "laravel-source",
        format!(
            "git clone --depth 1 --branch {LARAVEL_RELEASE_REF} {LARAVEL_REPO_URL} {LARAVEL_SOURCE_DIR}"
        ),
        output,
    )?;
    let source_checks =
        verify_git_checkout(probe, "laravel", LARAVEL_SOURCE_DIR, LARAVEL_REQUIRED_FILES)?;

    let output = probe
        .copy_dir_contents(LARAVEL_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "laravel-copy",
                format!("cp -a {LARAVEL_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "laravel-copy",
        format!("cp -a {LARAVEL_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "laravel-deployed",
        &plan.web_root,
        LARAVEL_REQUIRED_FILES,
        &[],
    )?;

    let db_password =
        read_database_password(paths)?.ok_or_else(|| Error::InstallVerificationFailed {
            checks: format!("database password was not found at {SECRETS_PATH}"),
        })?;
    write_existing_file(
        paths,
        &format!("{}/.env", plan.web_root),
        &laravel_env_content(plan, &db_password, app_url, laravel_runtime_kind(plan))?,
    )?;

    let mut checks = vec![
        InstallCheck::pass(
            "app-source",
            format!(
                "Checked out Laravel skeleton {LARAVEL_RELEASE_REF} into {}.",
                plan.web_root
            ),
        ),
        InstallCheck::pass(
            "app-env",
            format!(
                "Wrote Laravel .env with DB name `{}` and user `{}`; password remains in {SECRETS_PATH}.",
                plan.database_name, plan.database_user
            ),
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend(configure_laravel_runtime(
        probe,
        paths,
        plan,
        owned,
        laravel_runtime_kind(plan),
        LaravelRuntimeOptions::full(),
    )?);
    checks.push(InstallCheck::pass(
        "app-install-screen",
        format!("Laravel should be available at {app_url}."),
    ));

    Ok(checks)
}

pub(super) fn install_wordpress_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    remove_existing_path(paths, WORDPRESS_EXTRACT_DIR)?;
    let output = probe
        .download_file(WORDPRESS_DOWNLOAD_URL, WORDPRESS_ARCHIVE_PATH)
        .map_err(|err| {
            command_error(
                "wordpress-download",
                format!("curl -fsSL -o {WORDPRESS_ARCHIVE_PATH} {WORDPRESS_DOWNLOAD_URL}"),
                err,
            )
        })?;
    require_success(
        "wordpress-download",
        format!("curl -fsSL -o {WORDPRESS_ARCHIVE_PATH} {WORDPRESS_DOWNLOAD_URL}"),
        output,
    )?;
    let archive_check = verify_zip_archive(probe, "wordpress", WORDPRESS_ARCHIVE_PATH)?;

    let output = probe
        .unzip_archive(WORDPRESS_ARCHIVE_PATH, WORDPRESS_EXTRACT_DIR)
        .map_err(|err| {
            command_error(
                "wordpress-unzip",
                format!("unzip -q {WORDPRESS_ARCHIVE_PATH} -d {WORDPRESS_EXTRACT_DIR}"),
                err,
            )
        })?;
    require_success(
        "wordpress-unzip",
        format!("unzip -q {WORDPRESS_ARCHIVE_PATH} -d {WORDPRESS_EXTRACT_DIR}"),
        output,
    )?;
    let source_checks = verify_required_app_paths(
        probe,
        "wordpress-source",
        WORDPRESS_SOURCE_DIR,
        WORDPRESS_REQUIRED_FILES,
        WORDPRESS_REQUIRED_DIRS,
    )?;

    let output = probe
        .copy_dir_contents(WORDPRESS_SOURCE_DIR, &plan.web_root)
        .map_err(|err| {
            command_error(
                "wordpress-copy",
                format!("cp -a {WORDPRESS_SOURCE_DIR}/. {}", plan.web_root),
                err,
            )
        })?;
    require_success(
        "wordpress-copy",
        format!("cp -a {WORDPRESS_SOURCE_DIR}/. {}", plan.web_root),
        output,
    )?;
    let deployed_checks = verify_required_app_paths(
        probe,
        "wordpress-deployed",
        &plan.web_root,
        WORDPRESS_REQUIRED_FILES,
        WORDPRESS_REQUIRED_DIRS,
    )?;

    let mut checks = vec![InstallCheck::pass(
        "app-source",
        format!(
            "Downloaded WordPress latest.zip and copied it into {}.",
            plan.web_root
        ),
    )];
    checks.push(archive_check);
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend([
        InstallCheck::pass(
            "app-install-screen",
            format!(
                "WordPress browser installer should be available at {}.",
                app_entry_url(plan)
            ),
        ),
        InstallCheck {
            name: "app-db-handoff".to_string(),
            status: "info".to_string(),
            message: format!(
                "Use DB `{}` and user `{}` from {SECRETS_PATH} in the WordPress install screen.",
                plan.database_name, plan.database_user
            ),
        },
    ]);
    Ok(checks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaravelRuntimeKind {
    Gnuboard7,
    Gnuboard7Octane,
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
    fn full() -> Self {
        Self {
            run_migrations: true,
            run_optimize: true,
            verify_about: true,
            write_services: true,
            enable_services: true,
        }
    }

    fn browser_installer() -> Self {
        Self {
            run_migrations: false,
            run_optimize: false,
            verify_about: false,
            write_services: true,
            enable_services: false,
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

pub(super) fn gnuboard7_runtime_kind(plan: &plan::InstallPlan) -> LaravelRuntimeKind {
    if plan.app_profile == "gnuboard7-octane" {
        LaravelRuntimeKind::Gnuboard7Octane
    } else {
        LaravelRuntimeKind::Gnuboard7
    }
}

pub(super) fn is_octane_runtime(kind: LaravelRuntimeKind) -> bool {
    matches!(
        kind,
        LaravelRuntimeKind::Gnuboard7Octane | LaravelRuntimeKind::LaravelOctane
    )
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
    kind: LaravelRuntimeKind,
) -> Vec<AppSystemdUnit> {
    let prefix = match kind {
        LaravelRuntimeKind::Gnuboard7 | LaravelRuntimeKind::Gnuboard7Octane => "g7",
        LaravelRuntimeKind::Laravel | LaravelRuntimeKind::LaravelOctane => "laravel",
    };
    let mut units = vec![
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-queue.service",
                LaravelRuntimeKind::Gnuboard7Octane => "g7-queue.service",
                LaravelRuntimeKind::Laravel | LaravelRuntimeKind::LaravelOctane => {
                    "laravel-queue.service"
                }
            },
            content: queue_service_content(plan),
            enable_now: true,
        },
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-scheduler.service",
                LaravelRuntimeKind::Gnuboard7Octane => "g7-scheduler.service",
                LaravelRuntimeKind::Laravel | LaravelRuntimeKind::LaravelOctane => {
                    "laravel-scheduler.service"
                }
            },
            content: scheduler_service_content(plan, prefix),
            enable_now: false,
        },
        AppSystemdUnit {
            name: match kind {
                LaravelRuntimeKind::Gnuboard7 => "g7-scheduler.timer",
                LaravelRuntimeKind::Gnuboard7Octane => "g7-scheduler.timer",
                LaravelRuntimeKind::Laravel | LaravelRuntimeKind::LaravelOctane => {
                    "laravel-scheduler.timer"
                }
            },
            content: scheduler_timer_content(prefix),
            enable_now: true,
        },
    ];

    if matches!(
        kind,
        LaravelRuntimeKind::Gnuboard7 | LaravelRuntimeKind::Gnuboard7Octane
    ) {
        units.push(AppSystemdUnit {
            name: "g7-reverb.service",
            content: reverb_service_content(plan),
            enable_now: true,
        });
    }

    units
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

pub(super) fn reverb_service_content(plan: &plan::InstallPlan) -> String {
    format!(
        "[Unit]\nDescription=Gnuboard7 Reverb websocket server\nAfter=network.target {}\n\n[Service]\nType=simple\nUser={}\nGroup=www-data\nWorkingDirectory={}\nExecStart=/usr/bin/php artisan reverb:start --host=127.0.0.1 --port=8080\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n",
        database_service_name(plan),
        plan.site_user,
        plan.web_root,
    )
}

pub(super) fn systemd_unit_path(unit: &str) -> String {
    format!("/etc/systemd/system/{unit}")
}

pub(super) fn app_runtime_unit_names(plan: &plan::InstallPlan) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" => &[
            "g7-queue.service",
            "g7-scheduler.service",
            "g7-scheduler.timer",
            "g7-reverb.service",
        ],
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

pub(super) fn app_writable_paths(plan: &plan::InstallPlan) -> &'static [&'static str] {
    match plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" | "laravel" | "laravel-octane" => {
            &["storage", "bootstrap/cache"]
        }
        "wordpress" => &["wp-content/uploads"],
        _ => &[],
    }
}

pub(super) fn read_database_password(paths: &InstallPaths) -> Result<Option<String>> {
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

    Ok(content.lines().find_map(|line| {
        line.strip_prefix("database_password = ")
            .map(|value| value.trim().trim_matches('"').to_string())
    }))
}

pub(super) fn laravel_env_content(
    plan: &plan::InstallPlan,
    db_password: &str,
    app_url: &str,
    kind: LaravelRuntimeKind,
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
    env.push_str(&mail_env_content(plan));
    if matches!(
        kind,
        LaravelRuntimeKind::Gnuboard7 | LaravelRuntimeKind::Gnuboard7Octane
    ) {
        let public_reverb_port = if app_url.starts_with("https://") {
            "443"
        } else {
            "80"
        };
        let public_reverb_scheme = if app_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        env.push_str(&format!(
            "\nBROADCAST_CONNECTION=reverb\nREVERB_APP_ID=g7\nREVERB_APP_KEY=g7-local\nREVERB_APP_SECRET=g7-local-secret\nREVERB_SERVER_HOST=127.0.0.1\nREVERB_SERVER_PORT=8080\nREVERB_HOST=127.0.0.1\nREVERB_PORT=8080\nREVERB_SCHEME=http\nVITE_REVERB_APP_KEY=g7-local\nVITE_REVERB_HOST={}\nVITE_REVERB_PORT={public_reverb_port}\nVITE_REVERB_SCHEME={public_reverb_scheme}\n",
            primary_http_host(plan)
        ));
    }
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

pub(super) fn mail_env_content(plan: &plan::InstallPlan) -> String {
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
            format!(
                "MAIL_MAILER=smtp\nMAIL_HOST={host}\nMAIL_PORT={port}\nMAIL_USERNAME=null\nMAIL_PASSWORD=null\nMAIL_ENCRYPTION={encryption}\nMAIL_FROM_ADDRESS=\"{from}\"\nMAIL_FROM_NAME=\"{}\"\n",
                plan.app_profile_label
            )
        }
        _ => format!(
            "MAIL_MAILER=log\nMAIL_FROM_ADDRESS=\"noreply@{}\"\nMAIL_FROM_NAME=\"{}\"\n",
            plan.domain, plan.app_profile_label
        ),
    }
}

pub(super) fn placeholder_app_content(plan: &plan::InstallPlan) -> String {
    format!(
        "<?php\nheader('Content-Type: text/html; charset=utf-8');\n?><!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><title>{label} 준비됨</title><body><h1>{label} 설치 준비됨</h1><p>도메인, PHP 런타임, DB, SSL 설정이 완료되었습니다.</p><p>앱 소스 URL을 지정한 뒤 다시 설치하거나 수동 배포를 진행하세요.</p></body></html>\n",
        label = plan.app_profile_label
    )
}

pub(super) fn remove_existing_path(paths: &InstallPaths, path: &str) -> Result<()> {
    let target = paths.resolve(path);
    let metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: path.to_string(),
                source,
            });
        }
    };

    if metadata.file_type().is_dir() {
        fs::remove_dir_all(&target).map_err(|source| Error::FileRemoveFailed {
            path: path.to_string(),
            source,
        })
    } else {
        fs::remove_file(&target).map_err(|source| Error::FileRemoveFailed {
            path: path.to_string(),
            source,
        })
    }
}
