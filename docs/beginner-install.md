# 초보용 설치 안내

목표는 실제 VPS에서 `g7inst` 웹 마법사를 열고 `서버 점검 -> 설치 방식 -> 사양 확정 -> 서버 세팅 실행 -> 결과` 순서로 진행하는 것입니다.

대상은 그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해본 사용자입니다. 서버 명령은 복사해서 따라 하고, 낯선 용어는 웹 화면의 `?` 도움말과 아래 용어 설명에서 확인합니다.

현재 문서는 Public Beta 기준입니다. 새 Ubuntu 24.04 VPS에서 그누보드7 또는 WordPress 설치 기반을 만드는 흐름만 다룹니다.

## 전체 순서

1. Lightsail Ubuntu 24.04 서버 생성
2. 방화벽 22/80/443만 열기
3. 고정 IP 연결
4. DNS 연결
5. SSH 키 저장
6. 서버 접속과 sudo 확인
7. 필요하면 VPS 백업/스냅샷 확인
8. 개인키 또는 SSH 비밀번호용 한 줄 명령 실행
9. 브라우저에서 접속 확인 주소 열기
10. 웹 마법사에서 도메인 입력

## 준비물

> **필요한 것**
>
> - 도메인
> - AWS Lightsail Ubuntu 24.04 서버
> - 서버 고정 IP
> - SSH 개인키 `.pem` 또는 VPS 업체가 제공한 SSH 비밀번호
> - Mac 터미널 또는 Windows PowerShell

## 바꿔 넣을 값

| 표시 | 실제 값 |
| --- | --- |
| `SERVER_IP` | Lightsail 고정 IP |
| `YOUR_LIGHTSAIL_KEY.pem` | 내려받은 SSH 키 파일명 |
| `SSH_USER` | VPS 접속 계정. Ubuntu 이미지는 보통 `ubuntu` |

## 1. AWS 콘솔을 한글로 바꾸기

1. AWS 콘솔에 로그인합니다.
2. 우측 상단 톱니바퀴를 누릅니다.
3. `언어`를 `한국어`로 바꿉니다.

## 2. Lightsail 인스턴스 만들기

> **권장값**
>
> - OS: Ubuntu 24.04 LTS
> - 블루프린트: OS 전용
> - 네트워크: 듀얼 스택
> - 크기: 2GB 메모리, 2 vCPU, 60GB SSD, 3TB 전송

설치 계획 화면에는 1GB, 2GB, 4GB, 8GB, 16GB, 32GB와 32GB 초과 메모리 기준 튜닝값이 표시됩니다. 서버 사양이 커져도 PHP-FPM, DB, Redis, swap, Nginx worker, Apache `mpm_event` worker 값을 같은 기준으로 확인할 수 있습니다.

1. Lightsail에서 `인스턴스 생성`을 누릅니다.
2. `Linux/Unix`를 선택합니다.
3. `OS 전용`을 선택합니다.
4. `Ubuntu 24.04 LTS`를 선택합니다.
5. 권장 크기를 선택합니다.
6. SSH 키를 만들고 `.pem` 파일을 다운로드합니다.
7. `시작 스크립트 추가`에 아래 스크립트를 넣습니다.

```sh
#!/bin/sh
set -eu
LOG=/var/log/g7-lightsail-bootstrap.log
exec >"$LOG" 2>&1
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh -o "$tmp"
bash "$tmp"
g7inst --version
```

8. 인스턴스 이름을 입력합니다.
9. `인스턴스 생성`을 누릅니다.

## 3. 방화벽 열기

이 단계는 Lightsail 등 VPS 제공자 콘솔에서 직접 합니다. G7 Installer는 UFW·fail2ban을 설치하거나 방화벽 규칙을 변경하지 않습니다.

메뉴:

```text
Lightsail -> 인스턴스 -> 네트워킹 -> IPv4 방화벽
```

열 포트:

| 포트 | 용도 |
| --- | --- |
| 22/tcp | SSH |
| 80/tcp | HTTP |
| 443/tcp | HTTPS |

열지 말 것:

```text
7717/tcp
3306/tcp
6379/tcp
메일 수신 포트
```

## 4. 고정 IP 연결

1. Lightsail에서 `네트워킹`으로 갑니다.
2. `고정 IP 생성`을 누릅니다.
3. 방금 만든 인스턴스에 연결합니다.
4. 고정 IP를 기록합니다.

## 5. DNS 연결

Cloudflare를 쓰면 설치 중에는 프록시를 끕니다.

| 타입 | 이름 | 값 | 상태 |
| --- | --- | --- | --- |
| A | `@` | 서버 고정 IP | DNS only |
| CNAME | `www` | 루트 도메인 | DNS only |

메일 보내기 테스트를 할 예정이면 추가합니다.

| 타입 | 이름 | 값 | 상태 |
| --- | --- | --- | --- |
| A | `mail` | 서버 고정 IP | DNS only |

## 6. SSH 키 저장하기

### Mac

```bash
mkdir -p ~/.ssh
mv ~/Downloads/YOUR_LIGHTSAIL_KEY.pem ~/.ssh/lightsail_g7inst.pem
chmod 600 ~/.ssh/lightsail_g7inst.pem
```

### Windows PowerShell

```powershell
mkdir $env:USERPROFILE\.ssh
Move-Item "$env:USERPROFILE\Downloads\YOUR_LIGHTSAIL_KEY.pem" "$env:USERPROFILE\.ssh\lightsail_g7inst.pem"
icacls "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" /inheritance:r /grant:r "$($env:USERNAME):(R)"
```

## 7. 서버 접속 확인

Mac:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem ubuntu@SERVER_IP
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" ubuntu@SERVER_IP
```

프롬프트가 `ubuntu@...`로 바뀌면 서버 안에서 아래를 입력합니다.

```bash
sudo -n true && echo "sudo OK"
g7inst --version || true
```

`sudo OK`가 나오면 비밀번호 없이 설치 준비가 된 상태입니다.

Lightsail Ubuntu는 보통 서버 비밀번호를 몰라도 됩니다. `ubuntu` 계정으로 SSH 키 접속하고, 비밀번호 없는 `sudo`로 설치기를 관리자 권한 실행합니다.

다른 VPS에서 `sudo OK`가 나오지 않아도 sudo 비밀번호를 아는 계정이면 진행할 수 있습니다. 9단계 한 줄 명령이 요청할 때 SSH 터미널에 입력합니다. sudo 자체가 실패하면 root SSH, `su -`, 또는 VPS 콘솔에서 관리자 권한을 먼저 확보해야 합니다.

### `g7inst`가 아직 없을 때

Lightsail 시작 스크립트는 인스턴스가 처음 켜진 뒤 잠시 실행됩니다. 먼저 로그를 확인합니다.

```bash
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
```

로그가 끝났는데도 `g7inst: command not found`가 나오면 서버 안에서 아래를 한 번 실행합니다.

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl
tmp="$(mktemp)"
curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh -o "$tmp"
sudo bash "$tmp"
rm -f "$tmp"
g7inst --version
```

확인을 마치면 서버에서 `exit`로 나옵니다. 실제 설치는 9단계 한 줄 명령으로 다시 접속하며 시작합니다.

### 선택: g7inst만 비밀번호 없이 sudo 허용

매번 sudo 비밀번호를 묻는 서버라면 `g7inst` 명령만 비밀번호 없이 실행되도록 제한할 수 있습니다. root 또는 sudo 가능한 계정으로 한 번 접속한 뒤 진행합니다.

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

`ALL=(ALL) NOPASSWD: ALL`처럼 모든 명령을 비밀번호 없이 여는 설정은 권장하지 않습니다. 설치가 끝난 뒤 이 예외가 필요 없으면 아래처럼 제거합니다.

```bash
sudo rm /etc/sudoers.d/g7inst
```

## 8. 선택: VPS 백업/스냅샷 확인

신규 테스트 서버라면 건너뛰어도 됩니다.

운영 데이터가 있거나 되돌릴 지점이 필요하면 먼저 확인합니다.

1. Lightsail에서 인스턴스를 선택합니다.
2. `스냅샷` 탭을 엽니다.
3. 비용과 생성 시간을 확인합니다.
4. 필요할 때만 `스냅샷 생성`을 누릅니다.

## 9. 설치 웹 UI 열기

SSH 접속 방식에 맞는 명령 하나만 실행합니다. SSH 연결, 터널, 최신 `g7inst` 설치 또는 업데이트, 웹 마법사 실행이 한 번에 진행됩니다.

### `.pem` 개인키 방식

Mac 터미널:

```bash
ssh -i "$HOME/.ssh/lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh | sudo bash && sudo g7inst setup'
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh | sudo bash && sudo g7inst setup'
```

### SSH 비밀번호 방식

Mac 터미널과 Windows PowerShell에서 같은 명령을 사용합니다.

```bash
ssh -t -L 7717:127.0.0.1:7717 SSH_USER@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/latest/download/bootstrap.sh | sudo bash && sudo g7inst setup'
```

SSH 비밀번호와 sudo 비밀번호를 물으면 터미널에 입력합니다. 비밀번호는 명령어나 웹 화면에 넣지 않습니다.

브라우저에서 터미널에 나온 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

웹 화면에서는 root/ubuntu 서버 비밀번호를 입력하지 않습니다. 도메인은 웹 마법사에서 한 번만 입력합니다.

`사이트 계정 비밀번호`는 새로 정합니다. 이 값은 설치기가 만들 `g7` 같은 사이트 계정의 SFTP/파일관리 비밀번호입니다. sudo 권한은 주지 않습니다.

PHP 기본값은 8.5입니다. 설치기가 최신 PHP용 `ppa:ondrej/php` apt 소스를 자동 추가한 뒤 설치합니다. PHP 8.3을 선택하면 Ubuntu 24.04 기본 apt 소스를 사용합니다.

웹 UI 기본 조합은 `Nginx / PHP 8.5 / Ubuntu 24.04 apt의 MySQL / www로 통일 / Redis 사용 / 메일 발송 안 함 / 그누보드7`입니다. 외부 SMTP를 선택하면 계정과 비밀번호를 필수로 받고 비밀번호는 루트 전용 비밀 파일에만 저장합니다. 로컬 Postfix는 발신 IP 평판·PTR·25번 포트 정책을 직접 관리할 사용자만 선택합니다.

## 완료 기준

- 브라우저에 `G7 Installer` 화면이 뜹니다.
- 서버 점검 단계가 보입니다.
- 서버 세팅은 `패키지 설치/검증 -> 사이트 계정/웹루트 -> 웹서버 vhost/HTTP 검증 -> PHP/런타임 튜닝 -> DB 튜닝/계정 생성 -> SSL 인증서/HTTPS 검증 -> 웹앱 파일 배치 -> 리포트 생성 -> 세부 설정 액션 패널` 순서로 진행됩니다.
- 리포트에 `completed`가 보이면 서버 프로비저닝이 끝난 것입니다. 패키지, Nginx/Apache 도메인 연결, PHP/DB 튜닝, DB 계정, 앱 파일 배치, 설정 안내서 저장까지 완료된 상태입니다. CMS 관리자 설치 완료를 뜻하지는 않습니다.
- 그누보드7은 GitHub 공식 최신 안정 Release와 필수 빌드 파일을 검증하고 `.env.example`에서 사이트 계정 전용 `0600` 권한의 `.env`를 준비한 뒤 공식 `/install`로 넘깁니다. 결과 리포트의 `앱 링크`를 열어 Composer/Vendor, 관리자 계정, 확장과 마이그레이션을 진행합니다. WordPress도 리포트의 `/wp-admin/install.php` 링크에서 공식 설치를 마칩니다.
- 중간 단계가 실패하면 `completed`로 표시하지 않습니다. 실패 단계와 중단 원인을 확인한 뒤 재설치 초기화 또는 VPS 백업 복원 후 다시 진행합니다.
- 사이트 계정과 `/home/계정/public_html` 웹루트가 만들어집니다.
- DNS/IP와 SSL 발급이 통과하면 `https://도메인` 접속이 됩니다.
- 서버에 `/var/log/g7-installer/setup-guide.md` 설정 안내서가 저장됩니다. 웹루트, DB, 인증서, PHP 런타임, 앱 systemd unit, 재시작 명령을 여기서 확인합니다.
- 설치 중에는 SSH 터널 창을 닫지 않습니다.

## 막히면 확인

| 증상 | 먼저 볼 것 |
| --- | --- |
| `Permission denied` | SSH 키 파일 경로와 권한 |
| `sudo OK`가 안 나옴 | `g7inst setup`을 일반 계정에서 실행해 sudo 재실행을 시도하고, 안 되면 root SSH/`su -`/VPS 콘솔 확인 |
| `g7inst: command not found` | 시작 스크립트 로그 확인 후 위의 수동 bootstrap 실행 |
| 브라우저가 안 열림 | SSH 터널 창이 열려 있는지 확인 |
| 설치 세션 쿠키 없음 | 터미널에 나온 접속 확인 주소를 다시 열기 |

```bash
sudo tail -120 /var/log/g7-lightsail-bootstrap.log
g7inst doctor
```

## 용어 설명

| 용어 | 뜻 |
| --- | --- |
| VPS | 클라우드에 만든 가상 서버 |
| 고정 IP | 서버 주소로 쓸 공인 IP |
| A 레코드 | 도메인을 서버 IP에 연결하는 DNS 설정 |
| DNS only | Cloudflare 프록시를 끄고 실제 IP를 보여주는 상태 |
| SSH 키 | 비밀번호 대신 서버에 접속하는 파일 |
| SSH 터널 | 7717 포트를 외부에 열지 않고 내 PC에서만 여는 방법 |
| sudo | 일반 계정으로 관리자 권한 명령을 실행하는 방법 |
| 사이트 계정 | 웹파일 소유자이자 SFTP로 파일을 올릴 Linux 계정 |
| PHP 런타임 사용자 | PHP-FPM pool을 실행하는 계정. 기본은 사이트 계정 |
| 시작 스크립트 | 서버가 처음 만들어질 때 자동 실행되는 명령 |
| VPS 백업/스냅샷 | 서버 전체를 되돌릴 수 있게 저장한 복구 지점. 비용과 시간이 들 수 있음 |
| vhost | 도메인을 특정 웹루트에 연결하는 웹서버 설정 |
| 접속 확인 주소 | 터미널에 출력되는 `http://127.0.0.1:7717/?token=...` 주소 |

더 자세한 설명은 [Lightsail 상세 안내](lightsail-ubuntu24-setup-guide.md)를 봅니다.
