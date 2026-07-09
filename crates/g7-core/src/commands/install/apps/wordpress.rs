use super::*;

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
