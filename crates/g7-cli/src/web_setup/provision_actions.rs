use super::*;

pub(super) fn read_saved_report_json() -> std::result::Result<serde_json::Value, ApiError> {
    let content = fs::read_to_string(REPORT_PATH).map_err(|error| {
        ApiError::bad_request(format!("failed to read report: {error}"))
            .with_hint("먼저 기본 서버 구성을 실행해 리포트를 생성하세요.")
    })?;
    serde_json::from_str(&content).map_err(|error| {
        ApiError::bad_request(format!("failed to parse report: {error}"))
            .with_hint("리포트 파일이 손상되었습니다. 재설치 초기화 후 다시 진행하세요.")
    })
}

pub(super) fn run_provision_action(
    action: &str,
    report: &serde_json::Value,
) -> std::result::Result<ProvisionActionReport, ApiError> {
    let action = action.trim();
    let checks = match action {
        "webserver" => provision_webserver(report),
        "php" => provision_php(report),
        "database" => provision_database(report),
        "ssl" => provision_ssl(report),
        "mail" => provision_mail(report),
        "security" => provision_security(report),
        "app" => provision_app(report),
        _ => {
            return Err(
                ApiError::bad_request(format!("unsupported provision action: {action}")).with_hint(
                    "지원 작업은 webserver, php, database, ssl, mail, security, app 입니다.",
                ),
            );
        }
    };
    let status = if checks.iter().any(|check| check.status == "fail") {
        "fail"
    } else if checks.iter().any(|check| check.status == "manual") {
        "manual"
    } else {
        "pass"
    }
    .to_string();
    let message = match status.as_str() {
        "pass" => "읽기 전용 구성 점검이 완료되었습니다.",
        "manual" => "자동 실행 대신 후속 수동 확인이 필요합니다.",
        _ => "실패 항목을 확인하세요.",
    }
    .to_string();

    Ok(ProvisionActionReport {
        action: action.to_string(),
        status,
        message,
        checks,
    })
}

pub(super) fn provision_webserver(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let web_server = report_string(report, "web_server").unwrap_or_else(|| "nginx".to_string());
    if web_server == "apache" {
        vec![
            run_command_check("apache-configtest", "apache2ctl", &["configtest"], None),
            run_command_check(
                "apache-active",
                "systemctl",
                &["is-active", "--quiet", "apache2"],
                None,
            ),
        ]
    } else if web_server == "frankenphp" {
        vec![
            run_command_check("nginx-configtest", "nginx", &["-t"], None),
            run_command_check(
                "nginx-active",
                "systemctl",
                &["is-active", "--quiet", "nginx"],
                None,
            ),
            run_command_check(
                "frankenphp-active",
                "systemctl",
                &["is-active", "--quiet", "g7-frankenphp"],
                None,
            ),
        ]
    } else {
        vec![
            run_command_check("nginx-configtest", "nginx", &["-t"], None),
            run_command_check(
                "nginx-active",
                "systemctl",
                &["is-active", "--quiet", "nginx"],
                None,
            ),
        ]
    }
}

pub(super) fn provision_php(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    if report_string(report, "web_server").as_deref() == Some("frankenphp") {
        return vec![run_command_check(
            "frankenphp-active",
            "systemctl",
            &["is-active", "--quiet", "g7-frankenphp"],
            None,
        )];
    }
    let version = report_string(report, "php_version").unwrap_or_else(|| "8.5".to_string());
    let service = format!("php{version}-fpm");
    let binary = format!("php-fpm{version}");
    vec![
        run_command_check("php-fpm-configtest", &binary, &["-t"], None),
        run_command_check(
            "php-fpm-active",
            "systemctl",
            &["is-active", "--quiet", &service],
            None,
        ),
    ]
}

pub(super) fn provision_database(_report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let service = "mysql";
    vec![
        run_command_check(
            "database-configtest",
            "mysqld",
            &["--validate-config"],
            None,
        ),
        run_command_check(
            "database-active",
            "systemctl",
            &["is-active", "--quiet", service],
            None,
        ),
    ]
}

pub(super) fn provision_ssl(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let cert_name = report_string(report, "domain").unwrap_or_default();
    if cert_name.is_empty() {
        return vec![InstallApiCheck {
            name: "certbot-certificate".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 인증서 이름으로 사용할 도메인을 찾지 못했습니다.".to_string(),
        }];
    }

    let fullchain_path = format!("/etc/letsencrypt/live/{cert_name}/fullchain.pem");
    let privkey_path = format!("/etc/letsencrypt/live/{cert_name}/privkey.pem");
    if !path_is_file(&fullchain_path) || !path_is_file(&privkey_path) {
        return vec![
            file_check("certbot-fullchain", &fullchain_path),
            file_check("certbot-privkey", &privkey_path),
            InstallApiCheck {
                name: "certbot-rate-limit-guard".to_string(),
                status: "manual".to_string(),
                message:
                    "기존 인증서가 없어 설치 안내서 점검에서는 새 발급을 실행하지 않았습니다. 중복 발급 제한을 피하려면 기본 구성/TLS 단계를 한 번만 완료하세요."
                        .to_string(),
            },
        ];
    }

    vec![
        file_check("certbot-fullchain", &fullchain_path),
        file_check("certbot-privkey", &privkey_path),
        run_command_check(
            "certbot-certificate-list",
            "certbot",
            &["certificates", "--cert-name", &cert_name],
            None,
        ),
        run_command_check(
            "certbot-timer-active",
            "systemctl",
            &["is-active", "--quiet", "certbot.timer"],
            None,
        ),
    ]
}

pub(super) fn provision_mail(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    if report_string(report, "mail_mode").as_deref() != Some("local-postfix") {
        return vec![InstallApiCheck {
            name: "mail".to_string(),
            status: "manual".to_string(),
            message: "로컬 Postfix 모드가 아니라 구성 점검 대상이 없습니다.".to_string(),
        }];
    }

    vec![
        run_command_check("postfix-configtest", "postfix", &["check"], None),
        run_command_check(
            "postfix-active",
            "systemctl",
            &["is-active", "--quiet", "postfix"],
            None,
        ),
    ]
}

pub(super) fn provision_security(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let security_profile =
        report_string(report, "security_profile").unwrap_or_else(|| "standard".to_string());
    let ssh_policy =
        report_string(report, "ssh_policy").unwrap_or_else(|| "audit-only".to_string());
    let mut checks = vec![InstallApiCheck {
        name: "security-policy".to_string(),
        status: "manual".to_string(),
        message: format!(
            "보안 수준은 `{security_profile}`, SSH 정책은 `{ssh_policy}`입니다. SSH 차단을 피하기 위해 자동 변경 대신 점검 결과를 확인하세요."
        ),
    }];

    checks.push(run_command_check(
        "ssh-service-active",
        "systemctl",
        &["is-active", "--quiet", "ssh"],
        None,
    ));
    checks.push(InstallApiCheck {
        name: "firewall-scope".to_string(),
        status: "manual".to_string(),
        message: "UFW/fail2ban 설치와 규칙 변경은 이 설치기의 범위가 아닙니다. VPS 제공자 방화벽 또는 별도 유지보수 앱에서 관리하세요."
            .to_string(),
    });
    checks
}

pub(super) fn provision_app(report: &serde_json::Value) -> Vec<InstallApiCheck> {
    let app_profile = report_string(report, "app_profile")
        .or_else(|| report_string(report, "app_package"))
        .unwrap_or_default();
    let web_root = report_string(report, "web_root").unwrap_or_default();
    let site_user = report_string(report, "site_user").unwrap_or_default();
    if app_profile == "gnuboard7" {
        let mut checks = vec![
            file_check("app-artisan", &format!("{web_root}/artisan")),
            dir_check("app-storage", &format!("{web_root}/storage")),
            file_check(
                "g7-core-template-engine",
                &format!("{web_root}/public/build/core/template-engine.min.js"),
            ),
        ];
        checks.extend(app_permission_checks(
            &web_root,
            &site_user,
            &["storage", "bootstrap/cache"],
        ));
        let install_lock = format!("{web_root}/storage/app/g7_installed");
        checks.push(InstallApiCheck {
            name: "g7-install-lock".to_string(),
            status: if path_is_file(&install_lock) {
                "pass".to_string()
            } else {
                "manual".to_string()
            },
            message: if path_is_file(&install_lock) {
                format!("{install_lock} 파일을 확인했습니다. 브라우저 설치가 완료된 상태입니다.")
            } else {
                format!("{install_lock} 파일이 없습니다. 브라우저 /install 완료 전이면 정상이고, 완료 후라면 설치 잠금을 확인하세요.")
            },
        });
        if !site_user.is_empty() {
            checks.push(run_command_check(
                "g7-artisan-about",
                "runuser",
                &["-u", &site_user, "--", "php", "artisan", "about"],
                Some(&web_root),
            ));
        }
        checks.push(g7_ckeditor_upload_limit_check(&format!(
            "{web_root}/storage/app/plugins/sirsoft-ckeditor5/settings/setting.json"
        )));
        checks.push(InstallApiCheck {
            name: "app-browser-install".to_string(),
            status: "manual".to_string(),
            message: "그누보드7은 브라우저 /install 완료 후 /, /login, /admin 접속을 확인하세요."
                .to_string(),
        });
        return checks;
    }

    let mut checks = vec![file_check(
        "laravel-artisan",
        &format!("{web_root}/artisan"),
    )];
    checks.extend(app_permission_checks(
        &web_root,
        &site_user,
        &["storage", "bootstrap/cache"],
    ));
    checks.push(run_command_check(
        "artisan-about",
        "php",
        &["artisan", "about"],
        Some(&web_root),
    ));
    checks
}

pub(super) fn app_permission_checks(
    web_root: &str,
    site_user: &str,
    writable_paths: &[&str],
) -> Vec<InstallApiCheck> {
    if web_root.is_empty() {
        return vec![InstallApiCheck {
            name: "app-web-root".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 앱 경로를 찾지 못했습니다.".to_string(),
        }];
    }

    let mut checks = Vec::new();
    if site_user.is_empty() {
        checks.push(InstallApiCheck {
            name: "app-site-user".to_string(),
            status: "fail".to_string(),
            message: "리포트에서 사이트 계정을 찾지 못해 소유권을 적용할 수 없습니다.".to_string(),
        });
    } else {
        checks.push(run_command_check(
            "app-web-root-readable",
            "runuser",
            &["-u", site_user, "--", "test", "-r", web_root],
            None,
        ));
    }

    checks.push(run_command_check(
        "app-web-root-mode",
        "stat",
        &["-c", "%a %U:%G", web_root],
        None,
    ));

    for writable_path in writable_paths {
        let target = format!("{web_root}/{writable_path}");
        checks.push(dir_check(&format!("app-dir:{writable_path}"), &target));
        if path_is_dir(&target) && !site_user.is_empty() {
            checks.push(run_command_check(
                &format!("app-writable-test:{writable_path}"),
                "runuser",
                &["-u", site_user, "--", "test", "-w", &target],
                None,
            ));
        }
    }

    let env_path = format!("{web_root}/.env");
    checks.push(file_check("app-env", &env_path));
    if path_is_file(&env_path) {
        checks.push(run_command_check(
            "app-env-mode",
            "stat",
            &["-c", "%a %U:%G", &env_path],
            None,
        ));
    }

    checks
}

pub(super) fn g7_ckeditor_upload_limit_check(path: &str) -> InstallApiCheck {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return InstallApiCheck {
                name: "g7-ckeditor-upload-limit".to_string(),
                status: "manual".to_string(),
                message: format!(
                    "{path} 파일이 없습니다. CKEditor5 플러그인을 설치한 뒤 imageMaxSizeMb 값을 확인하세요."
                ),
            };
        }
    };

    let limit = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| value.get("imageMaxSizeMb").cloned())
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        });

    match limit {
        Some(value) if value > 2 => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "pass".to_string(),
            message: format!(
                "CKEditor5 imageMaxSizeMb={value}MB 입니다. PHP/Nginx 한도와 함께 확인하세요."
            ),
        },
        Some(value) => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "manual".to_string(),
            message: format!(
                "CKEditor5 imageMaxSizeMb={value}MB 입니다. 큰 이미지 업로드가 필요하면 플러그인 설정을 조정하세요."
            ),
        },
        None => InstallApiCheck {
            name: "g7-ckeditor-upload-limit".to_string(),
            status: "manual".to_string(),
            message: "CKEditor5 설정 파일은 있으나 imageMaxSizeMb 값을 읽지 못했습니다."
                .to_string(),
        },
    }
}

pub(super) fn file_check(name: &str, path: &str) -> InstallApiCheck {
    let exists = path_is_file(path);
    InstallApiCheck {
        name: name.to_string(),
        status: if exists { "pass" } else { "fail" }.to_string(),
        message: if exists {
            format!("{path} 파일을 확인했습니다.")
        } else {
            format!("{path} 파일이 없습니다.")
        },
    }
}

pub(super) fn dir_check(name: &str, path: &str) -> InstallApiCheck {
    let exists = path_is_dir(path);
    InstallApiCheck {
        name: name.to_string(),
        status: if exists { "pass" } else { "fail" }.to_string(),
        message: if exists {
            format!("{path} 디렉터리를 확인했습니다.")
        } else {
            format!("{path} 디렉터리가 없습니다.")
        },
    }
}

pub(super) fn path_is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

pub(super) fn path_is_dir(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

pub(super) fn run_command_check(
    name: &str,
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
) -> InstallApiCheck {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let display = std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");

    match command.output() {
        Ok(output) if output.status.success() => InstallApiCheck {
            name: name.to_string(),
            status: "pass".to_string(),
            message: format!("{display} 성공"),
        },
        Ok(output) => InstallApiCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            message: format!(
                "{display} 실패: status={} stdout={} stderr={}",
                output.status,
                trim_command_output(&output.stdout),
                trim_command_output(&output.stderr)
            ),
        },
        Err(error) => InstallApiCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            message: format!("{display} 실행 실패: {error}"),
        },
    }
}

pub(super) fn trim_command_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.chars().count() > 400 {
        let prefix = trimmed.chars().take(400).collect::<String>();
        format!("{prefix}...")
    } else {
        trimmed.to_string()
    }
}

pub(super) fn report_string(report: &serde_json::Value, key: &str) -> Option<String> {
    report
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "g7inst-provision-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn unsupported_provision_action_is_rejected() {
        let error = run_provision_action("unknown", &serde_json::json!({}))
            .expect_err("unknown provision action should fail");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            error
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("webserver")
        );
    }

    #[test]
    fn ssl_action_does_not_issue_new_certificate_when_lineage_is_missing() {
        let report = serde_json::json!({ "domain": "missing-cert.example" });
        let result = run_provision_action("ssl", &report).expect("ssl action should report checks");

        assert_eq!(result.status, "fail");
        assert!(
            result
                .checks
                .iter()
                .any(|check| check.name == "certbot-rate-limit-guard" && check.status == "manual")
        );
        assert!(
            result
                .checks
                .iter()
                .any(|check| check.name == "certbot-fullchain" && check.status == "fail")
        );
    }

    #[test]
    fn file_and_dir_checks_report_actual_paths() {
        let root = temp_path("paths");
        let file_path = root.join("file.txt");
        let dir_path = root.join("dir");
        fs::create_dir_all(&dir_path).expect("temp dir should be created");
        fs::write(&file_path, "ok").expect("temp file should be written");

        let file = file_check("sample-file", file_path.to_str().expect("utf8 temp path"));
        let dir = dir_check("sample-dir", dir_path.to_str().expect("utf8 temp path"));
        let missing = file_check(
            "missing-file",
            root.join("missing.txt").to_str().expect("utf8 temp path"),
        );

        assert_eq!(file.status, "pass");
        assert_eq!(dir.status, "pass");
        assert_eq!(missing.status, "fail");

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }

    #[test]
    fn ckeditor_upload_limit_check_classifies_default_and_larger_limits() {
        let root = temp_path("ckeditor");
        fs::create_dir_all(&root).expect("temp dir should be created");
        let setting = root.join("setting.json");

        fs::write(&setting, r#"{"imageMaxSizeMb":2}"#).expect("setting should be written");
        let default_limit = g7_ckeditor_upload_limit_check(setting.to_str().expect("utf8 path"));
        assert_eq!(default_limit.status, "manual");
        assert!(default_limit.message.contains("2MB"));

        fs::write(&setting, r#"{"imageMaxSizeMb":"16"}"#).expect("setting should be rewritten");
        let larger_limit = g7_ckeditor_upload_limit_check(setting.to_str().expect("utf8 path"));
        assert_eq!(larger_limit.status, "pass");
        assert!(larger_limit.message.contains("16MB"));

        let missing = g7_ckeditor_upload_limit_check(
            root.join("missing.json").to_str().expect("utf8 temp path"),
        );
        assert_eq!(missing.status, "manual");

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }

    #[test]
    fn report_string_only_returns_string_values() {
        let report = serde_json::json!({
            "domain": "example.com",
            "port": 443
        });

        assert_eq!(
            report_string(&report, "domain"),
            Some("example.com".to_string())
        );
        assert_eq!(report_string(&report, "port"), None);
        assert_eq!(report_string(&report, "missing"), None);
    }

    #[test]
    fn command_output_is_trimmed_to_short_log_line() {
        let long = "x".repeat(500);
        let trimmed = trim_command_output(long.as_bytes());

        assert_eq!(trimmed.chars().count(), 403);
        assert!(trimmed.ends_with("..."));
    }

    #[test]
    fn provision_status_prioritizes_fail_then_manual_then_pass() {
        let ssl = run_provision_action("ssl", &serde_json::json!({}))
            .expect("missing domain should produce a report");
        assert_eq!(ssl.status, "fail");
        assert_eq!(ssl.message, "실패 항목을 확인하세요.");

        let mail = run_provision_action("mail", &serde_json::json!({ "mail_mode": "none" }))
            .expect("disabled mail should produce a manual report");
        assert_eq!(mail.status, "manual");
        assert!(mail.message.contains("수동"));

        let success = run_command_check("true", "true", &[], None);
        assert_eq!(success.status, "pass");
        let failure = run_command_check("false", "false", &[], None);
        assert_eq!(failure.status, "fail");
        let missing = run_command_check("missing", "g7inst-command-that-does-not-exist", &[], None);
        assert_eq!(missing.status, "fail");
    }

    #[test]
    fn app_permission_checks_fail_closed_for_missing_identity_and_paths() {
        let missing_root = app_permission_checks("", "", &["storage"]);
        assert_eq!(missing_root[0].name, "app-web-root");
        assert_eq!(missing_root[0].status, "fail");

        let root = temp_path("permissions");
        fs::create_dir_all(&root).expect("temp root should be created");
        let checks = app_permission_checks(root.to_str().expect("utf8 path"), "", &["storage"]);
        assert!(checks.iter().any(|check| check.name == "app-site-user"));
        assert!(
            checks
                .iter()
                .any(|check| check.name == "app-env" && check.status == "fail")
        );
        assert!(
            checks
                .iter()
                .any(|check| check.name == "app-dir:storage" && check.status == "fail")
        );
        fs::remove_dir_all(root).expect("temp root should be removed");
    }

    #[test]
    fn ckeditor_invalid_json_is_reported_as_manual() {
        let root = temp_path("ckeditor-invalid");
        fs::create_dir_all(&root).expect("temp dir should be created");
        let setting = root.join("setting.json");
        fs::write(&setting, "not-json").expect("setting should be written");

        let check = g7_ckeditor_upload_limit_check(setting.to_str().expect("utf8 path"));
        assert_eq!(check.status, "manual");
        assert!(check.message.contains("읽지 못했습니다"));
        fs::remove_dir_all(root).expect("temp dir should be removed");
    }
}
