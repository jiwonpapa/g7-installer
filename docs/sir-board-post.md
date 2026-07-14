# SIR 게시판 소개글 초안

아래 `게시용 본문`만 복사해 SIR 게시판에 올릴 수 있습니다. 릴리스 버전이나 스크린샷은 게시 시점에 맞춰 추가합니다.

## 제목

```text
[공개 베타] 새 Ubuntu VPS에 그누보드7 서버를 준비하는 G7 Installer
```

## 게시용 본문

````markdown
새 Ubuntu 22.04 이상 VPS에서 그누보드7을 올릴 때, 서버 패키지와 도메인 연결을 웹 마법사로 준비하는 `G7 Installer`를 공개 베타로 배포합니다.

GitHub: https://github.com/jiwonpapa/g7-installer

대상은 그누보드 설치, 관리자 설정, FTP/SFTP 업로드 정도는 해보셨고 SSH 접속과 `sudo` 사용이 가능한 분입니다. 기존 운영 서버를 자동 전환하는 도구는 아닙니다.

### 하는 일

- 새 Ubuntu 22.04 이상 VPS 점검과 설치 가능 여부 확인
- Nginx 권장 또는 Apache 호환 구성, PHP 8.3·MySQL 8.0 운영 권장 프로필, PHP 8.5·MySQL 8.4 LTS 최신 지원 프로필, Redis, PHP-FPM 사이트 계정 pool 설정
- RAM과 vCPU에 맞춘 PHP, DB, Redis, swap 기준값 적용 및 리포트 저장
- 도메인과 `www` DNS/IP 확인, vhost 생성, Let's Encrypt 인증서 발급 또는 기존 인증서 재사용·갱신 점검
- 사이트 Linux 계정, 웹루트, DB와 DB 계정 생성
- 그누보드7은 GitHub 공식 최신 안정 Release를 매번 받아 Git 무결성·필수 빌드 파일을 검증하고, `.env.example` 기반 `.env`를 사이트 계정 전용 `0600` 권한으로 준비
- 공식 웹 설치 후 Redis 캐시·세션·큐, scheduler, Reverb, storage link와 Laravel 실효 설정을 후속 검증
- 설치 중 실시간 로그, 중단 리포트, 안전한 이어서 진행, 상세 설정 안내서 제공

### 먼저 알아둘 범위

- `completed`는 서버 프로비저닝 완료입니다. CMS 관리자 설치와 G7 런타임 마무리까지 끝났다는 뜻은 아닙니다.
- 그누보드7은 결과 리포트의 `/install` 링크에서 공식 설치 화면을 진행한 뒤 설치 안내서의 `G7 런타임 설정 적용`으로 마무리합니다.
- UFW, fail2ban, 기존 운영 서버 이전, 운영 데이터 백업은 범위 밖입니다. VPS 제공자 방화벽과 별도 유지보수 도구로 관리합니다.
- 재설치 초기화는 신규 VPS 테스트용입니다. 운영 데이터가 있으면 먼저 VPS 스냅샷 또는 별도 백업을 만드세요. 기존 Let's Encrypt 인증서는 중복 발급 제한을 피하기 위해 보존 우선입니다.

### 설치 전 준비

1. 새 Ubuntu 22.04 이상 VPS를 준비합니다.
2. 도메인 A 레코드는 서버 공인 IP로, `www`는 루트 도메인 CNAME으로 연결합니다.
3. VPS 제공자 방화벽에서 `22/tcp`, `80/tcp`, `443/tcp`만 엽니다. `7717`, `3306`, `6379`은 외부에 열지 않습니다.
4. Cloudflare 사용 시 인증서 발급 중에는 프록시를 `DNS only`로 둡니다.

### 설치 순서

`.pem` 개인키를 사용하는 Mac은 아래 한 줄을 실행합니다.

```bash
ssh -i "$HOME/.ssh/YOUR_KEY.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.12/bootstrap.sh | sudo bash && sudo g7inst setup'
```

Windows PowerShell:

```powershell
ssh -i "$env:USERPROFILE\.ssh\YOUR_KEY.pem" -t -L 7717:127.0.0.1:7717 ubuntu@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.12/bootstrap.sh | sudo bash && sudo g7inst setup'
```

SSH 비밀번호 로그인을 허용하는 VPS는 Mac과 Windows에서 아래 명령을 사용합니다.

```bash
ssh -t -L 7717:127.0.0.1:7717 SSH_USER@SERVER_IP 'curl -fsSL https://github.com/jiwonpapa/g7-installer/releases/download/v0.3.0-beta.12/bootstrap.sh | sudo bash && sudo g7inst setup'
```

SSH 비밀번호와 sudo 비밀번호는 터미널에만 입력합니다. 터미널에 출력된 아래 형태의 정확한 주소를 같은 PC 브라우저에서 엽니다. 설치가 끝날 때까지 터미널을 닫지 않습니다.

```text
http://127.0.0.1:7717/?token=...
```

웹 UI에서 도메인, 사이트 계정/SFTP 비밀번호, DB 계정, 웹서버와 PHP 옵션을 확인한 뒤 진행하면 됩니다. 서버 root 비밀번호는 웹 UI에 입력하지 않습니다. 사이트 계정 비밀번호는 SFTP/파일 관리용이며 sudo 권한을 주지 않습니다.

설치가 끝나면 결과 리포트의 `앱 링크`를 열어 그누보드7 공식 설치 화면을 마무리합니다.

### 참고 문서

- 복붙형 Mac/Windows 설치: https://github.com/jiwonpapa/g7-installer/blob/main/docs/copy-paste-install.md
- 초보자용 Lightsail 설치: https://github.com/jiwonpapa/g7-installer/blob/main/docs/beginner-install.md
- Lightsail 상세 안내: https://github.com/jiwonpapa/g7-installer/blob/main/docs/lightsail-ubuntu24-setup-guide.md

버그 제보에는 사용한 VPS 종류, Ubuntu 버전, 선택한 웹서버/PHP/DB, 결과 리포트의 실패 항목과 민감정보를 지운 로그를 함께 남겨주시면 재현에 도움이 됩니다.
````
