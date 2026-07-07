# 초보용 설치 안내

목표는 `g7inst` 웹 마법사 화면을 여는 것입니다. 설명은 짧게 두고, 그대로 따라 할 순서만 적습니다.

## 준비물

> **필요한 것**
>
> - 도메인
> - AWS Lightsail Ubuntu 24.04 서버
> - 서버 고정 IP
> - SSH 개인키 `.pem`
> - Mac 터미널 또는 Windows PowerShell

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

서버에서 확인합니다.

```bash
g7inst --version
g7inst doctor
```

## 8. 설치 전 스냅샷 찍기

1. Lightsail에서 인스턴스를 선택합니다.
2. `스냅샷` 탭을 엽니다.
3. `스냅샷 생성`을 누릅니다.
4. 스냅샷 이름에 날짜를 넣습니다.

## 9. 설치 웹 UI 열기

Mac:

```bash
ssh -i ~/.ssh/lightsail_g7inst.pem -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\lightsail_g7inst.pem" -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP
```

서버에 접속된 상태에서 실행합니다.

```bash
sudo g7inst setup --domain example.com
```

브라우저에서 터미널에 나온 주소를 엽니다.

```text
http://127.0.0.1:7717/?token=...
```

## 완료 기준

- 브라우저에 `G7 Installer` 화면이 뜹니다.
- 서버 점검 단계가 보입니다.
- 설치 완료 후 리포트에 `vhost-enabled`가 보입니다.
- `http://도메인` 접속이 됩니다.
- 설치 중에는 SSH 터널 창을 닫지 않습니다.

## 막히면 확인

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
| 시작 스크립트 | 서버가 처음 만들어질 때 자동 실행되는 명령 |
| 스냅샷 | 서버 전체를 되돌릴 수 있게 저장한 복구 지점 |
| vhost | 도메인을 특정 웹루트에 연결하는 웹서버 설정 |

더 자세한 설명은 [Lightsail 상세 안내](lightsail-ubuntu24-setup-guide.md)를 봅니다.
