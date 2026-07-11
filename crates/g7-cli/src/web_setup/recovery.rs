use super::*;

struct ResumeRecovery {
    can_resume: bool,
    can_retry_step: bool,
    failed_step: Option<String>,
    restore_status: Option<String>,
    resume_reason: Option<String>,
}

#[derive(Default)]
struct G7InstallEvidence {
    database_created: bool,
    install_completed: bool,
    install_lock_path: Option<String>,
}

pub(super) fn recovery_status() -> RecoveryApiStatus {
    let metadata_paths = installer_metadata_paths()
        .into_iter()
        .filter(|path| fs::metadata(path).is_ok())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let rollback_check = rollback::run(true, true);
    let (can_rollback, rollback_reason) = match rollback_check {
        Ok(_) => (true, None),
        Err(error) => (false, Some(error.to_string())),
    };

    let has_installer_metadata = !metadata_paths.is_empty();
    let resume = classify_resume_state(read_state_file(std::path::Path::new(STATE_PATH)));
    let can_resume = resume.can_resume;
    let can_retry_step = resume.can_retry_step;
    let failed_step = resume.failed_step;
    let restore_status = resume.restore_status;
    let resume_reason = resume.resume_reason;
    let can_reset = has_installer_metadata;
    let g7_evidence = g7_install_evidence();
    let recommended_action = if can_resume {
        "resume"
    } else if can_rollback {
        "rollback"
    } else {
        "manual"
    };
    let mut message = match recommended_action {
        "resume" if can_retry_step => {
            "실패한 단계의 변경을 복원한 뒤 해당 단계부터 다시 실행할 수 있습니다."
        }
        "resume" => "저장된 마지막 정상 단계부터 설치를 이어서 진행할 수 있습니다.",
        "rollback" => {
            "설치 직후 패키지 되돌리기가 가능합니다. 설치기가 새로 넣은 패키지만 제거하고 메타데이터를 정리합니다."
        }
        "reset" => {
            "설치기가 만든 계정, DB, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 제거하고 재설치 상태로 되돌립니다. Let's Encrypt 인증서는 보존합니다."
        }
        _ => {
            "설치기 소유 흔적이 확인되지 않았습니다. 기존 운영 서버일 수 있으므로 자동 초기화 버튼을 제공하지 않습니다."
        }
    }
    .to_string();
    if g7_evidence.install_completed {
        message = format!(
            "그누보드7 DB 생성 기록과 설치 완료 잠금 파일을 확인했습니다. 이미 그누보드7 설치가 완료된 서버입니다. {message}"
        );
    }

    RecoveryApiStatus {
        can_resume,
        can_retry_step,
        can_reset,
        can_rollback,
        recommended_action,
        failed_step,
        restore_status,
        message,
        metadata_paths,
        rollback_reason,
        resume_reason,
        g7_database_created: g7_evidence.database_created,
        g7_install_completed: g7_evidence.install_completed,
        g7_install_lock_path: g7_evidence.install_lock_path,
    }
}

fn g7_install_evidence() -> G7InstallEvidence {
    let report = match fs::read(REPORT_PATH)
        .ok()
        .and_then(|payload| serde_json::from_slice::<serde_json::Value>(&payload).ok())
    {
        Some(report) => report,
        None => return G7InstallEvidence::default(),
    };
    classify_g7_install_evidence(&report, |path| fs::metadata(path).is_ok())
}

fn classify_g7_install_evidence(
    report: &serde_json::Value,
    path_exists: impl Fn(&str) -> bool,
) -> G7InstallEvidence {
    if report
        .get("app_profile")
        .and_then(serde_json::Value::as_str)
        != Some("gnuboard7")
    {
        return G7InstallEvidence::default();
    }

    let database_created = report
        .get("database_checks")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|checks| {
            checks.iter().any(|check| {
                check.get("name").and_then(serde_json::Value::as_str) == Some("database-created")
                    && check.get("status").and_then(serde_json::Value::as_str) == Some("pass")
            })
        });
    let install_lock_path = report
        .get("web_root")
        .and_then(serde_json::Value::as_str)
        .filter(|path| path.starts_with("/home/") || path.starts_with("/var/www/"))
        .map(|web_root| format!("{web_root}/storage/app/g7_installed"));
    let install_completed =
        database_created && install_lock_path.as_deref().is_some_and(&path_exists);

    G7InstallEvidence {
        database_created,
        install_completed,
        install_lock_path,
    }
}

fn classify_resume_state(
    result: std::io::Result<g7_state::state::InstallerState>,
) -> ResumeRecovery {
    match result {
        Ok(state) if state.phase != g7_state::state::InstallerPhase::Completed.as_str() => {
            let failed = state.current_step.clone().or_else(|| {
                state
                    .steps
                    .iter()
                    .rev()
                    .find(|step| step.status == "failed")
                    .map(|step| step.id.clone())
            });
            let restore = failed.as_ref().and_then(|id| {
                state
                    .steps
                    .iter()
                    .find(|step| &step.id == id)
                    .and_then(|step| step.restore_status.clone())
            });
            ResumeRecovery {
                can_resume: true,
                can_retry_step: failed.is_some(),
                failed_step: failed,
                restore_status: restore,
                resume_reason: None,
            }
        }
        Ok(_) => ResumeRecovery {
            can_resume: false,
            can_retry_step: false,
            failed_step: None,
            restore_status: None,
            resume_reason: Some("설치가 이미 완료되었습니다.".to_string()),
        },
        Err(error) => ResumeRecovery {
            can_resume: false,
            can_retry_step: false,
            failed_step: None,
            restore_status: None,
            resume_reason: Some(format!("설치 상태를 읽을 수 없습니다: {error}")),
        },
    }
}

pub(super) fn installer_metadata_paths() -> [&'static str; 6] {
    [
        STATE_PATH,
        OWNED_FILES_PATH,
        REPORT_PATH,
        CONFIG_PATH,
        LOCAL_HOSTS_PATH,
        ROLLBACK_PATH,
    ]
}

#[cfg(test)]
mod tests {
    use super::{classify_g7_install_evidence, classify_resume_state};
    use g7_state::state::{InstallerPhase, InstallerState};

    #[test]
    fn failed_restored_step_is_exposed_as_retryable() {
        let mut state = InstallerState::new("install-id".to_string(), "example.com".to_string());
        state.begin_step("runtime");
        state.fail_step("runtime", "invalid php config", true);

        let recovery = classify_resume_state(Ok(state));

        assert!(recovery.can_resume);
        assert!(recovery.can_retry_step);
        assert_eq!(recovery.failed_step.as_deref(), Some("runtime"));
        assert_eq!(recovery.restore_status.as_deref(), Some("restored"));
    }

    #[test]
    fn completed_install_is_not_resumable() {
        let mut state = InstallerState::new("install-id".to_string(), "example.com".to_string());
        state.set_phase(InstallerPhase::Completed);

        let recovery = classify_resume_state(Ok(state));

        assert!(!recovery.can_resume);
        assert_eq!(
            recovery.resume_reason.as_deref(),
            Some("설치가 이미 완료되었습니다.")
        );
    }

    #[test]
    fn g7_install_evidence_requires_database_and_install_lock() {
        let report = serde_json::json!({
            "app_profile": "gnuboard7",
            "web_root": "/home/g7/public_html",
            "database_checks": [
                {"name": "database-created", "status": "pass", "message": "created"}
            ]
        });

        let evidence = classify_g7_install_evidence(&report, |path| {
            path == "/home/g7/public_html/storage/app/g7_installed"
        });

        assert!(evidence.database_created);
        assert!(evidence.install_completed);
        assert_eq!(
            evidence.install_lock_path.as_deref(),
            Some("/home/g7/public_html/storage/app/g7_installed")
        );
    }
}
