# Lightsail Ubuntu 24.04 상세 안내

`g7inst` 실서버 테스트용 Ubuntu VPS를 만드는 상세 설명입니다. 그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해본 사용자를 기준으로 합니다.

짧은 순서만 필요하면 [초보용 설치 안내](beginner-install.md)를 먼저 봅니다.

## 빠르게 설정하기

이미 서버, 고정 IP, 도메인 A 레코드가 준비됐으면 바로 진행합니다.

서버 비밀번호는 필요 없습니다. `ubuntu` 계정으로 SSH 키 접속하고, `sudo g7inst setup`으로 설치 컨트롤러를 실행합니다. 웹 UI에는 서버 비밀번호를 입력하지 않습니다.

1. SSH와 sudo 상태를 확인합니다.

```bash
ssh g7installer
sudo -n true && echo "sudo OK"
g7inst --version
exit
```

`sudo OK`가 나오면 비밀번호 없이 설치 준비가 된 상태입니다.

2. Lightsail 스냅샷을 생성합니다.

```text
Lightsail -> 인스턴스 -> 스냅샷 -> 스냅샷 생성
```

3. SSH 터널을 열고 서버에 접속합니다.

```bash
ssh -L 7717:127.0.0.1:7717 g7installer
```

4. 같은 터미널 창에서 설치 UI를 시작합니다.

```bash
sudo g7inst setup --domain g7devops.com
```

5. 브라우저에서 접속 확인 주소를 엽니다.

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
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/lightsail-init.sh -o "$tmp"
bash "$tmp"
```

이 스크립트는 `g7inst` 설치까지만 합니다. OS 업데이트, swap, UFW, fail2ban, 웹서버, PHP, DB, Redis, Certbot, 앱 설치는 `g7inst setup` 웹 UI가 처리합니다.

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

## 5. 방화벽 설정

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

서버에 접속합니다.

```bash
ssh g7installer
```

설치 여부를 확인합니다.

```bash
sudo -n true && echo "sudo OK"
g7inst --version
g7inst doctor
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
```

`sudo OK`가 나오지 않으면 설치 UI를 진행하지 않습니다. 기본 Lightsail Ubuntu 이미지에서는 보통 비밀번호 없이 sudo가 됩니다.

## 9. 설치 전 스냅샷

위험 작업 전 복구 지점을 먼저 만듭니다.

1. Lightsail 콘솔에서 인스턴스를 선택합니다.
2. `스냅샷` 탭을 엽니다.
3. `스냅샷 생성`을 누릅니다.
4. 이름 예시: `before-g7inst-YYYYMMDD-HHMM`

`g7inst rollback`은 installer가 만든 패키지/도메인 연결 설정/웹루트 흔적만 되돌립니다. 운영 파일이 섞이면 차단합니다. 서버 전체 복구는 Lightsail 스냅샷으로 합니다.

## 10. 설치 웹 UI 열기

Mac:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

SSH alias가 있으면:

```bash
ssh -L 7717:127.0.0.1:7717 g7installer
```

서버 안에서 실행합니다.

```bash
sudo g7inst setup --domain example.com
```

브라우저에서 접속 확인 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

웹 화면에서는 root/ubuntu 서버 비밀번호를 입력하지 않습니다. 접속 확인 주소로 접속하면 바로 서버 점검 단계로 진행합니다. 설치 중에는 이 SSH 터널 창을 닫지 않습니다.

`사이트 계정 비밀번호`는 새로 정합니다. 설치기가 만들 `g7` 같은 사이트 계정의 SFTP/파일관리 비밀번호이며, sudo 권한은 주지 않습니다. 기본 웹루트는 `/home/계정/public_html`이고 소유권은 `계정:www-data`로 맞춥니다.

PHP는 Ubuntu 24.04 기본 apt의 8.3을 기본값으로 둡니다. PHP 8.5를 선택하면 설치기가 `ppa:ondrej/php` apt 소스를 자동 추가하고 다시 `apt update`를 실행한 뒤 `php8.5-fpm`을 설치합니다.

기본 서버 구성 후 확인:

```bash
curl -I http://example.com
sudo cat /var/log/g7-installer/report.json
```

리포트 단계가 `vhost-enabled`이면 Nginx 도메인 연결 설정까지 적용된 상태입니다.

## 11. 다른 Ubuntu VPS에서 쓰기

Lightsail이 아니어도 Ubuntu 24.04 서버면 같은 초기 스크립트를 쓸 수 있습니다.

서버 접속 후 실행합니다.

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl
tmp="$(mktemp)"
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/lightsail-init.sh -o "$tmp"
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
| PHP-FPM pool user | PHP를 실행하는 계정. 기본은 사이트 계정 |
| 시작 스크립트 | 서버 최초 생성 때 자동 실행되는 스크립트 |
| UFW | Ubuntu 방화벽 |
| fail2ban | 반복 로그인 공격 차단 도구 |
| PTR | IP에서 도메인을 확인하는 reverse DNS |
| SPF/DKIM/DMARC | 메일 발송 인증용 DNS 레코드 |

## 공식 참고

- AWS 콘솔 언어 변경: <https://docs.aws.amazon.com/awsconsolehelpdocs/latest/gsg/change-language.html>
- AWS Lightsail 인스턴스 생성: <https://docs.aws.amazon.com/lightsail/latest/userguide/getting-started.html>
- AWS Lightsail 시작 스크립트: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-how-to-configure-server-additional-data-shell-script.html>
- AWS Lightsail 방화벽: <https://docs.aws.amazon.com/lightsail/latest/userguide/understanding-firewall-and-port-mappings-in-amazon-lightsail.html>
- AWS Lightsail 고정 IP: <https://docs.aws.amazon.com/lightsail/latest/userguide/lightsail-create-static-ip.html>
