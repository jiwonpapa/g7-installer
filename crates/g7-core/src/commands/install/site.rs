use super::*;

pub(super) fn apply_site_phase<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    site_user_password: Option<&str>,
) -> Result<Vec<InstallCheck>> {
    let mut checks = Vec::new();
    ensure_supported_web_root(plan)?;

    let user_exists = probe.user_exists(&plan.site_user).map_err(|err| {
        command_error("site-user-check", format!("id -u {}", plan.site_user), err)
    })?;
    if user_exists {
        checks.push(InstallCheck::pass(
            "site-user",
            format!("Linux account `{}` already exists.", plan.site_user),
        ));
    } else {
        let command = format!("useradd --create-home --shell /bin/bash {}", plan.site_user);
        let output = probe
            .create_login_user(&plan.site_user)
            .map_err(|err| command_error("site-user-create", &command, err))?;
        require_success("site-user-create", command, output)?;
        checks.push(InstallCheck::pass(
            "site-user",
            format!("Linux account `{}` was created.", plan.site_user),
        ));
    }

    if let Some(password) = site_user_password {
        let output = probe
            .set_login_password(&plan.site_user, password)
            .map_err(|err| command_error("site-user-password", "chpasswd", err))?;
        require_success("site-user-password", "chpasswd", output)?;
        checks.push(InstallCheck::pass(
            "site-user-password",
            format!(
                "Password was set for Linux account `{}` for SFTP/login use.",
                plan.site_user
            ),
        ));
    }

    let ready_path = ready_probe_path(plan);
    if !owned.iter().any(|path| path == &ready_path) {
        require_empty_or_absent_dir(paths, &plan.web_root)?;
        require_empty_or_absent_dir(paths, &plan.app_document_root)?;
    }
    create_owned_dir_if_absent(paths, &plan.web_root, owned)?;
    create_owned_dir_if_absent(paths, &plan.app_document_root, owned)?;
    checks.push(InstallCheck::pass(
        "web-root",
        format!("Created or verified {}.", plan.app_document_root),
    ));

    write_owned_file(paths, &ready_path, ready_probe_content(), owned)?;
    checks.push(InstallCheck::pass(
        "php-ready-probe",
        format!("Wrote temporary PHP smoke file {}.", ready_path),
    ));

    let owner_group = format!("{}:www-data", plan.site_user);
    let command = format!("chown -R {owner_group} {}", plan.web_root);
    let output = probe
        .chown_recursive(&owner_group, &plan.web_root)
        .map_err(|err| command_error("web-root-owner", &command, err))?;
    require_success("web-root-owner", command, output)?;
    let command = format!("chmod 0755 {}", plan.web_root);
    let output = probe
        .chmod_path("0755", &plan.web_root)
        .map_err(|err| command_error("web-root-permissions", &command, err))?;
    require_success("web-root-permissions", command, output)?;
    let site_home = site_home_path(plan);
    let command = format!("chmod 0711 {site_home}");
    let output = probe
        .chmod_path("0711", &site_home)
        .map_err(|err| command_error("site-home-traverse", &command, err))?;
    require_success("site-home-traverse", command, output)?;
    checks.push(InstallCheck::pass(
        "web-root-permissions",
        format!(
            "Set {} owner to {} and mode 0755; set {} to 0711 so the web server can traverse without listing the home directory.",
            plan.web_root, owner_group, site_home
        ),
    ));

    Ok(checks)
}

pub(super) fn install_placeholder_app(
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
) -> Result<Vec<InstallCheck>> {
    let index_path = format!("{}/index.php", plan.app_document_root);
    write_owned_file(paths, &index_path, &placeholder_app_content(plan), owned)?;
    Ok(vec![
        InstallCheck {
            name: "app-source".to_string(),
            status: "deferred".to_string(),
            message: format!(
                "{} source URL is not selected yet; wrote a temporary handoff page at {index_path}.",
                plan.app_profile_label
            ),
        },
        InstallCheck::pass(
            "app-install-screen",
            format!(
                "Temporary app handoff page is available at {}.",
                app_entry_url(plan)
            ),
        ),
    ])
}

pub(super) fn package_names(plan: &plan::InstallPlan) -> Vec<String> {
    plan.packages
        .iter()
        .flat_map(|package| package.name.split_whitespace())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn ensure_supported_web_root(plan: &plan::InstallPlan) -> Result<()> {
    let web_root = Path::new(&plan.web_root);
    let app_root = Path::new(&plan.app_document_root);
    if !web_root.is_absolute()
        || web_root == Path::new("/")
        || !app_root.starts_with(web_root)
        || !reset_safe_web_root(&plan.web_root)
    {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "web root is outside the current reset safety policy: {}",
                plan.web_root
            ),
        });
    }
    Ok(())
}

pub(super) fn reset_safe_web_root(path: &str) -> bool {
    let parts = Path::new(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if parts.len() == 4 && parts[1] == "home" && (parts[3] == "public_html" || parts[3] == "www") {
        return valid_path_segment(&parts[2]);
    }

    parts.len() == 4 && parts[1] == "var" && parts[2] == "www" && valid_path_segment(&parts[3])
}

pub(super) fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
}

pub(super) fn require_empty_or_absent_dir(paths: &InstallPaths, path: &str) -> Result<()> {
    let target = paths.resolve(path);
    let metadata = match fs::metadata(&target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::FileReadFailed {
                path: path.to_string(),
                source,
            });
        }
    };

    if !metadata.is_dir() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{path} exists but is not a directory"),
        });
    }

    let mut entries = fs::read_dir(&target).map_err(|source| Error::FileReadFailed {
        path: path.to_string(),
        source,
    })?;
    if entries.next().is_some() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{path} exists but is not empty"),
        });
    }

    Ok(())
}

pub(super) fn ready_probe_path(plan: &plan::InstallPlan) -> String {
    format!("{}/{}", plan.app_document_root, PHP_READY_FILENAME)
}

pub(super) fn site_home_path(plan: &plan::InstallPlan) -> String {
    format!("/home/{}", plan.site_user)
}

pub(super) fn ready_probe_content() -> &'static str {
    "<?php\nheader('Content-Type: text/plain; charset=utf-8');\necho \"G7inst vhost ready\\n\";\n"
}

pub(super) fn certbot_http01_challenge_dir(plan: &plan::InstallPlan) -> String {
    format!(
        "{}/{}",
        plan.app_document_root, CERTBOT_HTTP01_CHALLENGE_DIR
    )
}

pub(super) fn certbot_http01_smoke_path(plan: &plan::InstallPlan) -> String {
    format!(
        "{}/{}",
        certbot_http01_challenge_dir(plan),
        CERTBOT_HTTP01_SMOKE_FILENAME
    )
}

pub(super) fn certbot_http01_smoke_uri() -> String {
    format!("/{CERTBOT_HTTP01_CHALLENGE_DIR}/{CERTBOT_HTTP01_SMOKE_FILENAME}")
}

pub(super) fn certificate_files_exist(paths: &InstallPaths, cert_name: &str) -> bool {
    let cert_dir = format!("/etc/letsencrypt/live/{cert_name}");
    paths.resolve(&format!("{cert_dir}/fullchain.pem")).exists()
        && paths.resolve(&format!("{cert_dir}/privkey.pem")).exists()
}

pub(super) fn app_entry_url(plan: &plan::InstallPlan) -> String {
    format!("http://{}{}", primary_http_host(plan), app_entry_path(plan))
}

pub(super) fn app_access_url(plan: &plan::InstallPlan, summary: &ApplySummary) -> String {
    let scheme = if summary
        .certbot_checks
        .iter()
        .any(|check| check.name == "tls-certificate" && check.status == "pass")
    {
        "https"
    } else {
        "http"
    };
    format!(
        "{scheme}://{}{}",
        primary_http_host(plan),
        app_entry_path(plan)
    )
}

pub(super) fn app_base_url_from_access_url(plan: &plan::InstallPlan, app_url: &str) -> String {
    let scheme = if app_url.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{}", primary_http_host(plan))
}

pub(super) fn app_entry_path(plan: &plan::InstallPlan) -> &'static str {
    match plan.app_profile.as_str() {
        "gnuboard7" | "gnuboard7-octane" => "/install",
        "wordpress" => "/wp-admin/install.php",
        _ => "/",
    }
}

pub(super) fn create_owned_dir(
    paths: &InstallPaths,
    path: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    fs::create_dir_all(&target).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })?;
    if !owned.iter().any(|owned_path| owned_path == path) {
        owned.push(path.to_string());
    }
    Ok(())
}

pub(super) fn create_owned_dir_if_absent(
    paths: &InstallPaths,
    path: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    let existed = target.exists();
    fs::create_dir_all(&target).map_err(|source| Error::FileWriteFailed {
        path: path.to_string(),
        source,
    })?;
    if !existed {
        owned.push(path.to_string());
    }
    Ok(())
}

pub(super) fn create_owned_symlink(
    paths: &InstallPaths,
    source: &str,
    link: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let source_path = paths.resolve(source);
    let link_path = paths.resolve(link);
    if let Ok(metadata) = fs::symlink_metadata(&link_path) {
        if !owned.iter().any(|owned_path| owned_path == link) {
            return Err(Error::InstallVerificationFailed {
                checks: format!("{link} already exists and is not installer-owned"),
            });
        }
        if metadata.file_type().is_symlink()
            && fs::read_link(&link_path).ok().as_ref() == Some(&source_path)
        {
            return Ok(());
        }
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(&link_path).map_err(|source| Error::FileWriteFailed {
                path: link.to_string(),
                source,
            })?;
        } else {
            return Err(Error::InstallVerificationFailed {
                checks: format!("{link} exists but is not a replaceable file or symlink"),
            });
        }
    }
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    #[cfg(unix)]
    {
        unix_fs::symlink(&source_path, &link_path).map_err(|source| Error::FileWriteFailed {
            path: link.to_string(),
            source,
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = source_path;
        return Err(Error::InstallVerificationFailed {
            checks: "symlink creation is supported only on unix platforms".to_string(),
        });
    }
    if !owned.iter().any(|owned_path| owned_path == link) {
        owned.push(link.to_string());
    }
    Ok(())
}

pub(super) fn write_new_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    if target.exists() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{path} already exists; refusing to replace an untracked file"),
        });
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    g7_state::atomic::atomic_write(&target, content.as_bytes()).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    #[cfg(unix)]
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    owned.push(path.to_string());
    Ok(())
}

pub(super) fn write_owned_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    if paths.resolve(path).exists() {
        if !owned.iter().any(|owned_path| owned_path == path) {
            return Err(Error::InstallVerificationFailed {
                checks: format!("{path} already exists and is not installer-owned"),
            });
        }
        write_existing_file(paths, path, content)
    } else {
        write_new_file(paths, path, content, owned)
    }
}

pub(super) fn write_managed_marker_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    marker: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    let target = paths.resolve(path);
    if target.exists() {
        let existing = fs::read_to_string(&target).map_err(|source| Error::FileWriteFailed {
            path: path.to_string(),
            source,
        })?;
        if !(existing.contains(marker)
            || path == SWAP_SYSCTL_PATH && is_legacy_g7_swap_sysctl(&existing))
        {
            return Err(Error::InstallVerificationFailed {
                checks: format!("{path} already exists and is not marked as g7inst-managed"),
            });
        }
        write_existing_file(paths, path, content)?;
        if !owned.iter().any(|owned_path| owned_path == path) {
            owned.push(path.to_string());
        }
        return Ok(());
    }

    write_new_file(paths, path, content, owned)
}

pub(super) fn is_legacy_g7_swap_sysctl(content: &str) -> bool {
    let normalized = content
        .lines()
        .map(|line| line.split_whitespace().collect::<String>())
        .collect::<Vec<_>>();

    normalized == ["vm.swappiness=10", "vm.vfs_cache_pressure=50"]
}

pub(super) fn write_tracked_file(
    paths: &InstallPaths,
    path: &str,
    content: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    write_existing_file(paths, path, content)?;
    if !owned.iter().any(|owned_path| owned_path == path) {
        owned.push(path.to_string());
    }
    Ok(())
}

pub(super) fn write_existing_file(paths: &InstallPaths, path: &str, content: &str) -> Result<()> {
    let target = paths.resolve(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    #[cfg(unix)]
    let mode = fs::metadata(&target)
        .ok()
        .map(|metadata| metadata.permissions().mode())
        .unwrap_or(0o644);
    g7_state::atomic::atomic_write(&target, content.as_bytes()).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    #[cfg(unix)]
    fs::set_permissions(&target, fs::Permissions::from_mode(mode)).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    Ok(())
}

pub(super) fn write_validation_candidate(
    paths: &InstallPaths,
    path: &str,
    content: &str,
) -> Result<PathBuf> {
    let target = paths.resolve(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::FileWriteFailed {
            path: parent.display().to_string(),
            source,
        })?;
    }
    g7_state::atomic::atomic_write(&target, content.as_bytes()).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    #[cfg(unix)]
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).map_err(|source| {
        Error::FileWriteFailed {
            path: path.to_string(),
            source,
        }
    })?;
    Ok(target)
}

pub(super) fn remove_validation_candidates(
    paths: &InstallPaths,
    candidates: &[&str],
) -> Result<()> {
    for candidate in candidates {
        match fs::remove_file(paths.resolve(candidate)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::FileWriteFailed {
                    path: (*candidate).to_string(),
                    source,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn remove_owned_file(
    paths: &InstallPaths,
    path: &str,
    owned: &mut Vec<String>,
) -> Result<()> {
    match fs::remove_file(paths.resolve(path)) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(Error::FileRemoveFailed {
                path: path.to_string(),
                source,
            });
        }
    }
    owned.retain(|owned_path| owned_path != path);
    Ok(())
}
