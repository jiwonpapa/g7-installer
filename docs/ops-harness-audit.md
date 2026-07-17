# 운영 하네스 감사

기준일: 2026-07-13

## 자동 검증 계층

| 계층 | 실행 | 검증 범위 |
|---|---|---|
| 빠른 게이트 | `bash scripts/quick-gate.sh` | shell/정적 UI/Rust 핵심 테스트 |
| 전체 게이트 | `bash scripts/quality-gate.sh` | fmt, test, clippy, rustdoc, 전체 coverage 77%와 위험 모듈별 하한, audit, deny, 웹 빌드 |
| CI | `.github/workflows/quality-gate.yml` | 전체 게이트, Rust 1.85, dual-musl 릴리스 dry-run |
| 실제 VPS | `.github/workflows/ops-harness.yml` | 승인된 폐기 가능 Ubuntu 24.04 VPS 설치·초기화 |
| 릴리스 | `.github/workflows/release.yml` | 태그/버전 일치, 전체 quality gate와 웹 E2E 재검증, 체크섬, SBOM, provenance, Release |

## 커버리지 회귀 하한

전체 line coverage는 77% 이상이어야 합니다. 추가로 설치 트랜잭션 72%, 웹 API 64%,
프로비저닝 액션 62%, 웹 라우트 63% 등 파괴 작업과 사용자 제어 경로는 파일별 하한을
`scripts/check-coverage-ratchet.py`에서 검사합니다. 전체 수치가 유지되더라도 위험 모듈의
커버리지가 하락하면 quality gate는 실패합니다.

## 실제 VPS 증명 계약

`scripts/ops-harness.sh`는 호환 wrapper이며 실제 제어 로직은 표준 라이브러리만 사용하는
`scripts/ops_harness.py`에 있습니다. 이 하네스는 `G7_OPS_CONFIRM_DISPOSABLE=1` 없이는
실행되지 않습니다.
기본값은 실제 DNS 도메인, Let's Encrypt staging, 앱 HTTP 스모크이며 운영 인증서 발급은
별도 명시 없이는 금지합니다.

기본 실행이 증명하는 항목:

1. Ubuntu 24.04와 설치 전 `doctor install_allowed: true`
2. 선택한 앱/웹서버/PHP/MySQL 버전 옵션의 plan과 install 완료
3. `report.json` schema version, 필수 식별값, 모든 검사 섹션의 실패 없음
4. 상태 v2의 7개 단계 완료, 현재 실행 단계 없음
5. 미완료 파일 트랜잭션, 후보 설정 파일, 임시 비밀값이 남지 않음
6. 최종 비밀 파일 권한 `0600`
7. 설정 안내서와 앱 URL 실제 응답
8. 설치 후 fresh doctor 차단
9. reset dry-run과 실제 reset
10. 신규 설치 패키지, 사이트 계정, 웹루트, 관리 서비스 제거
11. 기존 Let's Encrypt lineage 보존
12. reset 뒤 fresh doctor 허용
13. 웹서버 native configtest와 Nginx 기본 deny vhost
14. PHP-FPM configtest, 사이트 전용 pool, 기본 `www` pool 비활성화
15. MySQL configtest, 선택 버전, DB와 DB 계정 생성
16. Redis localhost bind와 protected mode

GitHub Actions 실행은 `disposable-vps` Environment 승인과 다음 secret이 필요합니다.

- `G7_OPS_HOST`
- `G7_OPS_USER`
- `G7_OPS_SSH_PRIVATE_KEY`
- `G7_OPS_SSH_KNOWN_HOSTS`

## 로컬 실행 예시

```bash
G7_OPS_CONFIRM_DISPOSABLE=1 \
G7_OPS_HOST=g7-test \
G7_OPS_DOMAIN=staging.example.com \
G7_OPS_SOURCE=local \
G7_OPS_CERTBOT_SCOPE=staging \
G7_OPS_APP=gnuboard7 \
G7_OPS_WEB_SERVER=nginx \
G7_OPS_PHP_VERSION=8.3 \
G7_OPS_DATABASE_VERSION=8.0 \
bash scripts/ops-harness.sh
```

Python 하네스는 SSH 실행, 산출물 저장, `report.json`/`state.json` 계약 검증, 신규 설치 패키지
목록 산출, reset 검증, 앱 HTTP 스모크를 담당합니다. Bash wrapper는 CI와 기존 문서 명령 호환만
맡습니다.

Rust 장애 주입 테스트는 PHP/DB 후보 설정 실패 시 활성 파일이 생성되지 않는지, 트랜잭션이 기존 파일을 복원하는지, 패키지 재시도가 최초 기준선을 보존하는지 확인합니다. 로컬 브라우저 E2E는 관리자 명령 API를 mock 처리합니다. root 권한 서버 변경의 최종 증거는
반드시 폐기 가능 VPS 하네스 artifact로 남깁니다.

## 남은 운영 조건

- 실제 VPS 워크플로는 비용과 파괴 위험 때문에 수동 승인만 허용합니다.
- 워크플로 입력에서 Nginx/Apache, PHP 8.3/8.5, MySQL 8.0/8.4 조합을 선택해 각각 증거를 남깁니다.
- 운영 Let's Encrypt는 정기 CI 대상이 아닙니다.
- 릴리스 전 최신 커밋의 quality gate와 필요한 VPS 조합 증거를 함께 확인합니다.
