# 초보용 설치 안내

목표는 실제 VPS에서 `g7inst` 웹 마법사를 열고 `서버 점검 -> 설치 방식 -> 사양 확정 -> 서버 세팅 실행 -> 결과` 순서로 진행하는 것입니다.

대상은 그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해본 사용자입니다. 서버 명령은 복사해서 따라 하고, 낯선 용어는 웹 화면의 `?` 도움말과 아래 용어 설명에서 확인합니다.

## 전체 순서

1. Lightsail Ubuntu 24.04 서버 생성
2. 방화벽 22/80/443만 열기
3. 고정 IP 연결
4. DNS 연결
5. SSH 키 저장
6. 서버 접속과 sudo 확인
7. 설치 전 스냅샷 생성
8. SSH 터널 열기
9. `sudo g7inst setup --domain 도메인` 실행
10. 브라우저에서 접속 확인 주소 열기

## 준비물

> **필요한 것**
>
> - 도메인
> - AWS Lightsail Ubuntu 24.04 서버
> - 서버 고정 IP
> - SSH 개인키 `.pem`
> - Mac 터미널 또는 Windows PowerShell

## 바꿔 넣을 값

| 표시 | 실제 값 |
| --- | --- |
| `SERVER_IP` | Lightsail 고정 IP |
| `example.com` | 실제 도메인 |
| `YOUR_LIGHTSAIL_KEY.pem` | 내려받은 SSH 키 파일명 |

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

설치 계획 화면에는 1GB, 2GB, 4GB, 8GB, 16GB, 32GB와 32GB 초과 메모리 기준 튜닝값이 표시됩니다. 서버 사양이 커져도 PHP-FPM/FrankenPHP, DB, Redis, swap, Nginx worker, Apache `mpm_event` worker 값을 같은 기준으로 확인할 수 있습니다.

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
apt-get update
apt-get install -y ca-certificates curl
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
curl -fsSL https://raw.githubusercontent.com/jiwonpapa/g7-installer/main/scripts/lightsail-init.sh -o "$tmp"
bash "$tmp"
```

8. 인스턴스 이름을 입력합니다.
9. `인스턴스 생성`을 누릅니다.

## 3. 방화벽 열기

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
g7inst --version
exit
```

`sudo OK`가 나오면 비밀번호 없이 설치 준비가 된 상태입니다.

서버 비밀번호는 몰라도 됩니다. Lightsail Ubuntu는 `ubuntu` 계정으로 SSH 키 접속하고, `sudo`로 설치기를 관리자 권한 실행합니다.

## 8. 설치 전 스냅샷 찍기

1. Lightsail에서 인스턴스를 선택합니다.
2. `스냅샷` 탭을 엽니다.
3. `스냅샷 생성`을 누릅니다.
4. 스냅샷 이름에 날짜를 넣습니다.

## 9. 설치 웹 UI 열기

먼저 SSH 터널을 열면서 서버에 접속합니다.

Mac:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

프롬프트가 `ubuntu@...`로 바뀌면 같은 터미널 창에서 설치 UI를 시작합니다.

```bash
sudo g7inst setup --domain example.com
```

`example.com`은 실제 도메인으로 바꿉니다.

브라우저에서 터미널에 나온 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

웹 화면에서는 root/ubuntu 서버 비밀번호를 입력하지 않습니다. 접속 확인 주소로 접속하면 바로 서버 점검 단계로 진행합니다.

`사이트 계정 비밀번호`는 새로 정합니다. 이 값은 설치기가 만들 `g7` 같은 사이트 계정의 SFTP/파일관리 비밀번호입니다. sudo 권한은 주지 않습니다.

PHP는 Ubuntu 24.04 기본값인 8.3이 권장 기본값입니다. PHP 8.5를 선택하면 설치기가 최신 PHP용 `ppa:ondrej/php` apt 소스를 자동 추가한 뒤 설치합니다.

## 완료 기준

- 브라우저에 `G7 Installer` 화면이 뜹니다.
- 서버 점검 단계가 보입니다.
- 서버 세팅은 `패키지 설치/검증 -> 사이트 계정/웹루트 -> 웹서버 vhost/HTTP 검증 -> PHP/런타임 튜닝 -> DB 튜닝/계정 생성 -> SSL 인증서/HTTPS 검증 -> 웹앱 파일 배치 -> 리포트 생성` 순서로 진행됩니다.
- 리포트에 `completed`가 보입니다. 이 값은 패키지, Nginx/Apache/FrankenPHP 도메인 연결, PHP/DB 튜닝, DB 계정, SSL 처리, 앱 설치, 설정 안내서 저장까지 끝났다는 뜻입니다. 그누보드7/Laravel은 Composer, NPM, Artisan, queue, scheduler 서비스까지 구성하고, WordPress는 설치 화면으로 이어집니다.
- 중간 단계가 실패하면 `completed`로 표시하지 않습니다. 실패 단계와 중단 원인을 확인한 뒤 재설치 초기화 또는 스냅샷 복원 후 다시 진행합니다.
- 사이트 계정과 `/home/계정/public_html` 웹루트가 만들어집니다.
- `https://도메인` 접속이 됩니다.
- 서버에 `/var/log/g7-installer/setup-guide.md` 설정 안내서가 저장됩니다. 웹루트, DB, 인증서, PHP 런타임, 앱 systemd unit, 재시작 명령을 여기서 확인합니다.
- 설치 중에는 SSH 터널 창을 닫지 않습니다.

## 막히면 확인

| 증상 | 먼저 볼 것 |
| --- | --- |
| `Permission denied` | SSH 키 파일 경로와 권한 |
| `sudo OK`가 안 나옴 | Lightsail 기본 `ubuntu` 계정인지 확인 |
| `g7inst: command not found` | 시작 스크립트 로그 확인 |
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
| PHP 런타임 사용자 | PHP-FPM pool 또는 FrankenPHP 서비스를 실행하는 계정. 기본은 사이트 계정 |
| 시작 스크립트 | 서버가 처음 만들어질 때 자동 실행되는 명령 |
| 스냅샷 | 서버 전체를 되돌릴 수 있게 저장한 복구 지점 |
| vhost | 도메인을 특정 웹루트에 연결하는 웹서버 설정 |
| 접속 확인 주소 | 터미널에 출력되는 `http://127.0.0.1:7717/?token=...` 주소 |

더 자세한 설명은 [Lightsail 상세 안내](lightsail-ubuntu24-setup-guide.md)를 봅니다.
