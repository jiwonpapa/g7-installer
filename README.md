# G7 Installer

Ubuntu VPS에 `g7inst`를 설치하고 웹 마법사로 서버 설치를 진행하는 도구입니다.

> 현재 공개 릴리스는 `g7inst` 설치, 서버 점검, 웹 마법사, apt 패키지 설치, Nginx HTTP vhost 적용, 진행률, 리포트, rollback/reset까지 검증합니다. DB 앱 계정 생성, 앱 소스 배치, Certbot 발급은 다음 단계입니다.

## 무엇부터 보면 되나요?

| 상황 | 문서 |
| --- | --- |
| 처음 설치하는 초보자 | [초보용 설치 안내](docs/beginner-install.md) |
| Lightsail 화면을 보며 상세 설정 | [Lightsail 상세 안내](docs/lightsail-ubuntu24-setup-guide.md) |
| 이미 서버와 도메인이 준비됨 | [바로 설치 UI 열기](#바로-설치-ui-열기) |
| 개발/검증 기준 확인 | [운영 하네스 감사](docs/ops-harness-audit.md) |

## 권장 배포 기준

> **권장 인스턴스**
>
> - AWS Lightsail
> - Ubuntu 24.04 LTS
> - 듀얼 스택, 공인 IPv4 포함
> - 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송
> - 방화벽 22, 80, 443만 오픈

무료 크레딧, 무료 기간, 번들 가격은 AWS가 언제든 바꿀 수 있습니다. 실제 과금 기준은 인스턴스 생성 화면과 결제 안내를 확인하세요.

## 바로 설치 UI 열기

서버, 도메인, 방화벽 설정이 끝났다면 아래만 실행합니다.

1. Lightsail 스냅샷을 먼저 찍습니다.

```text
Lightsail -> 인스턴스 -> 스냅샷 -> 스냅샷 생성
```

2. SSH 터널을 엽니다.

```bash
ssh -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

SSH alias가 있으면:

```bash
ssh -L 7717:127.0.0.1:7717 g7installer
```

3. 서버 안에서 설치 UI를 시작합니다.

```bash
sudo g7inst setup --domain example.com
```

4. 브라우저에서 token URL을 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

`7717/tcp`는 외부에 열지 않습니다. SSH 터널로만 접속합니다.

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

`rollback`은 설치 직후 installer가 만든 패키지/vhost/webroot 흔적을 되돌리는 용도입니다. 운영 중인 사이트 백업 복구 기능이 아닙니다.

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
