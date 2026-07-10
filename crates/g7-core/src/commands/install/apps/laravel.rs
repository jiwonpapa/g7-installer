use super::*;

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
    let smtp_password = read_smtp_password(paths)?;
    write_existing_file(
        paths,
        &format!("{}/.env", plan.web_root),
        &laravel_env_content(
            plan,
            &db_password,
            app_url,
            laravel_runtime_kind(plan),
            smtp_password.as_deref(),
        )?,
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
    checks.push(apply_app_env_permissions(probe, plan)?);
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
