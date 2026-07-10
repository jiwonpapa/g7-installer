use super::*;

mod gnuboard7;
mod laravel;
mod runtime;
mod wordpress;

use gnuboard7::*;
use laravel::*;
use runtime::*;
pub(super) use runtime::{
    app_runtime_unit_names, app_writable_paths, read_database_password, systemd_unit_path,
};
use wordpress::*;

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
    checks.push(InstallCheck::pass(
        "app-file-permissions",
        format!(
            "Applied {} ownership to {} while preserving upstream file modes.",
            owner_group, plan.web_root
        ),
    ));

    for writable_path in app_writable_paths(plan) {
        let target = format!("{}/{}", plan.web_root, writable_path);
        let command = format!("chmod 0755 {target}");
        let output = probe
            .chmod_path("0755", &target)
            .map_err(|err| command_error("app-writable-permissions", &command, err))?;
        require_success("app-writable-permissions", command, output)?;
        checks.push(InstallCheck::pass(
            format!("app-writable:{writable_path}"),
            format!("Set runtime directory `{target}` to owner-writable mode 0755."),
        ));
    }
    Ok(checks)
}

pub(super) fn apply_app_env_permissions<R: CommandRunner>(
    probe: &SystemProbe<R>,
    plan: &plan::InstallPlan,
) -> Result<InstallCheck> {
    let env_path = format!("{}/.env", plan.web_root);
    let command = format!("chmod 0600 {env_path}");
    let output = probe
        .chmod_path("0600", &env_path)
        .map_err(|err| command_error("app-env-permissions", &command, err))?;
    require_success("app-env-permissions", command, output)?;
    Ok(InstallCheck::pass(
        "app-env-permissions",
        format!(
            "Set `{env_path}` to owner-only mode 0600 after web-root permission normalization."
        ),
    ))
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

    let status_command = format!(
        "git --no-optional-locks -C {source_dir} status --porcelain=v1 --untracked-files=no"
    );
    let output = probe
        .git_tracked_files_status(source_dir)
        .map_err(|err| command_error(error_step, &status_command, err))?;
    let tracked_changes = short_text(&output.stdout);
    require_success(error_step, status_command, output)?;
    if !tracked_changes.is_empty() {
        return Err(Error::InstallVerificationFailed {
            checks: format!("{app_key} checkout의 추적 파일이 HEAD와 다릅니다: {tracked_changes}"),
        });
    }
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
    if app_key.starts_with("gnuboard7") {
        "gnuboard7-source-verify"
    } else if app_key.starts_with("laravel") {
        "laravel-source-verify"
    } else {
        "app-source-verify"
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

#[cfg(test)]
mod git_checkout_tests {
    use super::*;
    use g7_system::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn tracked_file_changes_fail_checkout_verification() {
        let runner = FakeCommandRunner::default();
        runner.push_output(CommandOutput::success("deadbeef\n"));
        runner.push_output(CommandOutput::success(""));
        runner.push_output(CommandOutput::success(" M README.md\n"));
        let probe = SystemProbe::new(runner);

        let error = verify_git_checkout(&probe, "gnuboard7", "/tmp/g7", &[])
            .expect_err("tracked file changes must fail verification");

        assert!(matches!(
            error,
            Error::InstallVerificationFailed { checks }
                if checks.contains("M README.md")
        ));
    }
}
