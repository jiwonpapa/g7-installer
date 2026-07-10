# G7 Installer Public Beta 스펙

## 1. 목적

G7 Installer는 새 Ubuntu VPS에 그누보드7/WordPress 중심의 PHP 웹앱 운영 환경을 자동 구성하는 Rust 기반 서버 CLI입니다.

목표는 초보 사용자가 VPS에 접속한 뒤 최소 명령으로 그누보드7 또는 WordPress 설치 기반을 완료하게 하는 것입니다.

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
sudo g7inst setup --domain example.com
```

## 2. 레포 분리 원칙

G7 Installer는 그누보드7 본체와 별도 레포로 운영합니다.

현재 레포:

```text
jiwonpapa/g7-installer
```

분리 이유:

- 설치기는 서버 패키지, systemd, Nginx, Certbot 등 OS 변경을 담당합니다.
- 그누보드7 본체는 CMS 코드이며 책임 범위가 다릅니다.
- 설치기 릴리스 주기와 G7 본체 릴리스 주기가 다릅니다.
- root 권한으로 실행되는 도구라 별도 보안 리뷰와 배포 체계가 필요합니다.
- G7 본체 레포에 서버 자동화 코드를 넣으면 유지보수 경계가 흐려집니다.

G7 본체와의 관계:

- 그누보드7은 GitHub 공식 최신 안정 Release를 조회·clone하고 배포 후 Git 필수 파일을 재검증한 뒤 `.env.example`에서 소유자 전용 `0600` 권한의 `.env`를 준비하고 공식 브라우저 `/install`로 인계합니다. Composer/Vendor, 관리자 계정, 확장과 마이그레이션은 G7 공식 설치기가 담당합니다.
- WordPress는 최신 배포 zip을 받아 설치 화면으로 연결합니다.
- 각 앱 본체 레포를 직접 수정하지 않습니다.

## 3. 지원 범위

### 3.1 MVP 지원

```text
OS: Ubuntu 24.04 LTS
권한: root 또는 sudo
웹서버: Nginx 기본, Apache 선택 옵션
PHP: PHP-FPM 8.5 기본, 8.3 선택 옵션
DB: MySQL 기본, MariaDB 선택 옵션
앱 프로파일: gnuboard7 기본, WordPress 선택 옵션
HTTPS: Certbot Let's Encrypt
Cache/Queue: Redis 기본 지원
메일: local Postfix 기본, SMTP relay 선택
설치 대상: 새 VPS
```

PHP 8.5는 `ppa:ondrej/php` apt 소스를 자동 추가합니다. PHP 8.3은 Ubuntu 24.04 기본 apt 소스를 사용합니다.

Laravel, Octane, FrankenPHP는 내부 실험 프로필로 남아 있지만 Public Beta 사용자 흐름과 문서에서는 지원 범위로 보지 않습니다.

### 3.2 MVP 제외

```text
기존 운영 서버 자동 설치
cPanel, Plesk, Cafe24 같은 관리형/공유호스팅
다중 사이트 자동 병합
운영 DB 마이그레이션
서버 이전
장기 운영 웹패널
다중 서버 관리 UI
```

기존 서비스가 감지되면 설치를 중단하고 `doctor` 리포트만 제공합니다.

## 4. 사용자 명령

### 4.1 bootstrap

```bash
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | sudo bash
```

역할:

- OS/아키텍처 확인
- 최신 `g7inst` 바이너리 다운로드
- checksum 검증
- `/usr/local/bin/g7inst` 설치

bootstrap은 최소 Bash만 사용합니다. 실제 설치 로직은 Rust 바이너리에서 수행합니다.

### 4.2 CLI 명령

```bash
sudo g7inst setup --domain example.com
g7inst doctor
g7inst plan --domain example.com [options]
sudo g7inst install --domain example.com [options]
g7inst status
g7inst logs
sudo g7inst reset --yes
```

주요 옵션:

```bash
--local-test
--web-server nginx|apache
--app gnuboard7|wordpress
--php-version 8.3|8.5
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

명령 설명:

| 명령 | root 필요 | 설명 |
| --- | --- | --- |
| `doctor` | 부분 권장 | 서버 설치 가능 여부 진단, 기본 읽기 전용 |
| `plan` | 아니오 | 설치 전 변경 계획 출력 |
| `setup` | 예 | 로컬 웹 컨트롤러 기반 설치 준비 |
| `install` | 예 | 새 VPS 설치 실행 |
| `status` | 아니오 | 설치 상태 요약 출력. Public Beta에서는 제한적 진단용 |
| `logs` | 아니오 | 설치 로그 위치 출력 |
| `reset` | 예 | installer가 만든 계정, DB/DB 계정, 서비스, 웹루트/설정 파일, 새로 설치한 패키지, 메타데이터 제거. 기존 Let's Encrypt 인증서는 보존 우선 |
| `rollback` | 예 | 앱/DB/인증서 생성 전 초기 실패를 되돌리는 제한적 복구 |

`update`와 `self-update` 명령은 CLI 표면에는 남아 있지만 현재 Public Beta에서는 구현 완료 기능으로 문서화하지 않습니다.

`--local-test`는 개발/레거시 VM 검증 전용입니다. 공개 사용자 흐름은 실제 도메인과 DNS/IP 검증 기준으로 진행합니다.

## 5. 설치 흐름

`sudo g7inst install --domain example.com` 실행 시:

1. root 권한 확인
2. lock 획득
3. Ubuntu 버전 확인
4. fresh server 검사
5. 도메인 DNS가 서버 IP를 가리키는지 확인
6. www canonical 정책 확인
7. 80/443 포트 점유 확인
8. SMTP outbound 포트 확인
9. 설치 계획 출력
10. rollback/report/설정보존 준비
11. 사용자 확인
12. apt repository 업데이트
13. 선택 웹서버 설치
14. PHP-FPM 및 필수 PHP 확장 설치
15. Redis 설치 및 localhost-only hardening
16. 선택 DB 설치
17. DB 및 DB 사용자 생성, 앱 DB 비밀번호 적용
18. 선택 앱 소스 다운로드
19. 그누보드7 최신 안정 Release 검증·공식 `/install` 인계 또는 WordPress 최신 설치 화면 준비
20. 선택한 웹루트(`/home/<site-user>/public_html`, `/home/<site-user>/www`, `/var/www/<domain>`, custom)에 배치
21. `.env` 생성 및 앱 런타임 설정
22. 메일 발송 설정 반영
23. 파일 권한 설정
24. 선택 웹서버 vhost 생성
25. DNS/IP 조건이 맞으면 Certbot HTTPS 발급 또는 기존 인증서 재사용
26. Certbot 자동갱신 timer 확인
27. queue worker, scheduler, reverb systemd unit 파일 준비
28. 서비스 재시작
29. HTTP/HTTPS/mail/Redis/DB localhost bind smoke test
30. 설치 결과와 설정 안내서 출력
31. 웹 UI 7단계 세부 설정 액션 패널에서 후속 설정 확인

## 6. fresh server 검사

설치 중단 조건:

- `/etc/nginx/sites-enabled/*` 또는 `/etc/apache2/sites-enabled/*`에 기존 사이트 설정 존재
- 선택하지 않은 웹서버가 실행 중
- 80 또는 443 포트가 기존 프로세스에 의해 점유됨
- 선택한 웹루트가 비어 있지 않거나 선택 계정 소유가 아님
- legacy `/var/www/g7` 테스트 경로가 installer 소유권 없이 존재
- `/etc/g7-installer/owned-files.json` 없이 G7 관련 파일이 존재
- 기존 Certbot 인증서가 동일 도메인으로 존재하지만 installer 소유가 아님
- `/etc/g7-installer/state.json` 기준으로 다른 설치가 진행 중
- 도메인 A/AAAA 레코드가 VPS 공인 IP와 불일치
- 요청한 www host가 VPS 공인 IP와 불일치
- SMTP outbound 포트가 차단됨
- Redis가 외부 공개 bind로 설정됨
- DB가 외부 인터페이스에서 접근 가능함
- SSH hardening이 현재 접속 세션을 끊을 위험이 있음

중단 메시지는 사용자가 이해할 수 있어야 합니다.

예:

```text
기존 웹 서비스가 감지되어 설치를 중단했습니다.
이 도구는 새 Ubuntu VPS 전용입니다.

확인된 항목:
- nginx 실행 중
- /etc/nginx/sites-enabled/default 존재
- 443 포트 사용 중

서버 상태 확인:
g7inst doctor
```

## 7. 파일 경로

```text
/usr/local/bin/g7inst
/etc/g7-installer/config.toml
/var/lib/g7-installer/state.json
/var/lib/g7-installer/owned-files.json
/var/lib/g7-installer/rollback.json
/var/log/g7-installer/install.log
/var/log/g7-installer/report.json
/var/log/g7-installer/setup-guide.md
/var/backups/g7-installer
/home/<site-user>/public_html
/etc/nginx/sites-available/g7.conf
/etc/nginx/sites-enabled/g7.conf
/etc/apache2/sites-available/g7.conf
/etc/apache2/sites-enabled/g7.conf
```

`owned-files.json`은 installer가 만든 파일만 추적합니다. 추적되지 않은 운영 파일은 자동 수정하지 않습니다.

## 8. Rust 기술 스택

### 8.1 기본

```text
Rust stable
Rust 2024 edition
타깃: x86_64-unknown-linux-musl
타깃: aarch64-unknown-linux-musl
```

서버에는 Rust toolchain을 설치하지 않습니다.

### 8.2 주요 crates

| crate | 용도 |
| --- | --- |
| `clap` | CLI 명령/옵션 파싱 |
| `serde` | 설정/상태 직렬화 |
| `serde_json` | `state.json`, `owned-files.json` |
| `toml` | `config.toml` |
| `thiserror` | typed error |
| `miette` | 사용자 친화적 에러 리포트 |
| `tracing` | 구조화 로그 |
| `tracing-appender` | 파일 로그 |
| `axum` | `setup` 웹 컨트롤러 |
| `tokio` | async HTTP/WebSocket runtime |
| `tower-http` | HTTP middleware |
| `reqwest` | G7 릴리스/체크섬 다운로드 |
| `sha2` | checksum 검증 |
| `zip` | 릴리스 ZIP 압축 해제 |
| `tempfile` | staging 디렉토리 |
| `indicatif` | 진행률 표시 |
| `which` | 명령 존재 확인 |
| `fs2` | lock file |
| `nix` 또는 `rustix` | uid, signal, process 정보 |

## 9. 코드 구조

현재 workspace:

```text
crates/
  g7-cli/
    src/main.rs
    src/web_setup/
      api.rs
      defaults.rs
      provision_actions.rs
      recovery.rs
      render.rs
      routes.rs
      session.rs
      tests.rs
  g7-core/
    src/app_profile.rs
    src/defaults.rs
    src/installer_paths.rs
    src/commands/
      doctor.rs
      install.rs
      logs.rs
      plan.rs
      reset.rs
      rollback.rs
      status.rs
      update.rs
      self_update.rs
      install/
        apps.rs
        database.rs
        orchestrator.rs
        packages.rs
        report.rs
        runtime.rs
        site.rs
        tests.rs
        tls.rs
        vhost.rs
        apps/
          gnuboard7.rs
          laravel.rs
          runtime.rs
          wordpress.rs
      plan/
        builder.rs
        defaults.rs
        normalize.rs
        provisioning.rs
        resources.rs
        sizing.rs
        types.rs
  g7-system/
    src/command.rs
    src/apt.rs
    src/nginx.rs
    src/php.rs
    src/database.rs
    src/systemd.rs
    src/ufw.rs
    src/certbot.rs
  g7-release/
    src/download.rs
    src/checksum.rs
    src/extract.rs
  g7-state/
    src/state.rs
    src/lock.rs
    src/owned_files.rs
  g7-render/
    src/templates.rs
templates/
  nginx/g7.conf.tera
scripts/
  bootstrap.sh
  lightsail-init.sh
  quick-gate.sh
  quality-gate.sh
  ops-harness.sh
```

## 10. 에러/안전 정책

개발 규칙:

```text
unwrap 금지
expect 금지
panic 금지
외부 명령 실패는 전부 typed error로 변환
변경 작업 전 plan 생성
변경 작업 후 state 기록
installer 소유 파일만 자동 수정
기존 운영 파일은 자동 수정 금지
```

사용자 에러 메시지 형식:

```text
문제:
  443 포트가 이미 사용 중입니다.

원인:
  기존 웹서버 또는 다른 서비스가 HTTPS 포트를 점유하고 있습니다.

다음 조치:
  g7inst doctor 명령으로 점유 프로세스를 확인하세요.
```

## 11. 상태/재실행

설치 단계는 `state.json`에 기록합니다.

예:

```json
{
  "version": 1,
  "install_id": "20260703-abc123",
  "domain": "example.com",
  "phase": "nginx_configured",
  "completed_steps": [
    "preflight",
    "packages_installed",
    "database_created",
    "release_extracted",
    "nginx_configured"
  ]
}
```

재실행 시:

- 같은 domain이면 이어서 진행 가능
- 다른 domain이면 중단
- lock이 남아 있으면 오래된 lock 여부 확인 후 안내

## 12. 보안

- root 권한은 서버를 변경하는 명령에서만 요구합니다. Public Beta의 주요 변경 명령은 `setup`, `install`, `reset`, `rollback`입니다.
- SSH 비밀번호를 저장하지 않습니다.
- SSH 설정은 기본 `audit-only`입니다. 포트 변경과 hardening은 현재 접속 세션 보존 검증 후에만 수행합니다.
- DB root 비밀번호를 로그에 남기지 않습니다.
- 앱 DB 비밀번호는 기본값을 두지 않고 랜덤 생성합니다.
- `.env` 내용은 로그에 남기지 않습니다.
- Redis와 DB는 localhost/unix socket 전용으로 구성하고 외부 공개를 금지합니다.
- 다운로드 파일은 checksum 검증 후 사용합니다.
- bootstrap은 바이너리 서명 또는 checksum 검증을 필수로 합니다.
- `curl | sudo bash`는 공개 bootstrap 코드와 checksum 검증으로 신뢰를 보완합니다.

## 13. 릴리스/배포

GitHub Releases 산출물:

```text
g7inst-x86_64-unknown-linux-musl
g7inst-aarch64-unknown-linux-musl
checksums.txt
```

패키징:

- `/usr/local/bin/g7inst` 단일 바이너리
- bootstrap script는 `main/scripts/bootstrap.sh`에서 제공하고 latest release를 감지
- 설치 후 `g7inst --version` 출력

## 14. 테스트 전략

### 14.1 개발 빠른 테스트

```bash
bash scripts/quick-gate.sh
```

### 14.2 전체 품질 게이트

```bash
bash scripts/quality-gate.sh
```

`quality-gate.sh`는 quick gate를 먼저 실행한 뒤 전체 `cargo test`, clippy, doc, coverage, web build를 수행합니다.

### 14.3 통합 테스트

- fake command runner로 apt/nginx/systemd 명령 시뮬레이션
- fresh server 판정 테스트
- plan 생성 스냅샷 테스트
- template render 테스트
- state resume 테스트

### 14.4 실제 smoke

릴리스 전 Ubuntu 24.04 fresh VPS 또는 VM에서:

```bash
sudo g7inst setup --domain test.example.com
sudo g7inst install --domain test.example.com
g7inst doctor
curl -I https://test.example.com
```

운영 하네스는 실제 도메인과 staging Let's Encrypt를 기본으로 사용합니다. production Let's Encrypt 발급은 명시적으로 허용한 릴리스 검증 때만 사용합니다.

## 15. MVP 완료 기준

Public Beta는 아래 조건을 만족해야 배포 가능합니다.

- Ubuntu 24.04 fresh VPS에서 설치 성공
- `sudo g7inst setup --domain example.com` 동작
- `sudo g7inst install --domain example.com` 동작
- 기존 운영 서버 감지 시 안전 중단
- 선택 웹서버/PHP-FPM/선택 DB/공개 앱 프로필 설치 기반 완료
- DNS/IP 조건이 맞을 때 HTTPS 발급 또는 기존 인증서 재사용 성공
- 설치 로그 저장
- 설치기 생성 설정/상태 복구 매니페스트 저장
- `g7inst doctor`, 제한적 `g7inst status`, `g7inst logs` 동작
- 재실행 시 중복 설치로 서버를 망가뜨리지 않음

## 16. v1 이후

- `g7inst backup`
- `g7inst restore`
- `g7inst migrate-server`
- existing server 읽기 전용 리포트 강화
- cloud-init 템플릿 제공
- DigitalOcean/Linode/Vultr marketplace image
- 관리형 백업/모니터링 SaaS 연동
