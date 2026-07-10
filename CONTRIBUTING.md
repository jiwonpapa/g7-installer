# 기여 안내

## 개발 순서

1. 이슈에서 변경 목적과 파괴 범위를 먼저 합의합니다.
2. `codex/` 접두사 브랜치에서 작은 단위로 수정합니다.
3. 공개 API와 파괴 작업에는 rustdoc과 회귀 테스트를 함께 추가합니다.
4. `bash scripts/quality-gate.sh`를 통과시킨 뒤 Pull Request를 엽니다.

## 필수 기준

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo audit` 및 `cargo deny check`
- reset/rollback 변경 시 보존 리소스 golden 테스트
- 웹 UI 변경 시 Playwright E2E

실제 서버 검증은 승인된 폐기 가능 Ubuntu VPS에서
`.github/workflows/ops-harness.yml`을 수동 실행합니다. 운영 Let's Encrypt 발급은
일반 테스트에 사용하지 않습니다.
