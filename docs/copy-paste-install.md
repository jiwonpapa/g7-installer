# 따라하기식 설치 매뉴얼

새 Ubuntu 24.04 VPS에 `g7inst`를 설치하고 웹 마법사를 여는 가장 짧은 절차입니다. 기존 운영 서버에는 실행하지 마세요.

## 1. 준비할 값

| 값 | 예시 |
| --- | --- |
| 서버 공인 IP | `203.0.113.10` |
| SSH 계정 | Ubuntu 이미지라면 보통 `ubuntu` |
| SSH 개인키 | `YOUR_KEY.pem` |
| 도메인 | 웹 마법사에서 입력 |

VPS 제공자 방화벽은 `22`, `80`, `443`만 엽니다. `7717`, `3306`, `6379`는 열지 않습니다.

## 2. 접속 방식 하나 선택

아래 명령 한 줄이 다음 작업을 모두 처리합니다.

1. VPS에 SSH 접속
2. 설치 화면용 SSH 터널 생성
3. 최신 `g7inst` 설치 또는 업데이트
4. 웹 설치 마법사 실행

### A. `.pem` 개인키로 접속

Lightsail을 포함한 대부분의 클라우드 Ubuntu 이미지는 이 방식을 사용합니다.

Mac 터미널:

```bash
ssh -i "$HOME/.ssh/YOUR_KEY.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.8/bootstrap.sh | sudo bash && sudo g7inst setup'
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\YOUR_KEY.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.8/bootstrap.sh | sudo bash && sudo g7inst setup'
```

바꿀 값:

- `YOUR_KEY.pem`: 내려받은 개인키 파일명
- `SERVER_IP`: VPS 공인 IP
- `ubuntu`: VPS 제공자가 안내한 SSH 계정이 다를 때만 변경

개인키 파일을 아직 옮기지 않았다면 먼저 처리합니다.

Mac 터미널:

```bash
mkdir -p "$HOME/.ssh"
mv "$HOME/Downloads/YOUR_KEY.pem" "$HOME/.ssh/YOUR_KEY.pem"
chmod 600 "$HOME/.ssh/YOUR_KEY.pem"
```

Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\.ssh" | Out-Null
Move-Item "$env:USERPROFILE\Downloads\YOUR_KEY.pem" "$env:USERPROFILE\.ssh\YOUR_KEY.pem"
icacls "$env:USERPROFILE\.ssh\YOUR_KEY.pem" /inheritance:r /grant:r "$($env:USERNAME):(R)"
```

### B. SSH 비밀번호로 접속

VPS 업체가 SSH 계정과 비밀번호를 제공한 경우에만 사용합니다. Mac 터미널과 Windows PowerShell에서 같은 명령을 실행합니다.

```bash
ssh -t -L 7717:127.0.0.1:7717 SSH_USER@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.8/bootstrap.sh | sudo bash && sudo g7inst setup'
```

바꿀 값:

- `SSH_USER`: VPS 접속 계정
- `SERVER_IP`: VPS 공인 IP

SSH 비밀번호와 sudo 비밀번호를 물으면 터미널에 입력합니다. 비밀번호를 명령어에 넣거나 웹 설치 화면에 입력하지 않습니다.

## 3. 브라우저 열기

설치가 시작되면 터미널에 아래 형태의 주소가 나옵니다.

```text
http://127.0.0.1:7717/?token=...
```

주소 전체를 복사해 같은 PC의 브라우저에서 엽니다.

- 도메인은 웹 마법사에서 한 번만 입력합니다.
- 설치 중에는 터미널 창을 닫지 않습니다.
- `7717/tcp`는 외부 방화벽에 열지 않습니다.
- 설치를 마치면 터미널에서 `Ctrl+C`로 종료합니다.

## 4. 마법사 진행

웹 화면에서 다음 순서로 진행합니다.

1. 서버 상태 점검
2. 도메인과 사이트 계정 입력
3. 설치 옵션 확인
4. 설치 계획 확인
5. 패키지와 서버 설정 설치
6. 결과 리포트 확인
7. 결과의 앱 링크에서 CMS 설치 마무리

`사이트 계정 비밀번호`는 서버 SSH 비밀번호가 아닙니다. 설치기가 만들 SFTP/파일관리 계정의 새 비밀번호입니다.

## 5. 완료 기준

- 리포트 단계가 `completed`입니다.
- 웹서버, PHP, DB, SSL 검증 결과가 표시됩니다.
- `/var/log/g7-installer/setup-guide.md`가 생성됩니다.
- 결과 리포트의 앱 링크가 열립니다.

`completed`는 서버 프로비저닝 완료를 뜻합니다. 그누보드7의 관리자 계정과 CMS 내부 설정은 결과의 공식 `/install` 화면에서 마칩니다.

## 6. 중단됐을 때와 다시 설치할 때

설치 중 오류가 나면 결과 화면에서 stderr와 자동복원 상태를 확인합니다. 설치기는 원인을 자동 수정하지 않으므로 설치기 업데이트나 입력·환경 수정 후 `수정 후 현재 단계 재실행`을 누릅니다. 터미널에서는 아래와 같습니다.

```bash
sudo g7inst resume
```

완료 단계는 건너뛰고 실패 단계만 다시 적용합니다.

현재 단계 재시도로 해결할 수 없고 설치를 완전히 포기할 때만, 운영 데이터가 생기기 전의 신규 VPS에서 아래 초기화를 사용합니다.

웹 화면에서는 확인 입력란에 `초기화`를 정확히 입력해야 실행 버튼이 활성화됩니다. 그누보드7 DB와 설치 완료 잠금 파일이 확인된 경우에는 이미 설치가 끝난 사이트임을 별도로 경고합니다. 초기화하면 사이트 계정, 웹파일, DB/DB 계정, 서비스, 설정과 설치 패키지가 삭제되며 이 설치기로 복구할 수 없습니다. Let's Encrypt 인증서는 보존합니다.

```bash
sudo g7inst reset --yes
```

설치기가 만든 파일, 계정, DB, 패키지와 메타데이터만 제거합니다. 운영 중인 사이트의 백업 복구 기능은 아닙니다.

## 7. 막히면 확인

| 증상 | 확인할 것 |
| --- | --- |
| `Permission denied (publickey)` | 개인키 경로, 파일 권한, SSH 계정명 |
| 비밀번호 로그인이 안 됨 | VPS 업체가 SSH 비밀번호 로그인을 허용했는지 확인 |
| sudo 실패 | sudo 가능한 계정인지 확인하거나 VPS 콘솔에서 root 권한 확보 |
| 브라우저 접속 실패 | 명령을 실행한 터미널이 열려 있는지 확인 |
| `7717` 연결 거부 | 서버의 `g7inst setup`이 종료됐는지 확인하고 한 줄 명령 재실행 |
| 설치 세션 없음 | 터미널에 새로 출력된 token 주소 전체를 다시 열기 |
| 설치 단계 실패 | 원인 수정 후 `수정 후 현재 단계 재실행`; 터미널은 `sudo g7inst resume` |

## 용어 설명

| 용어 | 뜻 |
| --- | --- |
| SSH 개인키 | 비밀번호 대신 서버 접속을 증명하는 파일 |
| SSH 비밀번호 | VPS 업체가 허용한 경우 터미널에서 입력하는 서버 접속 비밀번호 |
| SSH 터널 | `7717`을 인터넷에 공개하지 않고 내 PC 브라우저로 연결하는 통로 |
| sudo | 일반 계정에서 관리자 권한 명령을 실행하는 기능 |
| bootstrap | 최신 `g7inst` 바이너리를 설치하거나 업데이트하는 과정 |
| 접속 확인 주소 | 터미널에 출력되는 `http://127.0.0.1:7717/?token=...` 주소 |

서버 생성, DNS, SSH 키 저장을 자세히 보려면 [Lightsail 상세 안내](lightsail-ubuntu24-setup-guide.md)를 확인합니다.
