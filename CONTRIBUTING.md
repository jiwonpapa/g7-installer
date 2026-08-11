# 기여 안내

## 개발 순서

1. 이슈에서 변경 목적과 파괴 범위를 먼저 합의합니다.
2. `codex/` 접두사 브랜치에서 작은 단위로 수정합니다.
3. 공개 API와 파괴 작업에는 rustdoc과 회귀 테스트를 함께 추가합니다.
4. 변경 범위에 맞는 로컬 게이트를 통과시킨 뒤 Pull Request를 엽니다.

## 필수 기준

- 문서/웹 정적 변경: `bash scripts/static-gate.sh`
- Rust 로직 변경: `bash scripts/quick-gate.sh`
- 공유 경계, reset/rollback, 릴리스 후보: `bash scripts/quality-gate.sh`
- 릴리스 전 커버리지 확인: `bash scripts/coverage-gate.sh`
- 백업 전 산출물 정리: `bash scripts/clean-artifacts.sh --yes`
- 릴리스 정책 검증: `python3 scripts/check-release-policy.py`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo audit` 및 `cargo deny check`
- reset/rollback 변경 시 보존 리소스 golden 테스트
- 웹 UI 변경 시 Playwright E2E

실제 서버 검증은 승인된 폐기 가능 Ubuntu VPS에서 로컬 터미널로
`G7_OPS_CONFIRM_DISPOSABLE=1 bash scripts/ops-harness.sh`를 실행합니다. 운영 Let's Encrypt 발급은
일반 테스트에 사용하지 않습니다.

릴리스 태그는 `git tag -a vX.Y.Z -m "release X.Y.Z"` 형식의 annotated tag로 만들며,
버전 문자열 자체는 `X.Y.Z` 또는 `X.Y.Z-prerelease` SemVer입니다. `CHANGELOG.md`는
Keep a Changelog 형식으로 `Unreleased`와 링크형 릴리스 제목을 유지합니다.
`bash scripts/local-release-gate.sh`로 품질 게이트, 커버리지, 릴리스 산출물 생성을 로컬에서
검증합니다. Rust 빌드 캐시는 기본적으로 repo 밖 사용자 캐시 디렉터리에 유지합니다.
완전 임시 빌드가 필요할 때만 `G7_USE_TEMP_TARGET=1`을 사용합니다.
