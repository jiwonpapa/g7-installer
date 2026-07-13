# 인프라 거버넌스 통제표

| 통제 | 구현 | 자동 증거 |
|---|---|---|
| 동시 파괴 작업 차단 | `/run/lock/g7-installer.lock` 배타 잠금 | `g7-state` 잠금 회귀 테스트 |
| 상태 손상 방지 | 임시 파일 fsync 후 rename, 상위 디렉터리 fsync | 원자적 교체 회귀 테스트 |
| 명령 추적·비밀 마스킹 | `commands.jsonl` 시작/종료/상태/소요시간 기록 | `g7-system` 마스킹 테스트 |
| 신규 VPS 실패 폐쇄 | doctor의 불명확한 서비스·포트·설정 상태를 차단 | `g7-core` doctor 테스트 |
| 단계별 재개 | 상태 v2에 현재 단계, 시도 횟수, 오류, 복원 상태를 저장하고 완료 단계는 건너뜀 | v1 호환·단계 재시도 회귀 테스트 |
| 설정 적용 안전성 | PHP-FPM/DB 후보 검사, 웹서버 native configtest, 파일 트랜잭션 후 reload/restart | 장애 주입·명령 형태·스냅샷 복원 테스트 |
| 비밀값 연속성 | DB 전 단계의 비밀값을 root-only 임시 파일에 보관하고 DB 완료 후 삭제 | 권한·이스케이프·성공 후 삭제 테스트 |
| 파괴 작업 보존 | 인증서와 Certbot 자동 갱신 보존 정책 | reset/rollback golden 테스트 |
| 의존성·라이선스 | `cargo audit`, `cargo deny` | quality-gate 워크플로 |
| 릴리스 무결성 | 체크섬, CycloneDX SBOM, GitHub provenance | release 워크플로 |
| 실제 VPS 검증 | staging LE, 앱 스모크, 상태/트랜잭션 계약, reset, fresh doctor | 승인형 ops-harness 워크플로 산출물 |
| 커버리지 회귀 차단 | 전체 line 77%와 설치·reset·웹 API 위험 모듈별 하한 | llvm-cov JSON 래칫 검사 |
| 실효 설정 검증 | 웹서버/PHP-FPM/MySQL native configtest와 DB·계정·Redis 계약 | 실제 VPS ops-harness 산출물 |

## 변경 승인

`main`은 필수 quality-gate를 통과해야 하며 강제 푸시와 브랜치 삭제를 금지합니다.
파괴 작업, 인증서 정책, 상태 스키마 변경은 CODEOWNERS 검토 대상입니다.

## 증거 보존

- CI 로그와 dry-run 릴리스 산출물은 GitHub Actions에서 보존합니다.
- 실제 VPS 실행 결과는 `target/ops-harness/<timestamp>`를 artifact로 업로드합니다.
- 서버 명령 감사 로그는 `/var/log/g7-installer/commands.jsonl`에 저장합니다.
