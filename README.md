# G7 Installer

Ubuntu VPS에 `g7inst`를 설치하고 웹 마법사로 그누보드7/WordPress용 서버 구성과 사이트 프로비저닝을 진행하는 도구입니다.

> 현재 공개 릴리스는 Public Beta입니다. 새 Ubuntu 24.04 VPS 전용으로 `g7inst` 설치, 서버 점검, 웹 마법사, apt 패키지 설치, Nginx/Apache 도메인 연결 설정(vhost), PHP/DB 사양 튜닝, DB 앱 계정 생성, Let's Encrypt 인증서 발급/갱신 검증, 그누보드7 브라우저 설치 화면 준비, WordPress 설치 화면 준비, 7단계 세부 설정 액션 패널과 카드별 재시작/점검, 상세 설정 안내서 저장까지 검증합니다.

## 무엇부터 보면 되나요?

| 상황 | 문서 |
| --- | --- |
| Mac/Windows에서 명령만 따라 설치 | [따라하기식 설치 매뉴얼](docs/copy-paste-install.md) |
| 처음 설치하는 초보자 | [초보용 설치 안내](docs/beginner-install.md) |
| Lightsail 화면을 보며 상세 설정 | [Lightsail 상세 안내](docs/lightsail-ubuntu24-setup-guide.md) |
| 이미 서버와 도메인이 준비됨 | [바로 설치 UI 열기](#바로-설치-ui-열기) |
| 개발/검증 기준 확인 | [운영 하네스 감사](docs/ops-harness-audit.md), [거버넌스 통제표](docs/governance-controls.md) |

## 대상 사용자

그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해본 사용자를 기준으로 합니다. 서버 명령은 복사해서 따라 할 수 있게 두고, 웹 UI의 전문 용어는 `?` 도움말에서 짧게 설명합니다.

## 웹 UI 도움말 원칙

- 기본 화면은 `권장 설치`와 실제 도메인 설치 흐름만 보입니다.
- 고급 항목은 `상세 설정` 안에 둡니다.
- 낯선 용어는 항목 옆 `?` 도움말에서 설명합니다.
- 도움말은 원래 기술명과 실제 의미를 함께 적습니다. 예: `vhost`는 `도메인 연결 설정`으로 표시합니다.
- 공개 설치 마법사는 실제 도메인 설치 흐름만 노출합니다.

## 권장 배포 기준

> **권장 인스턴스**
>
> - AWS Lightsail
> - Ubuntu 24.04 LTS
> - 듀얼 스택, 공인 IPv4 포함
> - 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송
> - 방화벽 22, 80, 443만 오픈

무료 크레딧, 무료 기간, 번들 가격은 AWS가 언제든 바꿀 수 있습니다. 실제 과금 기준은 인스턴스 생성 화면과 결제 안내를 확인하세요.

## 메모리 기준 튜닝

설치 계획에는 1GB, 2GB, 4GB, 8GB, 16GB, 32GB 메모리 프리셋과 32GB 초과 공식 기반 프리셋이 포함됩니다. PHP-FPM `max_children`, opcache, DB buffer pool, DB connection, Redis maxmemory, swap, Nginx worker process/connection/buffer, Apache `mpm_event` worker 값을 메모리와 vCPU 등급별로 계산합니다. 현재 실행 단계는 감지된 RAM/vCPU에 맞춰 PHP 런타임, PHP ini, Nginx/Apache vhost, MySQL/MariaDB 튜닝 파일을 적용하고 리포트에 기록합니다.

서버 점검은 총 RAM, 가용 RAM, swap, 루트 디스크 여유 공간, inode 여유를 확인합니다. 최소 기준을 못 채우면 설치를 차단합니다. 1GB급 서버는 Redis와 로컬 Postfix를 기본 해제하고, apt 설치보다 먼저 관리형 swap을 구성합니다.

설치·초기화·되돌리기 명령의 stdout/stderr는 웹 화면에 실시간 표시됩니다. 브라우저 연결이 끊기거나 새로고침되어도 같은 컨트롤러의 최근 로그를 순번 기준으로 다시 받습니다. DB 설정 이후 안전 경계에서 중단된 설치는 복구 패널의 `이어서 진행`으로 재개할 수 있습니다.

## PHP 버전과 apt 소스

- 기본 PHP는 8.5입니다. 설치기가 `software-properties-common`, `ca-certificates`, `lsb-release`를 먼저 설치하고 `ppa:ondrej/php`를 자동 추가한 뒤 다시 `apt update`를 실행합니다.
- PHP 8.3을 선택하면 Ubuntu 24.04 기본 apt 소스를 사용합니다.
- 리포트와 웹 UI에는 `php_source`가 `ubuntu` 또는 `ondrej`로 표시됩니다.

## 공개 지원 범위

- 웹서버는 Nginx 권장, Apache 호환 옵션만 노출합니다.
- 앱 패키지는 그누보드7과 WordPress에 집중합니다.
- FrankenPHP, Octane, Laravel 자동 배포는 내부 실험 프로필로만 남겨 두고 공개 설치 마법사와 사용자 문서에서는 지원하지 않습니다.

## 바로 설치 UI 열기

서버, 도메인, 방화벽 설정이 끝났다면 아래 순서대로 실행합니다.

Lightsail 기본 Ubuntu는 보통 서버 비밀번호 없이 `ubuntu` 계정 SSH 키 접속과 비밀번호 없는 `sudo`로 진행됩니다. 다른 VPS에서 `sudo` 비밀번호를 요구하면 `g7inst setup --domain example.com`을 일반 계정으로 실행해도 설치기가 SSH 터미널에서 `sudo` 재실행을 시도합니다. 비밀번호가 필요하면 터미널에 입력합니다. 웹 UI에는 root/ubuntu 비밀번호를 입력하지 않습니다.

`sudo` 권한이 아예 없으면 웹 UI에서 해결하지 않습니다. root SSH, `su -`, 또는 VPS 콘솔에서 관리자 세션을 얻은 뒤 bootstrap/setup 명령을 실행해야 합니다.

웹 UI의 `사이트 계정 비밀번호`는 별도입니다. 설치기가 만들 `g7` 같은 Linux 사이트 계정의 SFTP/파일관리 비밀번호이며, sudo 권한은 주지 않습니다.

웹 UI 기본 조합은 `Nginx / PHP 8.5 / MySQL(apt mysql-server, UI 기준 8.4 LTS 계열) / www로 통일 / Redis 사용 / 서버 Postfix 발송 / 그누보드7`입니다. 실제 DB 패키지 버전은 Ubuntu apt 소스와 선택한 DB 패키지에 따릅니다.

| 바꿔 넣을 값 | 의미 |
| --- | --- |
| `SERVER_IP` | Lightsail 고정 IP |
| `example.com` | 실제 도메인 |
| `~/.ssh/lightsail_g7inst.pem` | 내려받은 Lightsail SSH 키 경로 |

1. SSH와 sudo 상태를 확인합니다.

내 PC 터미널:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem ubuntu@SERVER_IP
```

서버 접속 후:

```bash
sudo -n true && echo "sudo OK"
g7inst --version
exit
```

SSH alias가 있으면:

```bash
ssh g7installer
```

서버 접속 후:

```bash
sudo -n true && echo "sudo OK"
g7inst --version
exit
```

`sudo OK`가 나오면 비밀번호 없이 설치 준비가 된 상태입니다. 실패하더라도 `sudo` 권한이 있는 서버라면 일반 계정에서 setup을 실행하면 설치기가 sudo로 재실행합니다.

```bash
g7inst setup --domain example.com
```

sudo 비밀번호를 물어보면 SSH 터미널에 입력합니다. sudo 자체가 실패하면 root SSH, `su -`, 또는 VPS 콘솔에서 관리자 권한을 먼저 확보해야 합니다.

필요하면 `g7inst`만 비밀번호 없이 sudo 실행되도록 제한할 수 있습니다. root 또는 sudo 가능한 계정으로 한 번 접속한 뒤 아래처럼 설정합니다.

```bash
sudo visudo -f /etc/sudoers.d/g7inst
```

열린 파일에 서버 접속 계정을 넣습니다. Lightsail 기본 계정이면 보통 `ubuntu`입니다.

```text
ubuntu ALL=(root) NOPASSWD: SETENV: /usr/local/bin/g7inst
```

저장 후 확인합니다.

```bash
sudo chmod 0440 /etc/sudoers.d/g7inst
sudo visudo -cf /etc/sudoers.d/g7inst
sudo -n /usr/local/bin/g7inst --version
```

`ALL=(ALL) NOPASSWD: ALL`처럼 모든 명령을 비밀번호 없이 여는 설정은 권장하지 않습니다. 설치가 끝난 뒤 이 예외가 필요 없으면 `sudo rm /etc/sudoers.d/g7inst`로 제거합니다.

2. 필요하면 VPS 백업/스냅샷을 준비합니다.

```text
스냅샷/백업은 비용과 시간이 들 수 있습니다.
신규 서버 테스트라면 필수 단계가 아닙니다.
이미 운영 데이터가 있거나 재설치 리스크가 있으면 먼저 백업 정책을 확인하세요.
```

3. SSH 터널을 열고 서버에 접속합니다.

```bash
ssh -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

SSH alias가 있으면:

```bash
ssh -L 7717:127.0.0.1:7717 g7installer
```

4. 같은 터미널 창에서 설치 UI를 시작합니다.

```bash
sudo g7inst setup --domain example.com
```

5. 브라우저에서 접속 확인 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

`7717/tcp`는 외부에 열지 않습니다. SSH 터널로만 접속합니다. 설치 중에는 이 터미널 창을 닫지 않습니다.

웹 UI에서 `사이트 계정`과 `사이트 계정 비밀번호`를 입력하면 설치기가 `/home/계정/public_html` 웹루트를 만들고 `계정:www-data` 소유권으로 맞춥니다. Nginx/Apache는 PHP-FPM pool을 이 사이트 계정 기준으로 연결합니다.

웹 UI는 `패키지 설치/검증 -> 사이트 계정/웹루트 -> 웹서버 vhost/HTTP 검증 -> PHP/런타임 튜닝 -> DB 튜닝/계정 생성 -> SSL 인증서/HTTPS 검증 -> 웹앱 파일 배치 -> 리포트 생성 -> 세부 설정 액션 패널` 순서로 진행합니다. 한 단계라도 실패하면 다음 단계로 넘어가지 않고 중단 리포트를 보여줍니다.

그누보드7은 GitHub 공식 최신 안정 Release를 매번 새로 조회·clone하고 Git 무결성과 필수 빌드 파일을 검증한 뒤 공식 브라우저 `/install`로 인계합니다. Composer/Vendor, `.env`, 관리자 계정, 확장과 마이그레이션은 G7 공식 설치기가 처리하며 `g7inst`는 사전 NPM 빌드나 Artisan 설치 명령을 실행하지 않습니다. WordPress는 공식 `latest.zip`을 매번 내려받아 설치 화면으로 이어집니다.

재설치 초기화는 installer가 만든 사이트 계정의 서비스, 프로세스와 SSH/SFTP 세션을 종료한 뒤 계정을 삭제합니다. Ubuntu 기본 SSH 계정, root로 실행 중인 웹 컨트롤러, Mac의 SSH 터널과 보존 대상 Let's Encrypt 인증서는 종료하거나 삭제하지 않습니다.

설치가 끝나면 웹 UI 결과 리포트와 서버의 `/var/log/g7-installer/setup-guide.md`를 확인합니다. 이 Markdown 안내서에는 웹루트, PHP 런타임, DB 설정, 인증서, 앱 systemd unit, 주요 `systemctl` 명령, 비밀 파일 위치가 정리됩니다. 웹 UI에서는 리포트 JSON, 요약 TXT, 설정 안내서 MD를 바로 저장할 수 있습니다. PDF가 필요하면 브라우저 인쇄/PDF 저장으로 내보내는 방식을 권장합니다.

`/var/backups/g7-installer/manifest.json`은 설치기가 만든 설정/상태/소유 파일을 추적하는 복구 매니페스트입니다. DB 덤프나 웹루트 운영 데이터 백업이 아니므로 실제 운영 백업은 별도 도구나 VPS 스냅샷으로 처리합니다.

## 시작 스크립트

Lightsail `시작 스크립트 추가`에는 아래만 넣습니다.

```sh
#!/bin/sh
set -eu
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh -o "$tmp"
bash "$tmp"
```

이 스크립트는 `g7inst` 설치까지만 합니다. OS 업데이트, swap, 웹서버, PHP, DB, Redis, Certbot, 앱 설치는 `g7inst setup` 웹 UI가 처리합니다. UFW·fail2ban 설치와 방화벽 규칙 변경은 범위 밖이며 VPS 제공자 설정 또는 별도 유지보수 앱에서 관리합니다.

## 기본 명령

```bash
g7inst --version
g7inst doctor
sudo g7inst resume
sudo g7inst setup --domain example.com
sudo g7inst rollback --yes
sudo g7inst reset --yes
```

`rollback`은 앱/DB/인증서 생성 전의 초기 실패를 되돌리는 용도입니다. 운영 중인 사이트 백업 복구 기능이 아닙니다.

`resume`은 DB 설정까지 끝난 설치가 TLS 또는 앱 배치 단계에서 중단됐을 때 저장된 리포트와 비밀 파일을 읽어 안전한 후반 단계만 다시 실행합니다. 패키지/계정/DB 이전 단계는 자동 재개하지 않으며 `status` 결과에 따라 rollback/reset을 사용합니다.

`reset --yes`는 이 설치기가 만든 사이트 계정, 웹루트/설정 파일, 앱 systemd unit, DB/DB 계정, 새로 설치한 apt 패키지, installer 메타데이터를 제거해 같은 신규 VPS에서 다시 설치를 시도할 수 있게 합니다. `/etc/letsencrypt/live/*`에 이미 발급된 Let's Encrypt 인증서가 있으면 중복 발급 제한을 피하기 위해 certbot 계열 패키지와 인증서 파일을 보존하고, 재설치 때 기존 인증서를 우선 재사용합니다. 기존 운영 서버 보존 기능이 아니라 신규 VPS 전용 재설치 초기화입니다. 운영 데이터가 있으면 실행 전 VPS 백업/스냅샷 비용과 복구 시간을 확인하세요.

## 열어야 할 포트

| 포트 | 용도 | 공개 |
| --- | --- | --- |
| 22/tcp | SSH | 가능하면 내 IP만 |
| 80/tcp | HTTP, 인증서 발급 | 전체 |
| 443/tcp | HTTPS | 전체 |

열지 않습니다:

```text
7717/tcp, 3306/tcp, 6379/tcp, 메일 수신 포트
```

## 상세 문서

- 개발 빠른 검증: `bash scripts/quick-gate.sh`
- 전체 품질 검증: `bash scripts/quality-gate.sh`
- [따라하기식 설치 매뉴얼](docs/copy-paste-install.md)
- [초보용 설치 안내](docs/beginner-install.md)
- [Lightsail 상세 안내](docs/lightsail-ubuntu24-setup-guide.md)
- [SPEC](SPEC.md)
- [운영 하네스 감사](docs/ops-harness-audit.md)
- [인프라 거버넌스 통제표](docs/governance-controls.md)
- [기여 안내](CONTRIBUTING.md)
- [보안 정책](SECURITY.md)
- [추천 도구 배너 JSON 정책](docs/promo-manifest.md)
