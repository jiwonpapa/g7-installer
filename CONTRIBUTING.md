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
`bash scripts/local-release-gate.sh`로 품질 게이트, 커버리지, 릴리스 산출물 생성을 로컬에서
검증합니다. 이 릴리스 게이트는 기본적으로 임시 `CARGO_TARGET_DIR`를 사용하고 완료 후 빌드
캐시를 삭제합니다. 캐시를 보존해야 할 때만 `G7_RELEASE_KEEP_TARGET=1`을 사용합니다.
