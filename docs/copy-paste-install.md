# 따라하기식 설치 매뉴얼

새 Ubuntu 24.04 VPS에 `g7inst`를 설치하고 웹 마법사를 여는 복붙용 절차입니다. Mac 터미널과 Windows PowerShell 둘 다 따라 할 수 있습니다.

대상은 새 서버입니다. 기존 운영 서버에는 실행하지 마세요.

실행 위치를 구분합니다.

| 표시 | 의미 |
| --- | --- |
| Mac | 내 Mac 터미널에서 실행 |
| Windows PowerShell | 내 Windows PowerShell에서 실행 |
| 서버 안 | SSH 접속 후 Ubuntu 서버 터미널에서 실행. Mac/Windows 공통 |

## 0. 내 값 정하기

Mac:

```bash
SERVER_IP="서버_공인_IP"
DOMAIN="example.com"
KEY="$HOME/.ssh/lightsail_g7inst.pem"
```

예:

```bash
SERVER_IP="52.79.62.209"
DOMAIN="g7devops.com"
KEY="$HOME/.ssh/lightsail_g7inst.pem"
```

Windows PowerShell:

```powershell
$SERVER_IP = "서버_공인_IP"
$DOMAIN = "example.com"
$KEY = "$env:USERPROFILE\.ssh\lightsail_g7inst.pem"
```

## 1. DNS 설정

Cloudflare를 쓰면 프록시는 끄고 `DNS only`로 둡니다.

| 타입 | 이름 | 값 |
| --- | --- | --- |
| A | `@` | `SERVER_IP` |
| CNAME | `www` | `DOMAIN` |

DNS 확인입니다. Mac:

```bash
dig +short "$DOMAIN" A
dig +short "www.$DOMAIN" A
```

Windows PowerShell:

```powershell
nslookup $DOMAIN
nslookup "www.$DOMAIN"
```

## 2. SSH 키 준비

Mac:

```bash
mkdir -p ~/.ssh
mv ~/Downloads/YOUR_LIGHTSAIL_KEY.pem "$KEY"
chmod 600 "$KEY"
```

Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.ssh" | Out-Null
Move-Item "$env:USERPROFILE\Downloads\YOUR_LIGHTSAIL_KEY.pem" "$KEY"
icacls "$KEY" /inheritance:r /grant:r "$($env:USERNAME):(R)"
```

## 3. 서버 접속

Mac:

```bash
ssh -i "$KEY" ubuntu@"$SERVER_IP"
```

Windows PowerShell:

```powershell
ssh -i $KEY ubuntu@$SERVER_IP
```

접속되면 서버 안에서 확인합니다. 아래 명령은 Mac/Windows 공통입니다.

```bash
sudo -n true && echo "sudo OK"
```

`sudo OK`가 나오지 않아도 sudo 비밀번호를 아는 계정이면 계속 진행할 수 있습니다.

## 4. g7inst 설치

서버 안에서 실행합니다. 아래 명령은 Mac/Windows 공통입니다.

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl
tmp="$(mktemp)"
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh -o "$tmp"
sudo bash "$tmp"
rm -f "$tmp"
g7inst --version
sudo g7inst doctor
```

sudo 비밀번호를 물어보면 SSH 터미널에 입력합니다.

## 5. 선택: g7inst만 비밀번호 없이 sudo 허용

매번 sudo 비밀번호를 묻는 서버라면 `g7inst` 명령만 비밀번호 없이 실행되도록 제한할 수 있습니다.

```bash
sudo visudo -f /etc/sudoers.d/g7inst
```

파일에 아래 줄을 넣습니다. 접속 계정이 `ubuntu`가 아니면 `ubuntu`를 실제 계정명으로 바꿉니다.

```text
ubuntu ALL=(root) NOPASSWD: SETENV: /usr/local/bin/g7inst
```

편집기가 `nano`로 열렸다면 `Ctrl+O`, `Enter`, `Ctrl+X` 순서로 저장하고 나옵니다.

저장 후 확인합니다.

```bash
sudo chmod 0440 /etc/sudoers.d/g7inst
sudo visudo -cf /etc/sudoers.d/g7inst
sudo -n /usr/local/bin/g7inst --version
```

모든 명령을 비밀번호 없이 여는 `ALL=(ALL) NOPASSWD: ALL` 설정은 권장하지 않습니다.

## 6. SSH 터널 열기

서버에서 나갑니다. 아래 명령은 Mac/Windows 공통입니다.

```bash
exit
```

내 PC에서 터널을 열며 다시 접속합니다.

Mac:

```bash
ssh -i "$KEY" -L 7717:127.0.0.1:7717 ubuntu@"$SERVER_IP"
```

Windows PowerShell:

```powershell
ssh -i $KEY -L 7717:127.0.0.1:7717 ubuntu@$SERVER_IP
```

## 7. 설치 마법사 시작

터널로 접속된 서버 안에서 실행합니다. 아래 명령은 Mac/Windows 공통입니다.

```bash
DOMAIN="example.com"
g7inst setup --domain "$DOMAIN"
```

`example.com`은 실제 도메인으로 바꿉니다. root가 아니면 설치기가 자동으로 `sudo` 재실행을 시도합니다. sudo 비밀번호가 필요하면 SSH 터미널에 입력합니다.

성공하면 터미널에 이런 주소가 나옵니다.

```text
http://127.0.0.1:7717/?token=...
```

## 8. 브라우저에서 열기

내 PC 브라우저에서 터미널에 나온 `http://127.0.0.1:7717/?token=...` 주소를 엽니다. Mac/Windows 모두 같은 주소를 엽니다.

웹 UI에는 root/ubuntu 서버 비밀번호를 입력하지 않습니다.

## 9. 웹 화면에서 입력할 값

기본값은 아래 기준입니다.

| 항목 | 값 |
| --- | --- |
| 도메인 | 실제 도메인 |
| 웹서버 | Nginx |
| PHP | 8.5 |
| DB | MySQL |
| Redis | 사용 |
| 메일 | 서버 Postfix 발송 |
| 앱 | 그누보드7 |
| 사이트 계정 | 예: `g7` |
| 사이트 계정 비밀번호 | 새로 정한 SFTP/파일관리 비밀번호 |

진행 순서:

1. 접속 확인
2. 서버 점검
3. 설치 방식
4. 사양 확정
5. 기본 구성
6. 결과
7. 세부 설정

## 10. 설치 후 확인

서버 터미널입니다. 아래 명령은 Mac/Windows 공통입니다.

```bash
sudo cat /var/log/g7-installer/report.json
sudo less /var/log/g7-installer/setup-guide.md
```

브라우저:

```text
http://example.com
https://example.com
```

`example.com`은 실제 도메인으로 바꿉니다.

SSL은 DNS/IP가 맞고 Let's Encrypt 발급 조건이 통과해야 적용됩니다.

`completed`는 서버 프로비저닝 완료입니다. CMS 관리자 설치까지 끝난 상태는 아닙니다. 결과 리포트의 `앱 링크`를 열어 다음을 진행합니다.

- 그누보드7: 공식 `/install` 화면에서 Composer/Vendor, 관리자 계정, 확장, 마이그레이션을 완료합니다. 설치기는 그 전에 최신 안정 Release, 필수 빌드 파일, `.env.example` 기반 `.env`와 사이트 계정 전용 `0600` 권한까지 준비합니다.
- WordPress: 공식 `/wp-admin/install.php` 화면에서 사이트 제목과 관리자 계정을 만듭니다.

`www` 사용 여부와 HTTPS 적용 결과에 따라 주소가 달라질 수 있으므로 URL을 직접 조합하지 말고 결과 리포트의 링크를 사용합니다.

## 11. 다시 설치해야 할 때

신규 VPS 테스트에서 설치기가 만든 항목을 지우고 다시 시도하려면:

```bash
sudo g7inst reset --yes
```

기존 Let's Encrypt 인증서는 중복 발급 제한을 피하기 위해 보존 우선입니다.

이 명령은 신규 VPS 재설치용입니다. 공식 CMS 설치를 마쳤거나 운영 데이터가 생겼다면 먼저 VPS 스냅샷이나 별도 백업을 만드세요.

## 막히면

| 증상 | 처리 |
| --- | --- |
| SSH 접속 실패 | IP, 키 파일 경로, 키 권한 확인 |
| `g7inst: command not found` | 4번 bootstrap 다시 실행 |
| sudo 비밀번호를 모름 | root SSH, `su -`, VPS 콘솔에서 관리자 권한 확보 |
| 브라우저 접속 안 됨 | SSH 터널 창이 열려 있는지 확인 |
| 세션 쿠키 없음 | 터미널에 나온 token URL을 다시 열기 |
| SSL 실패 | DNS A/CNAME, Cloudflare DNS only, 80/443 방화벽 확인 |
