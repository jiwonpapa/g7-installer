use super::*;

pub(super) async fn api_setup_guide(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_authenticated_session(&state, &headers, peer.ip())?;
    let content = read_setup_guide(Path::new(SETUP_GUIDE_PATH))?;

    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"g7-installer-setup-guide.md\"",
            ),
        ],
        content,
    ))
}

fn read_setup_guide(path: &Path) -> std::result::Result<String, ApiError> {
    fs::read_to_string(path).map_err(|error| {
        ApiError::bad_request(format!("설정 안내서를 읽지 못했습니다: {error}"))
            .with_hint("설치 완료 후 다시 시도하세요.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_guide_reader_returns_content_and_a_clear_missing_error() {
        let root = std::env::temp_dir().join(format!("g7-setup-guide-test-{}", std::process::id()));
        fs::create_dir_all(&root).expect("temp directory should be created");
        let guide = root.join("setup-guide.md");
        fs::write(&guide, "# setup guide\n").expect("guide should be written");

        assert_eq!(
            read_setup_guide(&guide).expect("guide should load"),
            "# setup guide\n"
        );
        let error =
            read_setup_guide(&root.join("missing.md")).expect_err("missing guide should fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            error
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("설치 완료")
        );

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }
}
