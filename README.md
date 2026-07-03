# G7 Installer

G7 Installer는 새 Ubuntu VPS에 그누보드7 설치 환경을 준비하기 위한 Rust 기반 CLI 설치기입니다.

목표는 서버에 Rust, Cargo, Git clone 없이 GitHub Release 바이너리만 내려받아 `g7inst` 명령으로 설치를 진행하는 것입니다.

> 현재 `v0.1.7`는 설치기 바이너리 설치, 서버 사전 점검, 풀스크린 TUI 선택, 설치 준비 파일 생성까지 검증합니다.
> 실제 `apt install`, Nginx/Apache/PHP/DB 서비스 설치와 포트 기동 검증은 아직 다음 단계입니다.

## 빠른 설치

Ubuntu VPS에서 Rust/Cargo 없이 설치기만 바로 설치합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
```

특정 릴리스로 고정하려면:

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo env G7_INSTALL_VERSION=v0.1.7 bash
```

설치된 바이너리 확인:

```bash
g7inst --version
g7inst doctor
```

풀스크린 TUI로 설치 준비를 시작합니다.

```bash
sudo g7inst setup
```

로컬 테스트 도메인으로 반복 테스트할 때:

```bash
sudo g7inst setup --local-test --domain g7-test.local
```

기존 문답식 CLI가 필요하면:

```bash
sudo g7inst setup --plain --local-test --domain g7-test.local
```

TUI 조작:

```text
Up/Down: 항목 이동
Left/Right: 선택값 변경
Enter: 선택 또는 Prepare install 실행
문자 입력: 도메인, SMTP host/from 입력
q, Esc, Ctrl+C: 종료
```

`Prepare install`은 한 번만 실행됩니다. 다시 테스트하려면 TUI를 종료한 뒤 `sudo g7inst reset --yes`로 준비 흔적을 지우고 다시 실행합니다.

테스트 흔적을 지우고 다시 시작하려면:

```bash
sudo g7inst reset --yes
```

자동화/디버그용 설치 준비 실행:

```bash
sudo g7inst install --domain example.com
```

전체 기능 스택 기준 설치 계획 예시:

```bash
g7inst plan \
  --domain example.com \
  --web-server nginx \
  --php-version 8.5 \
  --database mariadb \
  --site-user g7 \
  --web-root-mode public-html \
  --www-mode redirect-to-root \
  --redis enable \
  --security-profile standard \
  --ssh-policy audit-only \
  --mail-mode smtp-relay \
  --smtp-host smtp.example.com \
  --smtp-port 587 \
  --smtp-from no-reply@example.com
```

PHP 8.3 호환 모드:

```bash
g7inst plan --domain example.com --php-version 8.3
```

`bootstrap.sh`는 GitHub Release의 최신 바이너리를 다운로드하고, `checksums.txt`로 SHA256 검증 후 `/usr/local/bin/g7inst`에 설치합니다.

## 지원 환경

- OS: Ubuntu 24.04 LTS
- 권한: `doctor`는 일반 사용자 가능, `install`은 root 또는 sudo 필요
- 아키텍처:
  - `x86_64-unknown-linux-musl`
  - `aarch64-unknown-linux-musl`
- PHP: 기본 `8.5`, 호환 옵션 `8.3`
- 웹루트 기본값: `/home/g7/public_html`
- Redis: 기본 활성화 계획
- 메일: SMTP relay 권장, local Postfix는 선택 옵션

## 운영 기본 정책

- 웹루트는 계정 기준으로 잡습니다. 기본은 `--site-user g7 --web-root-mode public-html`이며 결과 경로는 `/home/g7/public_html`입니다.
- `--web-root-mode www`는 `/home/<site-user>/www`, `system`은 `/var/www/<domain>`, `custom`은 `--web-root /absolute/path`를 사용합니다.
- `/var/www/g7`은 이전 테스트용 placeholder 경로였고, 실제 운영 기본값이 아닙니다. 현재는 legacy/test 흔적 점검 대상으로만 남깁니다.
- DB 기본 비밀번호는 없습니다. 실제 DB 생성 단계에서는 앱 DB 계정 비밀번호를 랜덤 생성하고 stdout/log에 출력하지 않는 정책입니다.
- DB와 Redis는 localhost/unix socket 전용으로 계획합니다. `3306`, `6379`는 외부 공개 금지입니다.
- Redis는 `127.0.0.1`/`::1` 또는 unix socket, protected-mode 유지가 기본 보안 정책입니다.
- SSH는 기본 `audit-only`입니다. 현재 접속 포트와 세션을 보존하고, root login/password auth 위험을 리포트합니다. SSH hardening은 `--ssh-policy harden`을 명시한 경우에만 적용 대상으로 봅니다.
- `security-profile`은 `audit-only`, `standard`, `hardened`를 지원합니다. 기본 `standard`는 앱/DB/Redis/PHP 권한과 로컬 bind를 적용 대상으로 보고, 방화벽/SSH 변경은 보수적으로 다룹니다.

## 사용법

### 1. g7-test VM에서 직접 테스트

로컬에서 빌드 후 `g7-test` VM으로 복사합니다.

```bash
cd /Users/neojins/workspace/g7-installer
cargo build --release --target x86_64-unknown-linux-musl -p g7-cli --bin g7inst
scp target/x86_64-unknown-linux-musl/release/g7inst g7-test:/tmp/g7inst
```

VM 안에서 대화형 세팅을 시작합니다.

```bash
ssh -t g7-test 'chmod +x /tmp/g7inst && sudo /tmp/g7inst setup --local-test --domain g7-test.local'
```

기본 `setup`은 풀스크린 TUI로 실행됩니다. 기존 문답식 프롬프트가 필요하면 `--plain`을 붙입니다.

```bash
sudo g7inst setup --plain --local-test --domain g7-test.local
```

TUI 화면 조작:

```text
Up/Down: 선택지 이동
Left/Right: 선택값 변경
Enter: 선택 또는 Prepare install 실행
문자 입력: 도메인, SMTP host/from 입력
q, Esc, Ctrl+C: 종료
```

`Prepare install`이 성공한 뒤 같은 화면에서 Enter를 다시 눌러도 재실행하지 않습니다. 반복 테스트는 `reset --yes` 후 다시 시작합니다.

자동화 방식으로 한 번에 확인하려면:

```bash
scripts/g7-test-smoke.sh
```

테스트 후 installer가 만든 파일만 초기화하려면:

```bash
ssh -t g7-test 'sudo /tmp/g7inst reset --yes'
```

`reset --yes`는 `/etc/g7-installer`, `/var/lib/g7-installer`, `/var/log/g7-installer`, `/var/backups/g7-installer`, `/var/www/g7` 같은 설치 준비 흔적을 삭제합니다. 예전 명령어 충돌을 막기 위해 `/usr/local/bin/g7`, `/tmp/g7`도 함께 삭제합니다. 현재 실행 중인 `g7inst` 바이너리는 삭제하지 않으므로 바로 다시 테스트할 수 있습니다.

### 2. 실제 VPS에서 테스트

GitHub Release에 `g7inst-*` asset이 올라간 뒤에는 서버에서 bootstrap만 실행합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
g7inst --version
sudo g7inst setup
```

공개 도메인이 아직 없으면 로컬 테스트 모드로 시작합니다.

```bash
sudo g7inst setup --local-test --domain g7-test.local
```

실제 도메인이 있으면:

```bash
sudo g7inst setup --domain example.com
```

### 3. 자동화/디버그 명령

대화형 대신 옵션을 직접 지정할 수 있습니다.

```bash
g7inst plan \
  --domain example.com \
  --web-server nginx \
  --php-version 8.5 \
  --database mariadb \
  --site-user g7 \
  --web-root-mode public-html \
  --redis enable \
  --security-profile standard \
  --ssh-policy audit-only \
  --mail-mode none

sudo g7inst install \
  --domain example.com \
  --web-server nginx \
  --php-version 8.5 \
  --database mariadb \
  --site-user g7 \
  --web-root-mode public-html \
  --redis enable \
  --security-profile standard \
  --ssh-policy audit-only \
  --mail-mode none
```

로컬 테스트 도메인은 DNS/Certbot 검사를 건너뜁니다.

```bash
g7inst plan --local-test --domain g7-test.local
sudo g7inst install --local-test --domain g7-test.local
```

### 4. 선택 가능한 항목

```text
web server: nginx, apache
PHP-FPM: 8.5, 8.3
database: mariadb, mysql
site user: any safe Linux account name, default g7
web root mode: public-html, www, system, custom
redis: enable, disable
mail: none, smtp-relay, local-postfix
security profile: standard, hardened, audit-only
ssh policy: audit-only, harden
mode: public, local-test
```

현재 `install`은 실제 패키지를 설치하지 않고 준비 상태까지만 만듭니다. 생성되는 핵심 파일은 `/etc/g7-installer/config.toml`, `/var/lib/g7-installer/state.json`, `/var/lib/g7-installer/owned-files.json`입니다. 앱 웹루트는 config/report에 기록만 하고 아직 생성하지 않습니다.

## 현재 명령

```bash
sudo g7inst setup
g7inst doctor
g7inst plan --domain example.com [options]
sudo g7inst install --domain example.com [options]
g7inst status
g7inst logs
sudo g7inst reset --yes
sudo g7inst update
sudo g7inst self-update
```

주요 옵션:

```text
--local-test
--web-server nginx|apache
--php-version 8.5|8.3
--database mariadb|mysql
--site-user g7
--web-root-mode public-html|www|system|custom
--web-root /absolute/path
--www-mode redirect-to-root|redirect-to-www|include|none
--redis enable|disable
--mail-mode none|smtp-relay|local-postfix
--smtp-host smtp.example.com
--smtp-port 587
--smtp-from no-reply@example.com
--smtp-encryption none|starttls|tls
--security-profile audit-only|standard|hardened
--ssh-policy audit-only|harden
--rollback true|false
--preserve-config true|false
--dns-check true|false
```

## 현재 구현된 기능

- `g7inst setup`
  - 풀스크린 TUI 기반 설치 준비 흐름
  - `ratatui` + `crossterm` 기반 화면 UI
  - `--plain` 사용 시 기존 `dialoguer` 기반 선택 UI
  - 화면에 Up/Down, Left/Right, Enter, q/Esc/Ctrl+C 조작법 안내
  - 서버 상태 체크 후 도메인, 웹서버, PHP, DB, 웹루트, Redis, 메일, 보안 정책 선택
  - 요약 확인 후 `install` 준비 단계 실행

- `g7inst doctor`
  - Ubuntu 24.04 확인
  - root 권한 여부 확인
  - Nginx/Apache 실행 상태 확인
  - 80/443 포트 점유 확인
  - 기존 Nginx/Apache site config 확인
  - legacy `/var/www/g7` 테스트 경로 존재 여부 확인
  - installer state/owned-files 확인
  - Certbot live 인증서 디렉터리 확인

- `g7inst plan --domain example.com`
  - 설치 전 dry-run 계획 출력
  - preflight gate, 설치 예정 패키지, 파일, 서비스, 포트, 중단 조건 표시
  - `--local-test`로 공개 DNS/Let's Encrypt 없이 로컬 테스트 도메인 계획 출력
  - `--web-server nginx|apache`, `--database mariadb|mysql` 선택 반영
  - `--site-user`, `--web-root-mode`, `--web-root` 선택 반영
  - PHP 8.5 기본, PHP 8.3 호환 옵션 반영
  - www canonical 정책 반영
  - Redis/cache/session/queue 준비 항목 반영
  - DB/Redis localhost-only, 랜덤 DB 비밀번호 정책, SSH audit/harden 정책 표시
  - SMTP relay/local Postfix 메일 발송 준비 항목 반영
  - DNS/IP, SMTP outbound, Certbot renewal, rollback, 설정보존 gate 표시

- `sudo g7inst install --domain example.com`
  - 현재는 MVP 준비 단계입니다.
  - 실제 apt/Nginx/PHP/MariaDB/Certbot/Redis/SSH 설정 전 단계까지만 수행합니다.
  - 선택 옵션을 `/etc/g7-installer/config.toml`에 저장합니다.
  - 선택한 앱 웹루트는 config/report에 기록하지만 아직 디렉터리를 생성하지 않습니다.
  - `--local-test` 사용 시 `/etc/g7-installer/local-hosts.txt`에 hosts 등록 힌트를 저장합니다.
  - fresh-server gate 통과 후 아래 파일과 디렉터리를 생성합니다.

```text
/etc/g7-installer/config.toml
/var/lib/g7-installer/state.json
/var/lib/g7-installer/owned-files.json
/var/lib/g7-installer/rollback.json
/var/log/g7-installer/install.log
/var/log/g7-installer/report.json
/var/backups/g7-installer
/etc/g7-installer/local-hosts.txt (local-test only)
```

- `g7inst logs`
  - installer log 경로 출력

- `g7inst status`
  - placeholder 상태 출력

- `sudo g7inst reset --yes`
  - 테스트 VM 반복 실행용
  - `owned-files.json`에 기록된 installer 소유 파일만 삭제
  - legacy `/usr/local/bin/g7`, `/tmp/g7`도 삭제
  - 현재 `g7inst` 바이너리는 유지
  - `--dry-run`으로 삭제 대상 미리보기 가능

## 아직 미구현

- apt 패키지 설치
- Nginx vhost 렌더링 및 적용
- PHP-FPM 설정
- MariaDB/MySQL 설정
- DB app user/password 랜덤 생성 및 root-only 저장
- G7 Release 다운로드 및 압축 해제
- Certbot 인증서 발급
- Certbot 자동갱신 timer 실제 활성화
- 도메인 DNS A/AAAA와 VPS 공인 IP 실제 비교
- SMTP outbound 실제 연결 테스트
- Redis 실제 설치/하드닝
- SSH 설정 audit/hardening
- UFW/방화벽 allow/deny 적용
- 실패 시 자동 rollback 실행
- `update`, `self-update` 실제 동작

## GitHub Release 배포 방식

현재 bootstrap은 아래 Release asset을 사용합니다.

- `g7inst-x86_64-unknown-linux-musl`
- `g7inst-aarch64-unknown-linux-musl`
- `checksums.txt`

현재 테스트 릴리스:

```text
https://github.com/jiwonpapa/g7-installer/releases/tag/v0.1.7
```

수동으로 Release asset을 만들 때:

```bash
cargo build --release --target x86_64-unknown-linux-musl -p g7-cli --bin g7inst
cargo build --release --target aarch64-unknown-linux-musl -p g7-cli --bin g7inst

mkdir -p dist
install -m 0755 target/x86_64-unknown-linux-musl/release/g7inst dist/g7inst-x86_64-unknown-linux-musl
install -m 0755 target/aarch64-unknown-linux-musl/release/g7inst dist/g7inst-aarch64-unknown-linux-musl
(cd dist && sha256sum g7inst-* > checksums.txt)
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
