# Lightsail Ubuntu 24.04 상세 안내

`g7inst` 실서버 테스트용 Ubuntu VPS를 만드는 상세 설명입니다. 그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해본 사용자를 기준으로 합니다.

현재 문서는 Public Beta 기준입니다. 새 Ubuntu 24.04 VPS에서 그누보드7 설치 기반을 만드는 흐름만 다룹니다.

짧은 순서만 필요하면 [초보용 설치 안내](beginner-install.md)를 먼저 봅니다.

## 빠르게 설정하기

이미 서버, 고정 IP, 도메인 A 레코드가 준비됐으면 바로 진행합니다.

Lightsail 기본 Ubuntu는 `.pem` 개인키와 `ubuntu` 계정을 사용합니다. Mac 터미널에서는 아래 한 줄을 실행합니다.

```bash
ssh -i "$HOME/.ssh/lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

다른 VPS에서 SSH 비밀번호 로그인을 허용하면 Mac과 Windows에서 아래 명령을 사용합니다.

```bash
ssh -t -L 7717:127.0.0.1:7717 SSH_USER@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

SSH 비밀번호와 sudo 비밀번호는 요청될 때 터미널에만 입력합니다. 도메인은 웹 마법사에서 한 번만 입력합니다. 브라우저에서는 터미널에 출력된 접속 확인 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

## 1. AWS 콘솔을 한글로 바꾸기

1. AWS 콘솔에 로그인합니다.
2. 우측 상단 톱니바퀴를 누릅니다.
3. `언어`에서 `한국어`를 선택합니다.
4. 필요하면 `모든 사용자 설정 보기`에서 기본 리전도 바꿉니다.

## 2. 권장 인스턴스

> **권장값**
>
> - 서비스: Amazon Lightsail
> - 이미지: Linux/Unix, OS 전용
> - OS: Ubuntu 24.04 LTS
> - 네트워크: 듀얼 스택, 공인 IPv4 포함
> - 크기: 월 12 USD, 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송
> - 이름 예시: `g7installer`, `g7-prod-01`

2GB는 기본 권장값입니다. `g7inst` 계획 화면은 1GB, 2GB, 4GB, 8GB, 16GB, 32GB와 32GB 초과 메모리 기준으로 PHP-FPM, DB, Redis, swap, Nginx worker, Apache `mpm_event` worker 값을 나눠 보여줍니다.

무료 크레딧, 무료 기간, 번들 가격은 AWS가 언제든 바꿀 수 있습니다. 실제 과금은 인스턴스 생성 화면과 결제 안내를 기준으로 확인합니다.

## 3. 인스턴스 만들기

1. Lightsail 콘솔로 갑니다.
2. `인스턴스 생성`을 누릅니다.
3. 플랫폼은 `Linux/Unix`를 선택합니다.
4. 블루프린트는 `OS 전용`을 선택합니다.
5. OS는 `Ubuntu 24.04 LTS`를 선택합니다.
6. 네트워크는 `듀얼 스택`을 선택합니다.
7. 크기는 권장 인스턴스를 선택합니다.
8. SSH 키를 생성하고 `.pem` 파일을 다운로드합니다.
9. `시작 스크립트 추가`에 아래 스크립트를 넣습니다.
10. 인스턴스 이름을 입력합니다.
11. `인스턴스 생성`을 누릅니다.

```sh
#!/bin/sh
set -eu
LOG=/var/log/g7-lightsail-bootstrap.log
exec >"$LOG" 2>&1
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh -o "$tmp"
bash "$tmp"
g7inst --version
```

이 스크립트는 `g7inst` 설치까지만 합니다. 실행 로그는 `/var/log/g7-lightsail-bootstrap.log`에 남습니다. OS 업데이트, swap, 웹서버, PHP, DB, Redis, Certbot, 앱 설치는 `g7inst setup` 웹 UI가 처리합니다. UFW·fail2ban은 별도 유지보수 영역이므로 설치하거나 변경하지 않습니다.

Lightsail은 시작 스크립트를 자체 `/bin/sh` 스크립트 뒤에 붙여 실행할 수 있습니다. 그래서 `pipefail`과 `curl ... | bash`를 쓰지 않습니다.

## 4. SSH 키 저장하기

개인키는 Git, 문서, 시작 스크립트에 넣지 않습니다.

### Mac

```bash
mkdir -p ~/.ssh
mv ~/Downloads/YOUR_LIGHTSAIL_KEY.pem ~/.ssh/lightsail_g7inst.pem
chmod 600 ~/.ssh/lightsail_g7inst.pem
ssh -i ~/.ssh/lightsail_g7inst.pem ubuntu@SERVER_IP
```

### Windows PowerShell

```powershell
mkdir $env:USERPROFILE\.ssh
Move-Item "$env:USERPROFILE\Downloads\YOUR_LIGHTSAIL_KEY.pem" "$env:USERPROFILE\.ssh\lightsail_g7inst.pem"
icacls "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" /inheritance:r /grant:r "$($env:USERNAME):(R)"
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" ubuntu@SERVER_IP
```

### SSH alias 선택 설정

Mac은 `~/.ssh/config`, Windows는 `%USERPROFILE%\.ssh\config`에 저장합니다.

```sshconfig
Host g7installer
  HostName SERVER_IP
  User ubuntu
  IdentityFile ~/.ssh/lightsail_g7inst.pem
  IdentitiesOnly yes
```

접속:

```bash
ssh g7installer
```

## 5. VPS 제공자 방화벽 설정 (설치기 범위 밖)

메뉴 위치:

1. Lightsail 콘솔
2. 인스턴스 선택
3. `네트워킹`
4. `IPv4 방화벽`

열 포트:

| 포트 | 용도 | 대상 |
| --- | --- | --- |
| 22/tcp | SSH | 가능하면 내 IP |
| 80/tcp | HTTP, 인증서 발급 | 전체 |
| 443/tcp | HTTPS | 전체 |

열지 않을 포트:

| 포트 | 이유 |
| --- | --- |
| 7717/tcp | 설치 관리자 UI |
| 3306/tcp | MySQL 외부 공개 금지 |
| 6379/tcp | Redis 외부 공개 금지 |
| 25/465/587/tcp inbound | 메일 수신 안 하면 불필요 |

## 6. 고정 IP 연결

1. Lightsail 콘솔에서 `네트워킹`으로 이동합니다.
2. `고정 IP 생성`을 누릅니다.
3. 인스턴스에 연결합니다.
4. 도메인 A 레코드에 이 IP를 넣습니다.

고정 IP는 인스턴스에 연결해 둡니다. 연결하지 않고 방치하면 비용이 발생할 수 있습니다.

## 7. DNS 설정

Cloudflare를 쓰면 설치 중에는 프록시를 끕니다.

| 타입 | 이름 | 값 | 상태 |
| --- | --- | --- | --- |
| A | `@` | 서버 고정 IP | DNS only |
| CNAME | `www` | 루트 도메인 | DNS only |
| A | `mail` | 서버 고정 IP | DNS only |

확인:

```bash
dig +short example.com A
dig +short www.example.com A
```

## 8. g7inst 확인

서버에 접속합니다. SSH alias가 없어도 됩니다.

```bash
ssh -i "$HOME/.ssh/lightsail_g7inst.pem" ubuntu@SERVER_IP
```

설치 여부를 확인합니다.

```bash
sudo -n true && echo "sudo OK"
g7inst --version
g7inst doctor
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
```

`sudo OK`가 나오지 않으면 일반 계정에서 `g7inst setup`을 실행해 sudo 재실행을 시도합니다. 그래도 실패하면 설치 UI를 진행하지 말고 root SSH, `su -`, 또는 VPS 콘솔에서 관리자 권한을 확보합니다. 기본 Lightsail Ubuntu 이미지는 보통 비밀번호 없이 sudo가 됩니다.

매번 sudo 비밀번호를 묻는 서버는 아래 선택 설정을 적용하면 이후 `g7inst setup` 재실행이 비밀번호 없이 진행됩니다.

### 선택: `g7inst`만 비밀번호 없이 sudo 허용

```bash
sudo visudo -f /etc/sudoers.d/g7inst
```

접속 계정이 `ubuntu`가 아니면 실제 계정명으로 바꿉니다.

```text
ubuntu ALL=(root) NOPASSWD: SETENV: /usr/local/bin/g7inst
```

저장 후 검사합니다.

```bash
sudo chmod 0440 /etc/sudoers.d/g7inst
sudo visudo -cf /etc/sudoers.d/g7inst
sudo -n /usr/local/bin/g7inst --version
```

모든 명령을 허용하는 `ALL=(ALL) NOPASSWD: ALL`은 사용하지 않습니다. 설치 후 예외가 필요 없으면 `sudo rm /etc/sudoers.d/g7inst`로 제거합니다.

`g7inst: command not found`가 나오면 먼저 `sudo tail -120 /var/log/g7-lightsail-bootstrap.log`로 시작 스크립트가 끝났는지 확인합니다. 로그가 끝났는데도 없으면 아래를 서버 안에서 한 번 실행합니다.

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl
tmp="$(mktemp)"
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh -o "$tmp"
sudo bash "$tmp"
rm -f "$tmp"
g7inst --version
```

## 9. 선택: VPS 백업/스냅샷 확인

설치기가 되돌릴 수 있는 범위는 설치기가 만든 파일과 패키지입니다. 서버 전체 복구가 필요하면 VPS 백업/스냅샷이 기준이지만, 비용과 생성 시간이 들 수 있습니다.

1. Lightsail 콘솔에서 인스턴스를 선택합니다.
2. `스냅샷` 탭을 엽니다.
3. 비용과 생성 시간을 확인합니다.
4. 필요할 때만 `스냅샷 생성`을 누릅니다.
5. 이름 예시: `before-g7inst-YYYYMMDD-HHMM`

설치가 중단되면 먼저 stderr와 실패 항목을 확인합니다. 설치기는 원인을 자동 수정하지 않고 실패 단계의 파일 변경만 복원합니다. 설치기 업데이트나 입력·환경 수정 후 웹 결과 화면의 `수정 후 현재 단계 재실행` 또는 `sudo g7inst resume`을 사용합니다. 완료 단계는 건너뜁니다. `g7inst rollback`은 앱/DB/인증서 생성 전의 초기 실패만 되돌립니다. `g7inst reset --yes`는 설치를 완전히 포기할 때 설치기가 만든 계정, DB/DB 계정, 서비스, 웹루트/설정 파일, 새로 설치한 패키지, 메타데이터를 제거합니다. `/etc/letsencrypt/live/*`의 인증서와 certbot 자동 갱신은 보존합니다. 서버 전체 복구는 VPS 백업/스냅샷으로 처리합니다.

## 10. 설치 웹 UI 열기

`.pem` 개인키 방식은 SSH 터널과 설치기 설치·실행을 한 줄로 처리합니다.

Mac 터미널:

```bash
ssh -i "$HOME/.ssh/lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

SSH 비밀번호 방식은 Mac과 Windows에서 같은 명령을 사용합니다.

```bash
ssh -t -L 7717:127.0.0.1:7717 SSH_USER@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh | sudo bash && sudo g7inst setup'
```

브라우저에서 접속 확인 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

비밀번호는 요청될 때 터미널에만 입력합니다. 웹 화면에서는 서버 비밀번호를 입력하지 않고 도메인을 한 번 입력합니다. 설치 중에는 이 SSH 터널 창을 닫지 않습니다.

`사이트 계정 비밀번호`는 새로 정합니다. 설치기가 만들 `g7` 같은 사이트 계정의 SFTP/파일관리 비밀번호이며, sudo 권한은 주지 않습니다. 기본 웹루트는 `/home/계정/public_html`이고 소유권은 `계정:www-data`로 맞춥니다.

`운영 권장` 프로필은 Ubuntu 24.04 기본 apt의 PHP 8.3과 MySQL 8.0을 사용합니다. `최신 지원` 프로필은 검증된 Ondrej PPA의 PHP 8.5와 MySQL 공식 APT의 8.4 LTS를 사용합니다.

웹 UI 기본 조합은 `운영 권장 / Nginx / PHP 8.3 / MySQL 8.0 Ubuntu 기본 APT / www로 통일 / Redis 사용 / 메일 발송 안 함 / 그누보드7`입니다. 최신 지원은 PHP 8.5와 MySQL 8.4 LTS를 함께 선택합니다. 외부 SMTP를 선택하면 계정과 비밀번호를 필수로 받고 비밀번호는 루트 전용 비밀 파일에만 저장합니다. 로컬 Postfix는 발신 IP 평판·PTR·25번 포트 정책을 직접 관리할 사용자만 선택합니다.

기본 서버 구성 후 확인:

```bash
curl -I http://example.com
sudo cat /var/log/g7-installer/report.json
sudo less /var/log/g7-installer/setup-guide.md
```

리포트 단계가 `completed`이면 서버 프로비저닝이 끝난 상태입니다. 패키지, Nginx/Apache vhost, PHP/DB 튜닝, DB 계정, 앱 파일 배치, 설정 안내서 저장까지 끝났다는 뜻이며 CMS 관리자 설치 완료를 뜻하지는 않습니다. SSL은 DNS/IP 검증과 인증서 발급 조건이 맞을 때 적용되며, 보류나 실패가 있으면 리포트에 표시됩니다. 그누보드7은 GitHub 공식 최신 안정 Release와 필수 빌드 파일을 검증하고 `.env.example`에서 사이트 계정 전용 `0600` 권한의 `.env`를 준비한 뒤 브라우저 `/install`로 인계합니다. 결과 리포트의 `앱 링크`에서 Composer/Vendor, 관리자 계정, 확장과 마이그레이션을 진행합니다.

완료 리포트의 `PHP 환경 요약`은 PHP 버전, ini 경로, 시간대, 주요 한도, OPcache, PHP-FPM pool과 필수 확장을 보여줍니다. 전체 `phpinfo()` 페이지는 외부에 공개하지 않습니다. 재설치 초기화는 확인창에 `초기화`를 입력해야 실행되며 사이트 계정, 웹파일, DB/DB 계정, 서비스, 설정과 설치 패키지를 삭제합니다. G7 DB와 설치 잠금 파일이 확인되면 이미 설치 완료된 사이트라고 경고하고, Let's Encrypt 인증서는 보존합니다.

## 11. 다른 Ubuntu VPS에서 쓰기

Lightsail이 아니어도 Ubuntu 24.04 서버면 같은 초기 스크립트를 쓸 수 있습니다.

서버 접속 후 실행합니다.

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl
tmp="$(mktemp)"
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.7/bootstrap.sh -o "$tmp"
sudo bash "$tmp"
rm -f "$tmp"
```

## 12. 메일 보내기만 쓸 때

메일 수신은 하지 않습니다. 인바운드 25번은 열지 않습니다.

| 설정 | 값 |
| --- | --- |
| SPF | `v=spf1 ip4:SERVER_IP -all` |
| DMARC | `v=DMARC1; p=none; adkim=r; aspf=r` |
| DKIM | Postfix/OpenDKIM 설정 후 생성 |
| PTR | AWS에 reverse DNS 요청 |

AWS에서 outbound 25 제한이 있으면 해제 요청이 필요합니다.

## 13. 용어 설명

| 용어 | 뜻 |
| --- | --- |
| VPS | 클라우드 가상 서버 |
| 고정 IP | 서버 재시작 후에도 유지되는 공인 IP |
| A 레코드 | 도메인을 IP에 연결하는 DNS 기록 |
| DNS only | Cloudflare 프록시 없이 실제 서버 IP를 보여주는 상태 |
| SSH 키 | 비밀번호 대신 쓰는 접속 키 |
| SSH 터널 | 서버 내부 포트를 내 PC 브라우저로 안전하게 연결하는 방법 |
| 접속 확인 주소 | 터미널에 출력되는 `http://127.0.0.1:7717/?token=...` 주소 |
| 사이트 계정 | 웹파일 소유자이자 SFTP로 파일을 올릴 Linux 계정 |
| PHP 런타임 사용자 | PHP-FPM pool을 실행하는 계정. 기본은 사이트 계정 |
| 시작 스크립트 | 서버 최초 생성 때 자동 실행되는 스크립트 |
| UFW | Ubuntu 방화벽. G7 Installer는 설치·변경하지 않음 |
| fail2ban | 반복 로그인 공격 차단 도구. 별도 유지보수 앱 영역 |
| PTR | IP에서 도메인을 확인하는 reverse DNS |
| SPF/DKIM/DMARC | 메일 발송 인증용 DNS 레코드 |

## 공식 참고

- AWS 콘솔 언어 변경: <https://docs.aws.amazon.com/awsconsolehelpdocs/latest/gsg/change-language.html>
- AWS Lightsail 인스턴스 생성: <https://docs.aws.amazon.com/lightsail/latest/userguide/getting-started.html>
- AWS Lightsail 시작 스크립트: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-how-to-configure-server-additional-data-shell-script.html>
- AWS Lightsail 방화벽: <https://docs.aws.amazon.com/lightsail/latest/userguide/understanding-firewall-and-port-mappings-in-amazon-lightsail.html>
- AWS Lightsail 고정 IP: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-create-static-ip.html>
