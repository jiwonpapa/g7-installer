use super::*;

pub(super) fn create_session(
    state: &WebState,
    client_ip: IpAddr,
) -> std::result::Result<String, ApiError> {
    lock_client_ip(state, client_ip)?;
    let session_id = secure_token().map_err(ApiError::bad_request)?;
    let csrf_token = secure_token().map_err(ApiError::bad_request)?;
    let session = Session {
        csrf_token,
        authenticated: true,
        username: Some("root".to_string()),
        client_ip,
        expires_at: Instant::now() + SESSION_TTL,
    };

    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    sessions.insert(session_id.clone(), session);

    Ok(session_id)
}

pub(super) fn lock_client_ip(
    state: &WebState,
    client_ip: IpAddr,
) -> std::result::Result<(), ApiError> {
    let mut allowed = state
        .allowed_client_ip
        .lock()
        .map_err(|_| ApiError::bad_request("client IP lock is unavailable"))?;

    match *allowed {
        Some(allowed_ip) if allowed_ip == client_ip => Ok(()),
        Some(allowed_ip) => Err(client_ip_forbidden(allowed_ip, client_ip)),
        None => {
            *allowed = Some(client_ip);
            emit_log(state, format!("접속 IP 잠금 완료: {client_ip}"));
            Ok(())
        }
    }
}

pub(super) fn require_allowed_client_ip(
    state: &WebState,
    client_ip: IpAddr,
) -> std::result::Result<(), ApiError> {
    let allowed = state
        .allowed_client_ip
        .lock()
        .map_err(|_| ApiError::bad_request("client IP lock is unavailable"))?;

    match *allowed {
        Some(allowed_ip) if allowed_ip != client_ip => {
            Err(client_ip_forbidden(allowed_ip, client_ip))
        }
        _ => Ok(()),
    }
}

pub(super) fn client_ip_forbidden(allowed_ip: IpAddr, client_ip: IpAddr) -> ApiError {
    ApiError::forbidden("setup controller is locked to the first valid token client IP")
        .with_hint(
            "터미널의 token URL을 처음 연 같은 SSH 터널 또는 같은 클라이언트에서 접속하세요.",
        )
        .with_details(vec![
            format!("allowed_client_ip: {allowed_ip}"),
            format!("request_client_ip: {client_ip}"),
        ])
}

pub(super) fn require_session_id(headers: &HeaderMap) -> std::result::Result<String, ApiError> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing setup session cookie"))?;

    cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE).then(|| value.to_string()))
        .ok_or_else(|| ApiError::unauthorized("missing setup session cookie"))
}

pub(super) fn require_session(
    state: &WebState,
    headers: &HeaderMap,
    client_ip: IpAddr,
) -> std::result::Result<Session, ApiError> {
    require_allowed_client_ip(state, client_ip)?;
    let session_id = require_session_id(headers)?;
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;

    let now = Instant::now();
    sessions.retain(|_, session| session.expires_at > now);

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| ApiError::unauthorized("setup session expired or invalid"))?;
    if session.client_ip != client_ip {
        return Err(client_ip_forbidden(session.client_ip, client_ip));
    }
    session.expires_at = now + SESSION_TTL;

    Ok(session.clone())
}

pub(super) fn require_authenticated_session(
    state: &WebState,
    headers: &HeaderMap,
    client_ip: IpAddr,
) -> std::result::Result<Session, ApiError> {
    let session = require_session(state, headers, client_ip)?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(ApiError::unauthorized("setup token session is required"))
    }
}

pub(super) fn require_csrf(
    headers: &HeaderMap,
    session: &Session,
) -> std::result::Result<(), ApiError> {
    let token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("missing CSRF token"))?;

    if secure_eq(token, &session.csrf_token) {
        Ok(())
    } else {
        Err(ApiError::forbidden("invalid CSRF token"))
    }
}

pub(super) fn remove_session(
    state: &WebState,
    session_id: &str,
) -> std::result::Result<(), ApiError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| ApiError::bad_request("session store is unavailable"))?;
    sessions.remove(session_id);

    Ok(())
}

pub(super) fn session_cookie(session_id: &str) -> String {
    format!("{SESSION_COOKIE}={session_id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=1800")
}

pub(super) fn secure_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut diff = left.len() ^ right.len();

    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }

    diff == 0
}

impl ApiError {
    pub(super) fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
            hint: None,
            details: Vec::new(),
            retryable: true,
        }
    }

    pub(super) fn unauthorized(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: error.into(),
            hint: Some(
                "터미널에 출력된 token URL로 다시 접속하세요. 서버 비밀번호 입력은 사용하지 않습니다."
                    .to_string(),
            ),
            details: Vec::new(),
            retryable: true,
        }
    }

    pub(super) fn forbidden(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: error.into(),
            hint: Some(
                "권한 또는 접속 방식을 확인하세요. 원격 VPS는 SSH 터널 접속을 권장합니다."
                    .to_string(),
            ),
            details: Vec::new(),
            retryable: true,
        }
    }

    pub(super) fn conflict(error: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: error.into(),
            hint: Some("현재 작업이 끝난 뒤 다시 시도하세요.".to_string()),
            details: Vec::new(),
            retryable: true,
        }
    }

    pub(super) fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(super) fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
                hint: self.hint,
                details: self.details,
                retryable: self.retryable,
            }),
        )
            .into_response()
    }
}

pub(super) fn emit_log(state: &WebState, message: impl Into<String>) {
    let _ = state.events.send(WebEvent {
        event_type: "log",
        message: message.into(),
        stage: None,
        status: None,
        operation: None,
        percent: None,
    });
}

pub(super) fn emit_stage(
    state: &WebState,
    stage: &'static str,
    status: &'static str,
    message: impl Into<String>,
) {
    let _ = state.events.send(WebEvent {
        event_type: "stage",
        message: message.into(),
        stage: Some(stage),
        status: Some(status),
        operation: None,
        percent: None,
    });
}

pub(super) fn emit_progress(
    state: &WebState,
    operation: &'static str,
    percent: u8,
    message: impl Into<String>,
) {
    let _ = state.events.send(WebEvent {
        event_type: "progress",
        message: message.into(),
        stage: None,
        status: None,
        operation: Some(operation),
        percent: Some(percent.min(100)),
    });
}
