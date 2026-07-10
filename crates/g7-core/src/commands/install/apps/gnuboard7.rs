use super::*;

pub(super) fn install_gnuboard7_app<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
    owned: &mut Vec<String>,
    app_url: &str,
) -> Result<Vec<InstallCheck>> {
    let release_ref = latest_gnuboard7_release(probe)?;
    remove_existing_path(paths, GNUBOARD7_SOURCE_DIR)?;
    let output = probe
        .git_clone(GNUBOARD7_REPO_URL, &release_ref, GNUBOARD7_SOURCE_DIR)
        .map_err(|err| {
            command_error(
                "gnuboard7-source",
                format!(
                    "git clone --depth 1 --branch {release_ref} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
                ),
                err,
            )
        })?;
    require_success(
        "gnuboard7-source",
        format!(
            "git clone --depth 1 --branch {release_ref} {GNUBOARD7_REPO_URL} {GNUBOARD7_SOURCE_DIR}"
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

    let mut checks = vec![
        InstallCheck::pass(
            "app-source",
            format!(
                "GitHub 공식 최신 안정 버전 Gnuboard7 {release_ref}을(를) {}에 배치했습니다.",
                plan.web_root
            ),
        ),
        InstallCheck::manual(
            "app-official-installer",
            "G7 공식 설치 절차에 따라 Composer/Vendor, .env, 관리자 계정, 확장과 마이그레이션은 브라우저 /install에서 처리합니다.",
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.extend(verify_git_checkout(
        probe,
        "gnuboard7-deployed",
        &plan.web_root,
        GNUBOARD7_REQUIRED_FILES,
    )?);
    checks.push(InstallCheck::pass(
        "app-install-screen",
        format!("그누보드7 브라우저 설치 화면을 {app_url} 에 준비했습니다."),
    ));
    checks.push(InstallCheck::manual(
        "app-post-install",
        "G7 공식 설치 마법사에서 DB 정보, 관리자 계정, Vendor 방식과 확장을 선택해 설치를 완료하세요.",
    ));

    Ok(checks)
}

fn latest_gnuboard7_release<R: CommandRunner>(probe: &SystemProbe<R>) -> Result<String> {
    let output = probe
        .fetch_text(GNUBOARD7_LATEST_RELEASE_API_URL)
        .map_err(|err| {
            command_error(
                "gnuboard7-latest-release",
                format!("curl {GNUBOARD7_LATEST_RELEASE_API_URL}"),
                err,
            )
        })?;
    require_success(
        "gnuboard7-latest-release",
        format!("curl {GNUBOARD7_LATEST_RELEASE_API_URL}"),
        output.clone(),
    )?;

    let payload: serde_json::Value = serde_json::from_str(&output.stdout).map_err(|source| {
        Error::InstallVerificationFailed {
            checks: format!("G7 최신 Release 응답을 해석하지 못했습니다: {source}"),
        }
    })?;
    let tag = payload
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| stable_release_tag(value))
        .ok_or_else(|| Error::InstallVerificationFailed {
            checks: "G7 최신 Release 응답에 유효한 안정 버전 tag_name이 없습니다.".to_string(),
        })?;
    Ok(tag.to_string())
}

fn stable_release_tag(tag: &str) -> bool {
    let normalized = tag.strip_prefix('v').unwrap_or(tag);
    let segments = normalized.split('.').collect::<Vec<_>>();
    segments.len() == 3
        && segments
            .iter()
            .all(|segment| !segment.is_empty() && segment.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::stable_release_tag;

    #[test]
    fn accepts_only_stable_semver_release_tags() {
        assert!(stable_release_tag("7.0.2"));
        assert!(stable_release_tag("v7.1.0"));
        assert!(!stable_release_tag("7.0.3-beta.1"));
        assert!(!stable_release_tag("main"));
    }
}
