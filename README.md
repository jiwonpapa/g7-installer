# G7 Installer

G7 Installer는 새 Ubuntu VPS에 그누보드7 설치 환경을 준비하기 위한 Rust 기반 CLI 설치기입니다.

목표는 서버에 Rust, Cargo, Git clone 없이 GitHub Release 바이너리만 내려받아 `g7` 명령으로 설치를 진행하는 것입니다.

## 빠른 설치

테스트 단계에서는 별도 도메인 없이 GitHub raw 주소로 bootstrap을 실행합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
```

설치 후 확인:

```bash
g7 --version
g7 doctor
```

설치 준비 실행:

```bash
sudo g7 install --domain example.com
```

전체 기능 스택 기준 설치 계획 예시:

```bash
g7 plan \
  --domain example.com \
  --php-version 8.5 \
  --www-mode redirect-to-root \
  --redis enable \
  --mail-mode smtp-relay \
  --smtp-host smtp.example.com \
  --smtp-port 587 \
  --smtp-from no-reply@example.com
```

PHP 8.3 호환 모드:

```bash
g7 plan --domain example.com --php-version 8.3
```

`bootstrap.sh`는 GitHub Release의 최신 바이너리를 다운로드하고, `checksums.txt`로 SHA256 검증 후 `/usr/local/bin/g7`에 설치합니다.

## 지원 환경

- OS: Ubuntu 24.04 LTS
- 권한: `doctor`는 일반 사용자 가능, `install`은 root 또는 sudo 필요
- 아키텍처:
  - `x86_64-unknown-linux-musl`
  - `aarch64-unknown-linux-musl`
- PHP: 기본 `8.5`, 호환 옵션 `8.3`
- Redis: 기본 활성화 계획
- 메일: SMTP relay 권장, local Postfix는 선택 옵션

## 현재 명령

```bash
g7 doctor
g7 plan --domain example.com [options]
sudo g7 install --domain example.com [options]
g7 status
g7 logs
sudo g7 update
sudo g7 self-update
```

주요 옵션:

```text
--php-version 8.5|8.3
--www-mode redirect-to-root|redirect-to-www|include|none
--redis enable|disable
--mail-mode none|smtp-relay|local-postfix
--smtp-host smtp.example.com
--smtp-port 587
--smtp-from no-reply@example.com
--smtp-encryption none|starttls|tls
--rollback true|false
--preserve-config true|false
--dns-check true|false
```

## 현재 구현된 기능

- `g7 doctor`
  - Ubuntu 24.04 확인
  - root 권한 여부 확인
  - Nginx/Apache 실행 상태 확인
  - 80/443 포트 점유 확인
  - 기존 Nginx site config 확인
  - `/var/www/g7` 존재 여부 확인
  - installer state/owned-files 확인
  - Certbot live 인증서 디렉터리 확인

- `g7 plan --domain example.com`
  - 설치 전 dry-run 계획 출력
  - preflight gate, 설치 예정 패키지, 파일, 서비스, 포트, 중단 조건 표시
  - PHP 8.5 기본, PHP 8.3 호환 옵션 반영
  - www canonical 정책 반영
  - Redis/cache/session/queue 준비 항목 반영
  - SMTP relay/local Postfix 메일 발송 준비 항목 반영
  - DNS/IP, SMTP outbound, Certbot renewal, rollback, 설정보존 gate 표시

- `sudo g7 install --domain example.com`
  - 현재는 MVP 준비 단계입니다.
  - 실제 apt/Nginx/PHP/MariaDB/Certbot 설치 전 단계까지만 수행합니다.
  - 선택 옵션을 `/etc/g7-installer/config.toml`에 저장합니다.
  - fresh-server gate 통과 후 아래 파일과 디렉터리를 생성합니다.

```text
/etc/g7-installer/config.toml
/var/lib/g7-installer/state.json
/var/lib/g7-installer/owned-files.json
/var/lib/g7-installer/rollback.json
/var/log/g7-installer/install.log
/var/log/g7-installer/report.json
/var/backups/g7-installer
/var/www/g7
```

- `g7 logs`
  - installer log 경로 출력

- `g7 status`
  - placeholder 상태 출력

## 아직 미구현

- apt 패키지 설치
- Nginx vhost 렌더링 및 적용
- PHP-FPM 설정
- MariaDB/MySQL 설정
- G7 Release 다운로드 및 압축 해제
- Certbot 인증서 발급
- Certbot 자동갱신 timer 실제 활성화
- 도메인 DNS A/AAAA와 VPS 공인 IP 실제 비교
- SMTP outbound 실제 연결 테스트
- Redis 실제 설치/하드닝
- 실패 시 자동 rollback 실행
- `update`, `self-update` 실제 동작

## GitHub Release 배포 방식

현재 bootstrap은 아래 Release asset을 사용합니다.

- `g7-x86_64-unknown-linux-musl`
- `g7-aarch64-unknown-linux-musl`
- `checksums.txt`

현재 테스트 릴리스:

```text
https://github.com/jiwonpapa/g7-installer/releases/tag/v0.1.0
```

수동으로 Release asset을 만들 때:

```bash
cargo build --release --target x86_64-unknown-linux-musl -p g7-cli
cargo build --release --target aarch64-unknown-linux-musl -p g7-cli

mkdir -p dist
install -m 0755 target/x86_64-unknown-linux-musl/release/g7 dist/g7-x86_64-unknown-linux-musl
install -m 0755 target/aarch64-unknown-linux-musl/release/g7 dist/g7-aarch64-unknown-linux-musl
(cd dist && sha256sum g7-* > checksums.txt)
```

## 로컬 VM smoke test

`g7-test` VM 기준:

```bash
scripts/g7-test-smoke.sh
```

VM reset까지 같이 실행:

```bash
G7_SMOKE_RESET=1 scripts/g7-test-smoke.sh
```

## 개발 검증

커밋 전 기본 검증:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
