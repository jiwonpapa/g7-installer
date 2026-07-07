# Amazon Lightsail Ubuntu 24.04 인스턴스 준비 매뉴얼

작성일: 2026-07-07

이 문서는 `g7inst`로 새 VPS를 세팅하기 전에 Amazon Lightsail에서 Ubuntu 24.04 인스턴스를 안전하게 만드는 절차입니다.

## 권장 사양

- 서비스: Amazon Lightsail
- 이미지: Linux/Unix, OS 전용, Ubuntu 24.04 LTS
- 네트워크: 공인 IPv4 포함 듀얼 스택
- 번들: 범용, 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송
- 현재 공인 IPv4 포함 번들 가격: 월 12 USD
- 리전: 한국 사용자 대상이면 콘솔에서 선택 가능한 가까운 리전을 우선합니다.

`g7inst` 테스트나 실제 설치에는 WordPress, LAMP 같은 앱 포함 이미지를 고르지 않습니다. 깨끗한 Ubuntu OS 전용 인스턴스여야 설치기가 기존 서비스와 설치기 소유 서비스를 구분할 수 있습니다.

## 무료 크레딧과 비용 기준

AWS 공식 안내 기준으로 신규 AWS 계정은 Free Tier 가입 시 100 USD 크레딧을 즉시 받을 수 있고, 활동 조건에 따라 최대 200 USD까지 6개월 동안 사용할 수 있습니다. Lightsail 가격표에는 Linux/Unix 공인 IPv4 번들 중 월 12 USD 플랜이 3개월 무료 대상에 포함된다고 안내되어 있습니다.

다만 콘솔의 인스턴스 카드에는 무료 표시가 안 보일 수 있습니다. 실제 적용 여부는 계정의 Free Tier/크레딧 상태와 최종 생성 전 결제 안내를 기준으로 확인합니다. 무료/가격 정책은 바뀔 수 있으므로 실제 생성 전 AWS 콘솔과 공식 가격표를 다시 확인합니다. 이 프로젝트는 비용, 트래픽, 운영 난도를 함께 봤을 때 Lightsail 월 12 USD 듀얼 스택 Ubuntu 24.04 구성을 기본 배포 기준으로 둡니다.

> 주의: AWS Free Tier, 크레딧, Lightsail 무료 제공, 번들 가격은 AWS가 언제든 변경할 수 있는 정책입니다. 이 문서의 금액과 무료 조건은 설치 기준을 잡기 위한 참고이며, 실제 과금 여부는 인스턴스 생성 시점의 AWS 콘솔과 결제 안내가 기준입니다.

## IPv6 전용 대신 듀얼 스택을 쓰는 이유

Lightsail에는 같은 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송 조건의 IPv6 전용 Linux 번들도 더 낮은 가격으로 표시됩니다.

하지만 일반 웹서비스는 아직 공인 IPv4가 있는 구성이 운영 난도가 낮습니다. 도메인 A 레코드, SSL 발급, 외부 모니터링, 사용자 접속 환경까지 고려하면 형님이 고른 월 12 USD 듀얼 스택 구성이 맞습니다.

## AWS 계정 준비

1. AWS 계정을 생성합니다.
2. 루트 계정에 MFA를 켭니다.
3. 인스턴스 생성 전에 AWS Budgets 또는 결제 알림을 설정합니다.
4. Lightsail 콘솔로 이동합니다: <https://lightsail.aws.amazon.com/>
5. 루트 계정은 결제와 계정 복구용으로 두고, 가능하면 관리용 IAM 사용자 또는 IAM Identity Center 계정을 따로 씁니다.

## 인스턴스 생성 순서

1. Lightsail 콘솔에서 `Create instance`를 누릅니다.
2. 리전과 가용 영역을 선택합니다.
3. 플랫폼은 `Linux/Unix`를 선택합니다.
4. 블루프린트는 `OS Only`를 선택합니다.
5. OS는 `Ubuntu 24.04 LTS`를 선택합니다.
6. 네트워크는 `Dual-stack` 또는 공인 IPv4가 포함된 옵션을 선택합니다.
7. 플랜은 `2GB RAM / 2 vCPU / 60GB SSD / 3TB transfer`, 월 12 USD 공인 IPv4 번들을 선택합니다.
8. SSH 키는 새로 만들거나 Mac의 공개키를 업로드합니다.
9. 이 문서의 시작 스크립트를 추가합니다.
10. 인스턴스 이름은 `g7-prod-01`, `g7-test-01`처럼 용도를 알 수 있게 정합니다.
11. 인스턴스를 생성합니다.

## SSH 키 권장 방식

서버별 또는 프로젝트별 전용 키를 권장합니다. 모든 VPS에 개인 공용 키 하나를 계속 재사용하지 않는 편이 좋습니다.

Mac에서 새 키를 만들 때:

```bash
ssh-keygen -t rsa -b 4096 -f ~/.ssh/lightsail_g7inst_202607 -C "lightsail-g7inst"
```

Lightsail 콘솔에서 `.pub` 파일을 업로드하거나, 인스턴스 생성 화면에서 SSH 키 변경 기능이 있으면 해당 키를 선택합니다.

생성 후 Mac에서 접속:

```bash
chmod 600 ~/.ssh/lightsail_g7inst_202607
ssh -i ~/.ssh/lightsail_g7inst_202607 ubuntu@SERVER_IP
```

root 권한이 필요할 때만 전환합니다.

```bash
sudo -i
```

루트 비밀번호, DB 비밀번호, 앱 secret, SMTP 비밀번호는 Lightsail 시작 스크립트에 넣지 않습니다.

추가 SSH 키를 시작 스크립트로 넣을 수도 있지만, 넣는 값은 반드시 `.pub` 공개키여야 합니다. 개인키는 Mac에만 두고 서버, Git, 문서, 시작 스크립트에 넣지 않습니다.

```bash
cat ~/.ssh/lightsail_g7inst_202607.pub
```

출력값을 아래 시작 스크립트의 `EXTRA_SSH_PUBLIC_KEY`에 넣으면 `ubuntu` 계정에 추가됩니다. Lightsail 콘솔에서 SSH 키를 선택했다면 이 값은 비워둡니다.

## 시작 스크립트

Lightsail 생성 화면의 `Add launch script`에 아래 내용을 넣습니다.

이 스크립트는 `g7inst` 바이너리를 받을 최소 발판만 만듭니다. `curl`, `ca-certificates` 같은 부트스트랩 의존성 외에는 서버 설정을 건드리지 않습니다.

OS 업데이트, 보안 업데이트, swap, UFW, fail2ban, SSH 보안 점검, Nginx, Apache, PHP, MySQL, MariaDB, Redis, Certbot은 `g7inst setup`이 처리하고 리포트해야 합니다. 시작 스크립트가 먼저 처리하면 웹 UI, 진행률, 실패 리포트, 되돌리기 기준이 약해집니다.

`g7inst setup` 실행은 시작 스크립트에 넣지 않습니다. 웹 컨트롤러 token URL이 부팅 로그에 남고, DNS/고정 IP/도메인 준비 전에 설치 흐름이 시작될 수 있기 때문입니다.

```bash
#!/usr/bin/env bash
set -euxo pipefail

export DEBIAN_FRONTEND=noninteractive
LOG_FILE="/var/log/g7-lightsail-bootstrap.log"
exec > >(tee -a "${LOG_FILE}") 2>&1

EXTRA_SSH_PUBLIC_KEY=""

echo "g7 Lightsail bootstrap started at $(date -Is)"

timedatectl set-timezone Asia/Seoul || true

apt-get update
apt-get install -y \
  ca-certificates \
  curl

if [ -n "${EXTRA_SSH_PUBLIC_KEY}" ]; then
  install -d -m 700 -o ubuntu -g ubuntu /home/ubuntu/.ssh
  touch /home/ubuntu/.ssh/authorized_keys
  chown ubuntu:ubuntu /home/ubuntu/.ssh/authorized_keys
  chmod 600 /home/ubuntu/.ssh/authorized_keys
  if ! grep -qxF "${EXTRA_SSH_PUBLIC_KEY}" /home/ubuntu/.ssh/authorized_keys; then
    echo "${EXTRA_SSH_PUBLIC_KEY}" >>/home/ubuntu/.ssh/authorized_keys
  fi
fi

mkdir -p /opt/g7-bootstrap
cat >/opt/g7-bootstrap/README.txt <<'README'
This server was prepared for g7inst.
The launch script only installed minimal bootstrap dependencies and g7inst.
OS updates, security baseline, swap, firewall, fail2ban, web server, PHP,
database, Redis, Certbot, and app files should be installed by g7inst.
README

apt-get clean

curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/bootstrap.sh | bash
g7inst --version
g7inst doctor || true

echo "g7 Lightsail bootstrap completed at $(date -Is)"
```

## 생성 직후 확인

브라우저 SSH 또는 Mac 터미널로 접속한 뒤 확인합니다.

```bash
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
lsb_release -a
g7inst --version
g7inst doctor
```

시작 스크립트가 아직 실행 중이면 몇 분 기다린 뒤 다시 확인합니다.

## Lightsail 네트워크 체크리스트

인스턴스 생성 후 바로 확인합니다.

1. Lightsail 고정 IP를 생성하고 인스턴스에 연결합니다.
2. 고정 IP는 서버를 쓰는 동안 계속 연결해둡니다. 연결되지 않은 고정 IP는 비용이 발생할 수 있습니다.
3. Lightsail 방화벽에서 포트를 엽니다.
   - SSH 22/tcp: 가능하면 형님 IP만 허용
   - HTTP 80/tcp: 전체 허용
   - HTTPS 443/tcp: 전체 허용
4. `g7inst` 웹 컨트롤러 포트 `7717`은 외부 공개하지 않습니다.
5. 설치기 웹 UI는 SSH 터널로 엽니다.

```bash
ssh -i ~/.ssh/lightsail_g7inst_202607 -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

## g7inst 실행

시작 스크립트가 정상 완료되면 `g7inst` 바이너리는 이미 설치되어 있습니다. 서버 접속 후 상태를 확인합니다.

```bash
g7inst --version
g7inst doctor
```

이후 작업은 `g7inst setup` 웹 마법사에서 처리합니다.

`g7inst setup`이 맡아야 할 항목:

- apt update/upgrade
- 필수 운영 패키지 설치
- unattended-upgrades 설정
- swap 설정
- UFW 방화벽 설정
- fail2ban 설치와 상태 리포트
- SSH 보안 점검
- 웹서버, PHP, DB, Redis, Certbot 설치
- 앱 설치와 최종 리포트

서버에서는 설치기를 실행합니다.

```bash
sudo g7inst setup --domain example.com
```

터미널에 출력된 token URL을 Mac 브라우저에서 엽니다.

## DNS 체크리스트

실도메인을 쓸 때:

1. 도메인 A 레코드를 Lightsail 고정 IPv4로 지정합니다.
2. IPv6도 쓸 경우 AAAA 레코드를 인스턴스 IPv6로 지정합니다.
3. DNS 전파를 기다립니다.
4. Mac에서 확인합니다.

```bash
dig +short example.com A
dig +short example.com AAAA
```

## 백업 체크리스트

실제 데이터를 넣기 전 확인합니다.

1. 운영 서버라면 자동 스냅샷을 켭니다.
2. 큰 설치나 변경 전에는 수동 스냅샷을 남깁니다.
3. 스냅샷은 별도 비용이 발생할 수 있습니다.

## 시작 스크립트에 넣지 말 것

- 루트 비밀번호 설정
- DB 비밀번호
- SMTP 비밀번호
- 앱 secret key
- Nginx, Apache, PHP, MySQL, MariaDB, Redis, Certbot 설치
- `7717` 포트 외부 공개
- DNS가 준비되기 전 SSL 인증서 발급

이 항목들은 `g7inst` 또는 후속 배포 단계에서 통제하는 편이 맞습니다.

## 참고한 공식 문서

- AWS Lightsail 계정 준비: <https://docs.aws.amazon.com/lightsail/latest/userguide/setting-up.html>
- AWS Lightsail 인스턴스 생성: <https://docs.aws.amazon.com/lightsail/latest/userguide/getting-started.html>
- AWS Lightsail 시작 스크립트: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-how-to-configure-server-additional-data-shell-script.html>
- AWS Free Tier: <https://aws.amazon.com/free/>
- AWS Lightsail Free Tier: <https://aws.amazon.com/free/compute/lightsail/>
- AWS Lightsail 가격: <https://aws.amazon.com/lightsail/pricing/>
- AWS Lightsail SSH 키: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-how-to-set-up-ssh.html>
- AWS Lightsail IPv6/듀얼 스택: <https://docs.aws.amazon.com/lightsail/latest/userguide/amazon-lightsail-ipv6-only-plans.html>
- AWS Lightsail 고정 IP: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-create-static-ip.html>
- AWS Lightsail 방화벽: <https://docs.aws.amazon.com/lightsail/latest/userguide/understanding-firewall-and-port-mappings-in-amazon-lightsail.html>
- AWS Lightsail 스냅샷: <https://docs.aws.amazon.com/lightsail/latest/userguide/understanding-snapshots-in-amazon-lightsail.html>
