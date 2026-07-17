# G7 Installer 소개

G7 Installer는 새 Ubuntu VPS에 그누보드7 실행 환경을 준비하는 설치 마법사입니다.

서버 접속, 패키지 설치, 웹서버 설정, PHP/DB 구성, SSL 인증서, 그누보드7 설치 화면 준비까지 웹 UI에서 단계별로 진행하는 것을 목표로 합니다.

## 왜 만들었나요?

그누보드7은 Laravel 기반이라 예전 PHP 게시판보다 서버 준비 항목이 많습니다.

초보 사용자가 SSH 명령, Nginx/Apache 설정, PHP-FPM, MySQL, Redis, SSL, 파일 권한을 한 번에 맞추기 어렵기 때문에 설치 과정을 점검 가능한 마법사로 묶었습니다.

## 누구에게 맞나요?

- 새 VPS에 그누보드7을 설치하려는 사용자
- 서버 명령은 익숙하지 않지만 순서대로 따라 할 수 있는 사용자
- 운영 서버가 아니라 신규 테스트 서버에서 먼저 검증하려는 사용자
- G7 설치 환경을 반복해서 확인해야 하는 개발자 또는 운영자

## 무엇을 해주나요?

- 서버 상태 점검
- Nginx 또는 Apache 기반 웹서버 설정
- PHP-FPM, PHP 확장, PHP 런타임 설정
- MySQL 설치와 앱 DB 계정 생성
- Redis, queue, scheduler, Reverb 같은 G7 런타임 준비
- 도메인 연결용 vhost 설정
- Let's Encrypt SSL 인증서 발급과 갱신 확인
- 설치 결과 리포트와 복구 안내서 생성
- 설치기가 만든 항목에 대한 되돌리기와 재설치 초기화

## 무엇을 하지 않나요?

- 기존 운영 사이트 위에 덮어쓰기
- 서버 전체를 임의로 튜닝하거나 방화벽을 마음대로 변경
- 외부 SMTP, Cloudflare, R2 같은 외부 서비스 계정 자동 생성
- 그누보드7 공식 브라우저 설치 화면 이후의 관리자 입력 자동 대행
- 문제 원인을 AI처럼 자동 판단해서 무단 수정

## 기본 설치 흐름

1. 새 Ubuntu VPS를 준비합니다.
2. 도메인 A 레코드를 VPS 공인 IP로 연결합니다.
3. SSH 터널 한 줄 명령으로 `g7inst`를 설치하고 웹 마법사를 엽니다.
4. 브라우저에서 `http://127.0.0.1:7717/?token=...` 주소에 접속합니다.
5. 서버 점검, 설치 옵션, 설치 계획을 확인합니다.
6. 설치를 실행하고 진행률과 로그를 확인합니다.
7. 결과 리포트의 앱 링크에서 그누보드7 공식 설치를 마무리합니다.
8. 설치 안내서의 G7 런타임 마무리 단계를 실행합니다.

## 현재 지원 범위

- Ubuntu 22.04 이상 신규 VPS
- 권장 기준: Ubuntu 24.04 LTS
- 공개 앱 패키지: 그누보드7
- 웹서버: Nginx 권장, Apache 호환 옵션
- DB: MySQL
- 접속 방식: SSH 터널 기반 로컬 웹 UI

## 주의사항

이 프로젝트는 현재 Public Beta입니다.

신규 VPS에서 테스트한 뒤 사용해야 하며, 기존 운영 서버에는 바로 실행하지 않는 것을 권장합니다. 설치 중 생성되는 리포트와 로그는 문제 재현과 복구 판단의 기준이 됩니다.

## 바로 보기

- [프로젝트 README](https://github.com/jiwonpapa/g7-installer)
- [따라하기식 설치 매뉴얼](https://github.com/jiwonpapa/g7-installer/blob/main/docs/copy-paste-install.md)
- [초보용 설치 안내](https://github.com/jiwonpapa/g7-installer/blob/main/docs/beginner-install.md)
- [Lightsail 상세 안내](https://github.com/jiwonpapa/g7-installer/blob/main/docs/lightsail-ubuntu24-setup-guide.md)
