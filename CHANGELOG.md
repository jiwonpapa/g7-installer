# 변경 기록

형식은 Keep a Changelog 원칙을 따르며 버전은 Semantic Versioning을 사용합니다.

## Unreleased

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
