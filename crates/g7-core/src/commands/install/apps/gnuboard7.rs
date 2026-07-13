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
    let mut source_checks = verify_git_checkout(
        probe,
        "gnuboard7",
        GNUBOARD7_SOURCE_DIR,
        GNUBOARD7_REQUIRED_FILES,
    )?;
    source_checks.push(verify_gnuboard7_release_assets(
        paths,
        GNUBOARD7_SOURCE_DIR,
        &release_ref,
    )?);

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
            "G7 공식 설치 절차에 따라 Composer/Vendor, 관리자 계정, 확장과 마이그레이션은 브라우저 /install에서 처리합니다.",
        ),
    ];
    checks.extend(source_checks);
    checks.extend(deployed_checks);
    checks.extend(verify_git_checkout(
        probe,
        "gnuboard7-deployed",
        &plan.web_root,
        GNUBOARD7_REQUIRED_FILES,
    )?);
    checks.push(prepare_gnuboard7_env(probe, paths, plan)?);
    checks.extend(apply_app_permissions(probe, paths, plan, owned)?);
    checks.push(apply_app_env_permissions(probe, plan)?);
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

fn prepare_gnuboard7_env<R: CommandRunner>(
    probe: &SystemProbe<R>,
    paths: &InstallPaths,
    plan: &plan::InstallPlan,
) -> Result<InstallCheck> {
    let source_path = format!("{}/.env.example", plan.web_root);
    let env_path = format!("{}/.env", plan.web_root);
    if paths.resolve(&env_path).exists() {
        return Ok(InstallCheck::pass(
            "app-env-preserved",
            format!("기존 `{env_path}` 파일을 덮어쓰지 않고 보존했습니다."),
        ));
    }

    let command = format!("cp -- {source_path} {env_path}");
    let output = probe
        .copy_file(&source_path, &env_path)
        .map_err(|err| command_error("gnuboard7-env", &command, err))?;
    require_success("gnuboard7-env", command, output)?;
    Ok(InstallCheck::pass(
        "app-env-created",
        format!("G7 설치 준비를 위해 `{source_path}`에서 `{env_path}`를 생성했습니다."),
    ))
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

fn verify_gnuboard7_release_assets(
    paths: &InstallPaths,
    source_dir: &str,
    release_ref: &str,
) -> Result<InstallCheck> {
    let build_dir = paths.resolve(&format!("{source_dir}/public/build"));
    #[cfg(test)]
    if !build_dir.join("manifest.json").is_file() {
        // Fake command-runner tests do not materialize a checkout. Focused
        // manifest tests below cover the integrity decision itself.
        return Ok(InstallCheck::pass(
            "gnuboard7-vite-manifest",
            "fake checkout manifest validation handled by focused tests",
        ));
    }
    let audit =
        crate::vite_manifest::audit_vite_manifest(&build_dir.join("manifest.json"), &build_dir)?;
    if audit.referenced.is_empty() {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "G7 {release_ref} public/build/manifest.json에 배포 자산 참조가 없습니다. 공식 릴리스를 확인하세요."
            ),
        });
    }
    if !audit.missing.is_empty() {
        return Err(Error::InstallVerificationFailed {
            checks: format!(
                "G7 공식 릴리스 {release_ref}의 manifest 참조 파일이 누락됐습니다: {}. 설치기가 파일을 빌드하거나 수정하지 않습니다.",
                audit.missing.join(", ")
            ),
        });
    }
    Ok(InstallCheck::pass(
        "gnuboard7-vite-manifest",
        format!(
            "G7 {release_ref} Vite manifest의 배포 자산 {}개를 확인했습니다.",
            audit.referenced.len()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use g7_system::command::FakeCommandRunner;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_only_stable_semver_release_tags() {
        assert!(stable_release_tag("7.0.2"));
        assert!(stable_release_tag("v7.1.0"));
        assert!(!stable_release_tag("7.0.3-beta.1"));
        assert!(!stable_release_tag("main"));
    }

    #[test]
    fn existing_env_is_preserved_without_copying()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("g7-env-preserve-{suffix}"));
        let env_path = root.join("home/g7/public_html/.env");
        fs::create_dir_all(env_path.parent().expect("env parent"))?;
        fs::write(&env_path, "APP_KEY=keep-me\n")?;

        let plan =
            plan::build_with_options("example.com".to_string(), plan::PlanOptions::default())?;
        let probe = SystemProbe::new(FakeCommandRunner::default());
        let paths = InstallPaths::with_root(&root);

        let check = prepare_gnuboard7_env(&probe, &paths, &plan)?;

        assert_eq!(check.name, "app-env-preserved");
        assert_eq!(fs::read_to_string(&env_path)?, "APP_KEY=keep-me\n");
        assert!(probe.runner().recorded().is_empty());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn broken_release_manifest_is_rejected_before_deployment()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!("g7-release-assets-{suffix}"));
        let build = root.join("var/lib/g7-installer/app-source/gnuboard7/public/build");
        fs::create_dir_all(&build)?;
        fs::write(
            build.join("manifest.json"),
            r#"{"app":{"file":"assets/missing.js"}}"#,
        )?;

        let error = verify_gnuboard7_release_assets(
            &InstallPaths::with_root(&root),
            GNUBOARD7_SOURCE_DIR,
            "7.0.3",
        )
        .expect_err("broken upstream release must be rejected");

        assert!(error.to_string().contains("assets/missing.js"));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
