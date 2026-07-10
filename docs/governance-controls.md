# 인프라 거버넌스 통제표

| 통제 | 구현 | 자동 증거 |
|---|---|---|
| 동시 파괴 작업 차단 | `/run/lock/g7-installer.lock` 배타 잠금 | `g7-state` 잠금 회귀 테스트 |
| 상태 손상 방지 | 임시 파일 fsync 후 rename, 상위 디렉터리 fsync | 원자적 교체 회귀 테스트 |
| 명령 추적·비밀 마스킹 | `commands.jsonl` 시작/종료/상태/소요시간 기록 | `g7-system` 마스킹 테스트 |
| 신규 VPS 실패 폐쇄 | doctor의 불명확한 서비스·포트·설정 상태를 차단 | `g7-core` doctor 테스트 |
| 재개 경계 | DB 설정 이후 TLS/앱 단계만 `resume` 허용 | 리포트 복원·필수 필드 테스트 |
| 파괴 작업 보존 | 인증서와 Certbot 자동 갱신 보존 정책 | reset/rollback golden 테스트 |
| 의존성·라이선스 | `cargo audit`, `cargo deny` | quality-gate 워크플로 |
| 릴리스 무결성 | 체크섬, CycloneDX SBOM, GitHub provenance | release 워크플로 |
| 실제 VPS 검증 | staging LE, 앱 스모크, reset, fresh doctor | 승인형 ops-harness 워크플로 산출물 |

## 변경 승인

`main`은 필수 quality-gate를 통과해야 하며 강제 푸시와 브랜치 삭제를 금지합니다.
파괴 작업, 인증서 정책, 상태 스키마 변경은 CODEOWNERS 검토 대상입니다.

## 증거 보존

- CI 로그와 dry-run 릴리스 산출물은 GitHub Actions에서 보존합니다.
- 실제 VPS 실행 결과는 `target/ops-harness/<timestamp>`를 artifact로 업로드합니다.
- 서버 명령 감사 로그는 `/var/log/g7-installer/commands.jsonl`에 저장합니다.
