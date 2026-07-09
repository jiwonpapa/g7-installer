use super::*;

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
    let can_reset = has_installer_metadata && !can_rollback;
    let recommended_action = if can_rollback {
        "rollback"
    } else if can_reset {
        "reset"
    } else {
        "manual"
    };
    let message = match recommended_action {
        "rollback" => {
            "설치 직후 패키지 되돌리기가 가능합니다. 설치기가 새로 넣은 패키지만 제거하고 메타데이터를 정리합니다."
        }
        "reset" => {
            "설치기가 만든 계정, DB, 인증서, 서비스, 웹루트/설정 파일, 패키지, 메타데이터를 제거하고 재설치 상태로 되돌립니다."
        }
        _ => {
            "설치기 소유 흔적이 확인되지 않았습니다. 기존 운영 서버일 수 있으므로 자동 초기화 버튼을 제공하지 않습니다."
        }
    }
    .to_string();

    RecoveryApiStatus {
        can_reset,
        can_rollback,
        recommended_action,
        message,
        metadata_paths,
        rollback_reason,
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
