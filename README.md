# G7 Installer

G7 Installer는 새 Ubuntu VPS에 그누보드7 설치 환경을 준비하기 위한 Rust 기반 CLI 설치기입니다.

목표는 서버에 Rust, Cargo, Git clone 없이 GitHub Release 바이너리만 내려받아 `g7inst` 명령으로 설치를 진행하는 것입니다.

> 현재 `v0.2.7`는 설치기 바이너리 설치, 웹 마법사 기반 설정, 앱 프로파일(`gnuboard7`, `wordpress`, `laravel`)별 요구사항 계획/리포트, 라이트/다크 테마, 잘리지 않는 항목별 도움말, 서버 사전 점검 통과 후 다음 단계 진입, 설치 계획 생성, 실제 apt 패키지 설치, 진행률 표시, HTML 결과 리포트, 설치 직후 패키지 되돌리기, 오류 힌트/상세 리포팅, 첫 token 접속 IP 잠금까지 검증합니다.
> Nginx/Apache vhost 적용, DB 앱 계정 생성, G7 Release 배치, Certbot 인증서 발급은 다음 단계입니다.

## AWS Lightsail 기준 배포

현재 배포 기준 VPS는 Amazon Lightsail Ubuntu 24.04 LTS입니다. 가입부터 인스턴스 생성까지 자세한 절차는 [Lightsail Ubuntu 24.04 준비 매뉴얼](docs/lightsail-ubuntu24-setup-guide.md)을 확인합니다.

공식 안내 기준으로 신규 AWS 계정은 Free Tier 가입 시 100 USD 크레딧을 즉시 받을 수 있고, 활동 조건에 따라 최대 200 USD까지 6개월 동안 사용할 수 있습니다. Lightsail 가격표에는 Linux/Unix 공인 IPv4 번들 중 월 12 USD 플랜이 3개월 무료 대상에 포함된다고 안내되어 있습니다. 다만 콘솔의 인스턴스 카드에는 무료 표시가 안 보일 수 있으므로, 실제 적용 여부는 계정의 Free Tier/크레딧 상태와 최종 생성 전 결제 안내를 기준으로 확인합니다.

> 주의: AWS Free Tier, 크레딧, Lightsail 무료 제공, 번들 가격은 AWS가 언제든 변경할 수 있는 정책입니다. 이 문서의 금액과 무료 조건은 설치 기준을 잡기 위한 참고이며, 실제 과금 여부는 인스턴스 생성 시점의 AWS 콘솔과 결제 안내가 기준입니다.

권장 인스턴스:

- 이미지: Linux/Unix, OS Only, Ubuntu 24.04 LTS
- 네트워크: 공인 IPv4 포함 듀얼 스택
- 플랜: 범용, 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송, 월 12 USD
- 방화벽: 22/tcp, 80/tcp, 443/tcp 허용
- 고정 IP: 생성 후 인스턴스에 연결

Lightsail `Add launch script`에는 짧은 부트스트랩만 넣습니다. 긴 스크립트를 콘솔에 직접 붙여넣지 않고, GitHub에 버전 관리되는 `scripts/lightsail-init.sh`를 받아 실행합니다.

```bash
#!/bin/sh
set -eu
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/lightsail-init.sh -o "$tmp"
bash "$tmp"
```

이 스크립트는 `g7inst` 설치까지만 처리합니다. OS 업데이트, swap, UFW, fail2ban, SSH 보안 점검, 웹서버, PHP, DB, Redis, Certbot, 앱 설치는 `g7inst setup` 웹 UI가 처리합니다.

Lightsail은 사용자 시작 스크립트를 자체 `/bin/sh` 초기화 스크립트에 이어 붙여 실행할 수 있습니다. 그래서 `bash` 전용 `pipefail`이나 `curl ... | bash` 형태를 쓰지 않고, `sh` 호환 명령으로 파일을 내려받은 뒤 `bash "$tmp"`로 실행합니다.

운영 서버에서 재현성을 더 엄격하게 보려면 `main` 대신 릴리스 태그나 커밋 해시가 들어간 raw URL로 고정합니다. `bootstrap.sh`가 내려받는 `g7inst` 바이너리는 GitHub Release의 `checksums.txt`로 SHA256 검증합니다.

SSH 키는 시작 스크립트에 넣지 않습니다. 가장 쉬운 흐름은 Lightsail 인스턴스 생성 화면에서 SSH 키를 만들고 다운로드한 `.pem` 개인키를 Mac에 보관하는 방식입니다.

Mac에 저장:

```bash
mkdir -p ~/.ssh
mv ~/Downloads/YOUR_LIGHTSAIL_KEY.pem ~/.ssh/lightsail_g7inst.pem
chmod 600 ~/.ssh/lightsail_g7inst.pem
```

접속:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem ubuntu@SERVER_IP
```

로컬에서 직접 키를 만들고 싶으면 `.pub` 공개키만 Lightsail에 업로드합니다.

```bash
ssh-keygen -t rsa -b 4096 -f ~/.ssh/lightsail_g7inst_202607 -C "lightsail-g7inst"
cat ~/.ssh/lightsail_g7inst_202607.pub
ssh -i ~/.ssh/lightsail_g7inst_202607 ubuntu@SERVER_IP
```

개인키는 서버, Git, 문서, 시작 스크립트에 넣지 않습니다.

부트스트랩 확인:

```bash
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
g7inst --version
g7inst doctor
```

원격 서버에서 웹 설치 UI를 열 때는 7717 포트를 외부 공개하지 않고 SSH 터널을 씁니다.

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

서버에서 설치기를 시작합니다.

```bash
sudo g7inst setup --domain example.com
```

터미널에 출력된 token URL을 Mac 브라우저에서 엽니다.

### Lightsail 배포/접속 절차

실서버 설치는 7717 포트를 외부에 열지 않고 SSH 터널로 접속합니다. Lightsail 방화벽은 22/tcp, 80/tcp, 443/tcp만 엽니다.

1. 도메인 DNS A 레코드를 Lightsail 고정 IP로 지정합니다. 설치 중에는 Cloudflare 프록시를 끄고 DNS only로 둡니다.
2. Mac에서 SSH 터널을 포함해 서버에 접속합니다.

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

SSH alias를 등록했다면 더 짧게 접속합니다.

```bash
ssh -L 7717:127.0.0.1:7717 g7installer
```

3. 열린 SSH 세션 안에서 설치 웹 UI를 시작합니다.

```bash
sudo g7inst setup --domain example.com
```

4. 터미널에 출력된 URL을 Mac 브라우저에서 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

5. 설치가 끝날 때까지 SSH 터널 터미널을 닫지 않습니다. 설치가 끝나면 `Ctrl+C`로 `g7inst setup`을 종료합니다.

`7717/tcp`는 설치 관리자 UI 포트입니다. 인터넷에 직접 공개하지 않습니다. DB, Redis, 메일 수신 포트도 외부에 열지 않습니다.

## 빠른 설치

Ubuntu VPS에서 Rust/Cargo 없이 설치기만 바로 설치합니다.

AWS Lightsail에서 새 Ubuntu 24.04 VPS를 먼저 만들 때는 [Lightsail Ubuntu 24.04 준비 매뉴얼](docs/lightsail-ubuntu24-setup-guide.md)을 확인합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
```

특정 릴리스로 고정하려면:

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo env G7_INSTALL_VERSION=v0.2.7 bash
```

설치된 바이너리 확인:

```bash
g7inst --version
g7inst doctor
```

웹 컨트롤러로 설치를 시작합니다.

```bash
sudo g7inst setup
```

터미널에 출력된 URL을 브라우저에서 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

원격 VPS는 SSH 터널로 접속합니다.

```bash
ssh -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

로컬 테스트 도메인으로 반복 테스트할 때:

```bash
sudo g7inst setup --local-test --domain g7-test.local
```

웹 UI의 `패키지 설치 시작`은 한 번만 실행됩니다. 뒤로가기, 새로고침, 외부 링크 이동 후 돌아와도 브라우저 세션과 서버 report를 기준으로 설치/복구 상태를 다시 복원합니다. 다시 테스트할 때 apt 패키지까지 되돌리려면 `패키지 되돌리기` 또는 `sudo g7inst rollback --yes`를 먼저 실행합니다. installer 메타데이터만 지우려면 `sudo g7inst reset --yes`를 사용합니다.

웹 UI의 복구 버튼은 설치기 소유 흔적이 확인될 때만 열립니다. `rollback` 가능 상태에서는 `메타데이터 리셋`을 먼저 누를 수 없게 막습니다. 운영 중이던 웹서비스만 감지되고 `/var/lib/g7-installer/state.json`, `owned-files.json`, `report.json` 같은 설치기 메타데이터가 없으면 자동 초기화 버튼을 제공하지 않습니다.

테스트 흔적을 지우고 다시 시작하려면:

```bash
sudo g7inst reset --yes
```

설치 직후 패키지까지 되돌리려면:

```bash
sudo g7inst rollback --yes
```

`rollback`은 운영 사이트용 백업 복구가 아닙니다. 설치 단계가 `packages-installed`이고, 웹루트가 비어 있거나 없고, 설치 전 패키지 기준 정보가 남아 있을 때만 실행합니다. 설치 전 이미 있던 패키지와 그 서비스는 건드리지 않고, 설치기가 새로 넣은 패키지만 apt purge 대상으로 봅니다.

자동화/디버그용 패키지 설치 실행:

```bash
sudo g7inst install --domain example.com
```

전체 기능 스택 기준 설치 계획 예시:

```bash
g7inst plan \
  --domain example.com \
  --app gnuboard7 \
  --web-server nginx \
  --php-version 8.3 \
  --database mysql \
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

PHP 8.5 선택 모드:

```bash
g7inst plan --domain example.com --php-version 8.5
```

`bootstrap.sh`는 GitHub Release의 최신 바이너리를 다운로드하고, `checksums.txt`로 SHA256 검증 후 `/usr/local/bin/g7inst`에 설치합니다.

## 지원 환경

- OS: Ubuntu 24.04 LTS
- 권한: `doctor`는 일반 사용자 가능, `install`은 root 또는 sudo 필요
- 아키텍처:
  - `x86_64-unknown-linux-musl`
  - `aarch64-unknown-linux-musl`
- PHP: 기본 `8.3`, 선택 옵션 `8.5`
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

VM 안에서 웹 컨트롤러를 시작합니다.

```bash
ssh -t g7-test 'chmod +x /tmp/g7inst && sudo /tmp/g7inst setup --local-test --domain g7-test.local'
```

터미널에 출력된 URL을 VM 내부 브라우저에서 열거나 SSH 터널로 연결합니다.

```bash
ssh -L 7717:127.0.0.1:7717 g7-test
```

```text
http://127.0.0.1:7717/?token=...
```

웹 UI에서 서버 계정 로그인, 서버 점검, 설치 방식 선택, 사양 확정, 패키지 설치, 결과 확인 순서로 진행합니다. 서버 점검을 통과해야 설치 방식 단계로 넘어갈 수 있고, 사양 확정 단계에서 계획 생성이 성공해야 `이 사양으로 진행` 버튼이 열립니다. `패키지 설치 시작`을 누르면 확인 창에서 한 번 더 묻습니다. 각 단계는 `이전` 버튼과 브라우저 뒤로가기로 돌아갈 수 있습니다.

기본 화면은 초보자용 마법사이며, 어려운 용어 옆의 `?` 버튼으로 도움말을 확인할 수 있습니다. 상세 설정은 접힘 영역에 숨겨져 있고, 라이트/다크 테마를 선택할 수 있습니다. CSS/JS는 빌드 버전 쿼리와 no-cache 헤더를 사용하므로 새 바이너리 재설치 후 예전 화면이 남지 않게 처리합니다.

웹 UI의 앱 선택은 `그누보드7`, `WordPress`, `Laravel`을 제공합니다. 선택값은 코어 `AppProfile`로 정규화되어 계획/리포트에 앱 요구사항, 앱 문서 루트, 후속 설치 단계를 표시합니다. 실제 앱 소스 다운로드, DB 설정 파일 생성, vhost 연결은 후속 설치 단계에서 구현합니다.

웹 컨트롤러는 token URL을 처음 정상으로 연 클라이언트 IP로 접속을 잠급니다. SSH 터널 접속이면 보통 `127.0.0.1`로 잠기며, 다른 IP에서 같은 URL이나 쿠키를 재사용하면 403과 허용 IP/요청 IP가 표시됩니다. 잘못 잠겼으면 `Ctrl+C`로 웹 컨트롤러를 종료하고 다시 실행하세요.

설치 중 오류가 나면 웹 UI의 상세 로그와 결과 리포트 영역에 서버 오류, 해결 힌트, 실패한 패키지/서비스/포트 검증 항목이 같이 표시됩니다. PHP 8.5 패키지가 apt 소스에 없으면 PHP 8.3 조합으로 다시 시도하세요.

자동화 방식으로 한 번에 확인하려면:

```bash
scripts/g7-test-smoke.sh
```

테스트 후 installer가 만든 파일만 초기화하려면:

```bash
ssh -t g7-test 'sudo /tmp/g7inst reset --yes'
```

`reset --yes`는 `/etc/g7-installer`, `/var/lib/g7-installer`, `/var/log/g7-installer`, `/var/backups/g7-installer`, `/var/www/g7` 같은 installer 메타데이터와 준비 흔적을 삭제합니다. 예전 명령어 충돌을 막기 위해 `/usr/local/bin/g7`, `/tmp/g7`도 함께 삭제합니다. 현재 실행 중인 `g7inst` 바이너리와 이미 설치된 apt 패키지는 삭제하지 않습니다.

패키지 설치 직후 전체 되돌리기:

```bash
ssh -t g7-test 'sudo /tmp/g7inst rollback --yes'
```

`rollback --yes`는 설치 전 패키지 기준 정보가 있고 운영 콘텐츠가 없을 때만 실행됩니다. 이미 운영 중인 사이트, 비어 있지 않은 웹루트, DB/vhost/G7 배치 같은 후속 단계 흔적이 있으면 중단합니다. 설치 전부터 있던 패키지는 보존합니다.

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

원격 공개 바인딩은 명시 옵션이 필요합니다. 기본값은 로컬 전용입니다.

```bash
sudo g7inst setup --bind 0.0.0.0:7717 --allow-remote
```

현재 빌드는 원격 공개 바인딩에서 서버 계정 비밀번호 로그인을 차단합니다. 서버 계정 로그인과 패키지 설치 액션은 기본 `127.0.0.1` 바인딩에서 SSH 터널로 접속해 사용하세요.

### 3. 자동화/디버그 명령

대화형 대신 옵션을 직접 지정할 수 있습니다.

```bash
g7inst plan \
  --domain example.com \
  --app gnuboard7 \
  --web-server nginx \
  --php-version 8.3 \
  --database mysql \
  --site-user g7 \
  --web-root-mode public-html \
  --redis enable \
  --security-profile standard \
  --ssh-policy audit-only \
  --mail-mode none

sudo g7inst install \
  --domain example.com \
  --app gnuboard7 \
  --web-server nginx \
  --php-version 8.3 \
  --database mysql \
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
PHP-FPM: 8.3, 8.5
database: mysql, mariadb
database version: Ubuntu apt default, MySQL 8.0 family, MySQL 8.4 LTS family (web UI 표시/검증용)
app profile: gnuboard7, wordpress, laravel (계획/리포트의 앱 요구사항 기준)
site user: any safe Linux account name, default g7
web root mode: public-html, www, system, custom
redis: enable, disable
mail: none, smtp-relay, local-postfix (웹 UI 표시는 메일 발송 안 함, 외부 SMTP로 발송, 서버 Postfix로 발송)
security profile: standard, hardened, audit-only
ssh policy: audit-only, harden
mode: public, local-test
```

현재 `install`은 apt 패키지 설치, 기본 서비스 시작, 패키지/서비스/포트 검증까지 수행합니다. 생성되는 핵심 파일은 `/etc/g7-installer/config.toml`, `/var/lib/g7-installer/state.json`, `/var/lib/g7-installer/owned-files.json`, `/var/log/g7-installer/report.json`입니다. 앱 프로파일, 앱 문서 루트, 앱별 요구사항은 config/report에 기록하지만 아직 앱 디렉터리를 생성하거나 소스를 배치하지 않습니다.

## 현재 명령

```bash
sudo g7inst setup
g7inst doctor
g7inst plan --domain example.com [options]
sudo g7inst install --domain example.com [options]
g7inst status
g7inst logs
sudo g7inst reset --yes
sudo g7inst rollback --yes
sudo g7inst update
sudo g7inst self-update
```

주요 옵션:

```text
--local-test
--app gnuboard7|wordpress|laravel
--web-server nginx|apache
--php-version 8.3|8.5
--database mysql|mariadb
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

`setup` 전용 옵션:

```text
--bind 127.0.0.1:7717
--allow-remote
```

## 현재 구현된 기능

- `g7inst setup`
  - `axum` 기반 로컬 웹 컨트롤러 실행
  - 기본 바인딩 `127.0.0.1:7717`, 원격은 `--allow-remote` 필요
  - 실행 토큰으로 최초 세션 생성
  - 서버 계정 로그인 후 root 또는 sudo 가능 계정만 설치 액션 허용
  - 로그인 실패 3회 후 60초 제한
  - 원격 공개 바인딩에서는 서버 계정 비밀번호 로그인 차단
  - CSRF 토큰과 HttpOnly 세션 쿠키 사용
  - WebSocket으로 Live log, 단계별 진행 상태, 설치/되돌리기 퍼센트 표시
  - 서버 점검, 옵션 선택, 계획 확인, 패키지 설치, 리포트, 리셋, 패키지 되돌리기 화면 제공
  - 뒤로가기/새로고침 후 브라우저 세션과 서버 report 기준으로 마법사 단계 복원
  - `/api/recovery`로 설치기 메타데이터와 안전한 rollback 가능 여부 확인
  - HTML 결과 리포트, 패키지/서비스/포트/DNS/메일/Certbot 검증 목록 제공
  - 선택 앱 프로파일의 요구사항과 후속 설치 단계를 결과 리포트에 표시
  - 설치 전 패키지 기준을 `신규 설치`, `기존 보존` 라벨로 표시
  - 현재 패키지 설치 단계에서는 vhost/app이 없으므로 도메인 접속 링크는 비활성 안내로 표시
  - 정적 CSS/JS 자산에 빌드 버전 쿼리와 no-cache 헤더 적용
  - reset 성공 후 server check 자동 재실행

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
  - `--web-server nginx|apache`, `--database mysql|mariadb` 선택 반영
  - `--app gnuboard7|wordpress|laravel` 선택 반영
  - `--site-user`, `--web-root-mode`, `--web-root` 선택 반영
  - PHP 8.3 기본, PHP 8.5 선택 옵션 반영
  - www canonical 정책 반영
  - Redis/cache/session/queue 준비 항목 반영
  - DB/Redis localhost-only, 랜덤 DB 비밀번호 정책, SSH audit/harden 정책 표시
  - SMTP relay/local Postfix 메일 발송 준비 항목 반영
  - DNS/IP, SMTP outbound, Certbot renewal, rollback, 설정보존 gate 표시
  - 앱별 PHP 확장, Composer/Node, writable path, queue/scheduler/Reverb 같은 요구사항을 `planned`/`deferred` 상태로 표시

- `sudo g7inst install --domain example.com`
  - 현재는 apt 패키지 설치 단계입니다.
  - 설치 전 각 apt 패키지의 설치 여부를 `preinstall_package_checks`로 기록합니다.
  - `apt-get update`, 후보 패키지 확인, `apt-get install -y --no-install-recommends`를 비대화식으로 실행합니다.
  - Nginx/Apache, PHP-FPM, MySQL/MariaDB, Redis, Certbot timer 같은 기본 서비스를 `systemctl enable --now`로 시작합니다.
  - 설치 후 `dpkg-query`, `systemctl is-active`, `ss` 기반으로 패키지/서비스/포트를 검증합니다.
  - 실제 도메인 모드에서는 `curl`로 서버 공인 IPv4를 확인하고 `getent ahostsv4`로 도메인/www A 레코드가 같은 IP를 가리키는지 리포트합니다.
  - `smtp-relay` 모드에서는 선택한 SMTP host/port TCP 연결성을 리포트합니다.
  - `local-postfix` 모드에서는 Postfix 서비스 활성 상태를 리포트합니다.
  - 실제 도메인 모드에서는 Certbot 패키지와 `certbot.timer` 상태를 리포트하고, 인증서가 이미 있으면 `certbot renew --dry-run`을 실행합니다.
  - 인증서 신규 발급은 vhost/app 단계에서 HTTP-01 challenge를 안전하게 제공할 수 있을 때 실행합니다.
  - 선택 옵션을 `/etc/g7-installer/config.toml`에 저장합니다.
  - 선택한 앱 프로파일, 앱 문서 루트, 앱별 요구사항은 config/report에 기록하지만 아직 디렉터리를 생성하지 않습니다.
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

- `sudo g7inst rollback --yes`
  - 설치 직후 패키지 되돌리기용
  - `packages-installed` 단계에서만 실행
  - 설치 전 패키지 기준 정보가 없거나 알 수 없는 패키지가 있으면 중단
  - 설치 전 이미 있던 패키지와 그 서비스는 보존
  - 웹루트가 비어 있지 않거나 app/vhost/DB/cert/SSH 후속 단계 흔적이 있으면 중단
  - 관리 서비스 `systemctl disable --now`, 설치 패키지 `apt-get purge -y --auto-remove`, installer 메타데이터 리셋 순서로 진행
  - `--dry-run`으로 서비스, 패키지, 메타데이터 대상을 미리보기 가능

## 아직 미구현

- Nginx vhost 렌더링 및 적용
- PHP-FPM 설정
- MySQL/MariaDB 설정
- DB app user/password 랜덤 생성 및 root-only 저장
- WordPress/G7/Laravel 앱 소스 다운로드 및 압축 해제
- Composer/Node/Bun 기반 앱별 후처리
- 앱별 `.env`, `wp-config.php`, Laravel `APP_KEY` 생성
- 앱별 writable path 권한 적용
- 앱별 health check 실행
- Certbot 인증서 발급
- Certbot 신규 인증서 발급 후 vhost TLS 적용
- IPv6 AAAA 공인 IP 대조
- Redis 하드닝
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
https://github.com/jiwonpapa/g7-installer/releases/tag/v0.2.7
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

## 서버 운영 하네스

운영 하네스는 disposable Ubuntu 24.04 테스트 VPS에서만 실행합니다.

Release 바이너리 기준:

```bash
G7_OPS_CONFIRM_DISPOSABLE=1 \
G7_OPS_HOST=g7-test \
G7_OPS_SOURCE=release \
G7_OPS_VERSION=v0.2.7 \
scripts/ops-harness.sh
```

로컬 빌드 바이너리 기준:

```bash
G7_OPS_CONFIRM_DISPOSABLE=1 \
G7_OPS_HOST=g7-test \
G7_OPS_SOURCE=local \
scripts/ops-harness.sh
```

검증 항목:

- Ubuntu 24.04 서버 확인
- bootstrap 또는 로컬 바이너리 배치
- `doctor` fresh install 허용 확인
- `plan --local-test` 생성
- `install --local-test` 실행
- `/var/log/g7-installer/report.json` 계약 검증
- 설치 후 `doctor` fresh install 차단 확인
- `rollback --dry-run` 확인
- `rollback --yes` 실행
- 새로 설치된 패키지 전체 제거 확인
- installer metadata 파일과 installer 전용 디렉터리 제거 확인
- rollback 후 재설치 가능 여부 확인

## 개발 검증

커밋 전 기본 검증:

```bash
bash -n scripts/*.sh
scripts/web-static-smoke.sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
cargo llvm-cov --workspace --all-targets --summary-only --fail-under-lines 75
(cd web && bun install --frozen-lockfile && bun run build)
```

한 번에 실행:

```bash
scripts/quality-gate.sh
```

현재 커버리지 하한은 line coverage `75%`입니다. GitHub Actions 강제 게이트는 workflow 권한이 있는 토큰으로 추가해야 합니다.
