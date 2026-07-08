# G7 Installer

Ubuntu VPS에 `g7inst`를 설치하고 웹 마법사로 그누보드7/워드프레스/라라벨용 서버 구성과 사이트 프로비저닝을 진행하는 도구입니다.

> 현재 공개 릴리스는 `g7inst` 설치, 서버 점검, 웹 마법사, apt 패키지 설치, Nginx/Apache 도메인 연결 설정(vhost), PHP-FPM/DB 사양 튜닝, DB 앱 계정 생성, 웹서버별 Let's Encrypt 인증서 발급/갱신 검증, 그누보드7/Laravel Composer·NPM·Artisan·queue·scheduler·Reverb 구성, WordPress 설치 링크, 상세 설정 안내서 저장까지 검증합니다.

## 무엇부터 보면 되나요?

| 상황 | 문서 |
| --- | --- |
| 처음 설치하는 초보자 | [초보용 설치 안내](docs/beginner-install.md) |
| Lightsail 화면을 보며 상세 설정 | [Lightsail 상세 안내](docs/lightsail-ubuntu24-setup-guide.md) |
| 이미 서버와 도메인이 준비됨 | [바로 설치 UI 열기](#바로-설치-ui-열기) |
| 개발/검증 기준 확인 | [운영 하네스 감사](docs/ops-harness-audit.md) |

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

설치 계획에는 1GB, 2GB, 4GB, 8GB, 16GB, 32GB 메모리 프리셋과 32GB 초과 공식 기반 프리셋이 포함됩니다. PHP-FPM `max_children`, opcache, DB buffer pool, DB connection, Redis maxmemory, swap, Nginx worker process/connection/buffer, Apache `mpm_event` worker 값을 메모리와 vCPU 등급별로 계산합니다. 현재 실행 단계는 감지된 RAM/vCPU에 맞춰 PHP-FPM pool, PHP ini, Nginx/Apache vhost, MySQL/MariaDB 튜닝 파일을 적용하고 리포트에 기록합니다.

## PHP 버전과 apt 소스

- PHP 8.3은 Ubuntu 24.04 기본 apt 소스를 사용합니다.
- PHP 8.5를 선택하면 설치기가 `software-properties-common`, `ca-certificates`, `lsb-release`를 먼저 설치하고 `ppa:ondrej/php`를 자동 추가한 뒤 다시 `apt update`를 실행합니다.
- 리포트와 웹 UI에는 `php_source`가 `ubuntu` 또는 `ondrej`로 표시됩니다.

## 바로 설치 UI 열기

서버, 도메인, 방화벽 설정이 끝났다면 아래 순서대로 실행합니다.

서버 비밀번호는 필요 없습니다. Ubuntu 서버는 `ubuntu` 계정으로 SSH 키 접속하고, 설치기는 `sudo g7inst setup`으로 설치 컨트롤러를 실행합니다. 웹 UI에는 root/ubuntu 비밀번호를 입력하지 않습니다.

웹 UI의 `사이트 계정 비밀번호`는 별도입니다. 설치기가 만들 `g7` 같은 Linux 사이트 계정의 SFTP/파일관리 비밀번호이며, sudo 권한은 주지 않습니다.

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

`sudo OK`가 나오면 비밀번호 없이 설치 준비가 된 상태입니다.

2. Lightsail 스냅샷을 먼저 찍습니다.

```text
Lightsail -> 인스턴스 -> 스냅샷 -> 스냅샷 생성
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

웹 UI에서 `사이트 계정`과 `사이트 계정 비밀번호`를 입력하면 설치기가 `/home/계정/public_html` 웹루트를 만들고 `계정:www-data` 소유권으로 맞춥니다. PHP-FPM pool은 이 사이트 계정으로 실행됩니다.

설치가 끝나면 웹 UI 결과 리포트와 서버의 `/var/log/g7-installer/setup-guide.md`를 확인합니다. 이 Markdown 안내서에는 웹루트, PHP-FPM pool, DB 설정, 인증서, 앱 systemd unit, 주요 `systemctl` 명령, 비밀 파일 위치가 정리됩니다. PDF가 필요하면 브라우저 인쇄/PDF 저장으로 내보내는 방식을 권장합니다.

## 시작 스크립트

Lightsail `시작 스크립트 추가`에는 아래만 넣습니다.

```sh
#!/bin/sh
set -eu
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/lightsail-init.sh -o "$tmp"
bash "$tmp"
```

이 스크립트는 `g7inst` 설치까지만 합니다. OS 업데이트, swap, UFW, fail2ban, 웹서버, PHP, DB, Redis, Certbot, 앱 설치는 `g7inst setup` 웹 UI가 처리합니다.

## 기본 명령

```bash
g7inst --version
g7inst doctor
sudo g7inst setup --domain example.com
sudo g7inst rollback --yes
sudo g7inst reset --yes
```

`rollback`은 앱/DB/인증서 생성 전의 초기 실패를 되돌리는 용도입니다. 운영 중인 사이트 백업 복구 기능이 아닙니다.

`reset --yes`는 이 설치기가 만든 사이트 계정, 웹루트/설정 파일, 앱 systemd unit, DB/DB 계정, Let's Encrypt 인증서, 새로 설치한 apt 패키지, installer 메타데이터를 제거해 같은 신규 VPS에서 다시 설치를 시도할 수 있게 합니다. 기존 운영 서버 보존 기능이 아니라 신규 VPS 전용 재설치 초기화입니다. 실행 전 Lightsail/VPS 스냅샷을 먼저 찍으세요.

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

- [초보용 설치 안내](docs/beginner-install.md)
- [Lightsail 상세 안내](docs/lightsail-ubuntu24-setup-guide.md)
- [SPEC](SPEC.md)
- [운영 하네스 감사](docs/ops-harness-audit.md)
