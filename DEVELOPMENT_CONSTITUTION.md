# G7 Installer 개발 헌법 초안

이 문서는 G7 Installer를 구현할 때 모든 코드 변경이 따라야 하는 기준이다.
G7 Installer는 새 Ubuntu VPS에 그누보드7/WordPress 운영 환경을 설치하는 root 권한 CLI이므로,
편의성보다 안전성, 재현성, 복구 가능성을 우선한다.

## 1. 목적 우선 원칙

- 이 프로젝트의 목적은 초보 사용자가 새 VPS에서 최소 명령으로 G7/WordPress 설치 기반을 완료하게 하는 것이다.
- 기존 운영 서버를 자동으로 고치거나 병합하지 않는다.
- 설치기는 G7 본체를 수정하지 않는다.
- 설치기는 서버 자동화 도구이며, CMS 기능을 구현하지 않는다.

## 2. 안전 우선 원칙

- fresh server가 아니면 설치를 중단한다.
- 기존 운영 파일은 자동 수정하지 않는다.
- installer가 만든 파일만 `owned-files.json`으로 추적하고 수정한다.
- 변경 작업 전에는 반드시 plan을 만든다.
- 변경 작업 후에는 반드시 state를 기록한다.
- 중간 실패 후 재실행해도 서버를 더 망가뜨리면 안 된다.
- 서버를 변경하지 않는 명령은 가능한 읽기 전용으로 유지한다.

## 3. Rust 코드 원칙

- Rust stable과 Rust 2024 edition을 기준으로 한다.
- 서버에는 Rust toolchain을 설치하지 않는다.
- 배포물은 static binary를 목표로 한다.
- 코드가 정본 문서다. 설치 정책, 기본값, 보안 기준은 먼저 Rust 코드와 rustdoc 주석에 존재해야 한다.
- rustdoc는 `scripts/rustdoc-gate.sh`로 강제한다. 이 게이트는 `RUSTDOCFLAGS="-D warnings"`와 `--document-private-items`로 실행되어 rustdoc 경고를 실패로 취급한다.
- `plan.rs`와 `app_profile.rs`는 설치 범위, 기본값, 앱 요구사항, 보안 정책, 파일/서비스/포트 계획의 SSOT이다. README/SPEC/웹 UI는 이를 설명하거나 표시만 한다.
- 새 명령, 새 서버 변경, 새 보안 정책을 추가할 때 관련 Rust 모듈 상단에 `//!` module doc을 먼저 추가하거나 갱신한다.
- `unwrap`, `expect`, `panic`은 금지한다.
- 실패 가능성이 있는 모든 흐름은 typed error로 표현한다.
- 사용자에게 보이는 오류는 문제, 원인, 다음 조치가 분리되어야 한다.
- 비즈니스 로직은 테스트 가능한 함수로 분리하고, 외부 명령 실행과 직접 섞지 않는다.

## 4. 크레이트 경계 원칙

- `g7-cli`는 CLI 파싱과 출력 조립만 담당한다.
- `g7-core`는 install, doctor, plan, status의 orchestration을 담당한다.
- `g7-system`은 apt, nginx, php, database, systemd, certbot 같은 OS 작업을 담당한다.
- `g7-release`는 G7 릴리스 다운로드, checksum, 압축 해제를 담당한다.
- `g7-state`는 lock, state, owned-files를 담당한다.
- `g7-render`는 nginx와 systemd 템플릿 렌더링만 담당한다.
- 크레이트 간 책임이 섞이면 새 기능보다 경계 정리를 먼저 한다.

## 5. 외부 명령 실행 원칙

- 모든 외부 명령은 공통 command runner를 통해 실행한다.
- 명령, 인자, 종료 코드, stderr는 구조화해서 기록한다.
- 비밀번호, token, `.env`, DB credential은 로그에 남기지 않는다.
- command runner는 dry-run 또는 fake runner 테스트가 가능해야 한다.
- shell string 조합보다 argv 배열 전달을 우선한다.

## 6. 설치 상태 원칙

- install/resume/reset/rollback/provision action은 동일한 OS 배타 잠금을 사용한다.
- 상태와 소유권 메타데이터는 임시 파일 fsync, rename, 상위 디렉터리 fsync 순서로 원자 저장한다.
- 설치 단계는 `state.json`에 기록한다.
- 자동 재개는 DB 설정 이후 TLS/앱 단계처럼 검증된 경계에서만 허용한다.
- 다른 domain 재실행은 중단해야 한다.
- lock이 남아 있으면 stale 여부를 판단하고 사용자에게 다음 조치를 알려야 한다.
- 완료된 step은 idempotent해야 한다.

## 7. 파일 소유권 원칙

- installer가 생성한 파일은 즉시 `owned-files.json`에 기록한다.
- 추적되지 않은 파일은 자동 수정, 삭제, 덮어쓰기를 하지 않는다.
- 템플릿으로 생성하는 파일은 생성 전 충돌 검사를 한다.
- 기존 파일 충돌 시에는 doctor 리포트와 수동 조치 안내를 제공한다.

## 8. 보안 원칙

- bootstrap은 checksum 또는 서명 검증 없이는 바이너리를 설치하지 않는다.
- 다운로드한 G7 릴리스는 checksum 검증 후 사용한다.
- DB root password, app secret, `.env` 내용은 stdout과 로그에 출력하지 않는다.
- 권한은 필요한 명령에서만 요구한다.
- root로 실행되는 코드에는 임의 shell 실행 경로를 만들지 않는다.
- 임시 파일은 안전한 staging directory에서 만들고 검증 후 배치한다.

## 9. 사용자 경험 원칙

- 초보 사용자가 이해할 수 있는 문장으로 실패 원인을 설명한다.
- 오류 메시지는 해결 가능한 다음 명령을 포함한다.
- plan은 실제 변경될 패키지, 파일, 서비스, 포트를 보여준다.
- install 완료 출력은 domain, 설치 경로, HTTPS 상태, 다음 접속 URL을 포함한다.
- PHP는 기본 8.5로 계획하고, 8.5는 Ondrej PHP PPA, 8.3은 Ubuntu 기본 apt 소스를 사용한다.
- 공개 앱 선택은 G7/WordPress를 기준으로 한다. 내부 실험 프로필이 있더라도 사용자 문서와 공개 UI에서는 지원 범위를 과장하지 않는다.
- www canonical 정책은 설치 전 plan에 반드시 드러나야 한다.
- Redis, 메일 발송, Certbot 자동갱신, DNS/IP 검증은 전체 기능 설치 프로필의 일부로 본다.
- SMTP 비밀번호/API key 같은 비밀값은 CLI 인자와 로그에 남기지 않는다.
- 대화형 확인은 명확해야 하며, CI나 자동화 환경을 위한 non-interactive 옵션을 고려한다.

## 10. 테스트 원칙

- 모든 핵심 로직은 unit test를 가진다.
- fresh server 판정은 regression test를 가진다.
- plan 출력은 snapshot test로 보호한다.
- 템플릿 렌더링은 실제 nginx/systemd 문법에 가까운 테스트를 둔다.
- state resume은 중간 실패 case를 포함해 테스트한다.
- 외부 명령은 fake runner로 통합 테스트한다.
- 릴리스 전 Ubuntu 24.04 fresh VPS 또는 VM smoke test를 통과해야 한다.
- reset/rollback은 인증서, DB, 사이트 계정, 웹루트, systemd, apt purge 보존·삭제 golden 테스트를 가진다.
- 실제 VPS 하네스는 staging 인증서, 앱 스모크, 리포트 계약, reset, fresh doctor를 분리 증명한다.

## 11. 릴리스 원칙

- release artifact는 target triple별로 만든다.
- `checksums.txt`는 release artifact와 함께 배포한다.
- 릴리스에는 CycloneDX SBOM과 GitHub build provenance를 함께 배포한다.
- bootstrap은 latest release 감지, checksum 검증, `/usr/local/bin/g7inst` 설치만 담당한다.
- `g7inst --version`은 설치기 버전과 build target을 출력한다.
- self-update를 공개 기능으로 구현할 때는 현재 바이너리 교체 실패 시 복구 가능해야 한다.

## 12. 개발 완료 기준

코드 변경은 아래 조건을 만족해야 완료로 본다.

- `cargo fmt --check` 통과
- `cargo clippy --all-targets -- -D warnings` 통과
- `bash scripts/rustdoc-gate.sh` 통과
- `cargo test` 통과
- `cargo audit`와 `cargo deny check` 통과
- 관련 명령의 plan, state, log 동작 확인
- root 권한 변경 작업은 fake runner 또는 VM smoke로 증명
- 문서와 실제 동작이 어긋나지 않음

## 13. 금지 사항

- 기존 운영 서버 자동 수정
- installer 소유가 아닌 파일 덮어쓰기
- 비밀값 로그 출력
- root shell string 임의 조합
- 실패를 무시하고 다음 단계 진행
- state 기록 없는 변경 작업
- rollback/report 기록 없는 서버 변경 작업
- DNS/IP 불일치를 무시한 Certbot 발급 시도
- Redis 6379 포트 외부 공개
- 테스트 없이 fresh server 판정 변경
- G7 본체 레포 직접 수정

## 14. 판단 기준

애매한 경우 아래 순서로 판단한다.

1. 사용자 서버를 보호하는가
2. 재실행해도 안전한가
3. 실패 원인과 다음 조치가 명확한가
4. installer 책임 범위 안에 있는가
5. 테스트 가능한 구조인가
