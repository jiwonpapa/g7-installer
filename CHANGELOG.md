# 변경 기록

형식은 Keep a Changelog 원칙을 따르며 버전은 Semantic Versioning을 사용합니다.

## Unreleased

## 0.2.32 - 2026-07-10

### Fixed

- G7 공식 빌드 파일을 삭제하던 사전 `npm run build` 실행 제거
- 재설치 초기화가 사이트 계정의 잔존 프로세스 때문에 `userdel` 코드 8로 중단되던 문제 수정
- 설치 화면과 우측 패널에 같은 실시간 로그가 중복 표시되던 문제 수정

### Changed

- G7 GitHub 최신 안정 Release를 실행 시 조회·clone하고 최종 배포 Git 무결성을 재검증
- G7 앱 설정을 공식 브라우저 `/install`로 인계하고 사전 Composer·NPM·Artisan 실행 제거
- WordPress 공식 `latest.zip` 설치 계약을 유지하고 앱 파일 권한을 소유자 기준으로 제한
- 재설치 초기화에서 사이트 계정 프로세스·로그인 세션만 종료하고 컨트롤러·SSH 터널·인증서는 보존

## 0.2.31 - 2026-07-10

### Added

- 설치·초기화·되돌리기 명령의 stdout/stderr 실시간 웹 로그와 재접속 이력 복원
- 웹 복구 패널의 안전 단계 `이어서 진행`
- RAM, 가용 메모리, swap, 디스크, inode 사전점검과 저사양 안전 기본값

### Changed

- swap을 apt 설치 전에 구성하고 1GB급 서버에서 Redis/Postfix를 기본 해제
- 계획 상세를 접이식 2열 구성으로 정리하고 모바일 오버플로 E2E를 추가
- UFW·fail2ban 설치·변경을 범위 밖으로 명시하고 관련 실행 경로를 제거
- VPS 하네스 셸 권한 래퍼와 Certbot README 오탐을 수정

## 0.2.30 - 2026-07-10

### Added

- 중단된 설치의 안전한 후반 단계를 재개하는 `g7inst resume`
- 전역 작업 잠금, 원자적 상태 저장, JSONL 명령 감사 로그
- 실제 상태 파일과 서비스 점검을 반영하는 `g7inst status`
- 승인형 폐기 가능 VPS 운영 하네스와 릴리스 SBOM/provenance

### Changed

- doctor가 서비스·포트·설정 상태를 확인할 수 없으면 설치를 차단
- 운영 하네스가 전체 리포트 계약, 앱 스모크, 초기화 자원 제거와 인증서 보존을 검증

## 0.2.29 - 2026-07-09

- 설치 엔진 모듈 분리와 운영 하네스 기반을 정리했습니다.
