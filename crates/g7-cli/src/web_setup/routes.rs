use super::*;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub async fn run(config: WebSetupConfig) -> Result<()> {
    ensure_setup_runs_as_root_or_reexec()?;

    let bind = parse_bind(&config.bind)?;
    ensure_remote_binding_is_explicit(bind, config.allow_remote)?;

    let state = WebState {
        access_token: secure_token()?,
        domain: config.domain,
        local_test: config.local_test,
        events: broadcast::channel(128).0,
        event_history: Arc::new(Mutex::new(VecDeque::with_capacity(512))),
        event_sequence: Arc::new(AtomicU64::new(0)),
        install_running: Arc::new(AtomicBool::new(false)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        allowed_client_ip: Arc::new(Mutex::new(None)),
    };

    let listener = TcpListener::bind(bind)
        .await
        .map_err(|err| miette!("failed to bind web setup controller on {bind}: {err}"))?;
    let local_addr = listener.local_addr().into_diagnostic()?;
    print_startup(local_addr, &state.access_token);

    let app = build_router(state);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|err| miette!("web setup controller failed: {err}"))
}

pub(super) fn build_router(state: WebState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/setup/connect", get(index))
        .route("/setup/doctor", get(index))
        .route("/setup/options", get(index))
        .route("/setup/plan", get(index))
        .route("/setup/install", get(index))
        .route("/setup/result", get(index))
        .route("/setup/guide", get(index))
        .route("/setup/provision", get(index))
        .route("/app.js", get(app_js))
        .route("/modules/event-stream.js", get(event_stream_js))
        .route("/app.css", get(app_css))
        .route("/assets/setup-orbit-light.webp", get(intro_image))
        .route("/promo.json", get(promo_json))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/events", get(api_events))
        .route("/api/doctor", get(api_doctor))
        .route("/api/plan", post(api_plan))
        .route("/api/install/prepare", post(api_install_prepare))
        .route("/api/install/resume", post(api_resume))
        .route("/api/provision/action", post(api_provision_action))
        .route("/api/reset", post(api_reset))
        .route("/api/rollback", post(api_rollback))
        .route("/api/status", get(api_status))
        .route("/api/recovery", get(api_recovery))
        .route("/api/report", get(api_report))
        .route("/api/artifacts/setup-guide", get(api_setup_guide))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub(super) fn parse_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .map_err(|err| miette!("invalid --bind value `{bind}`: {err}"))
}

pub(super) fn ensure_remote_binding_is_explicit(
    bind: SocketAddr,
    allow_remote: bool,
) -> Result<()> {
    if is_loopback(bind.ip()) || allow_remote {
        return Ok(());
    }

    Err(miette!(
        "--allow-remote is required when binding setup controller to {bind}"
    ))
}

pub(super) fn ensure_setup_runs_as_root_or_reexec() -> Result<()> {
    if current_user_is_root()? {
        return Ok(());
    }

    if std::env::var_os("G7INST_DISABLE_SUDO_REEXEC").is_some() {
        return Err(setup_requires_root_error());
    }

    reexec_setup_with_sudo()
}

pub(super) fn current_user_is_root() -> Result<bool> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .into_diagnostic()
        .map_err(|err| miette!("failed to check current user id: {err}"))?;

    if !output.status.success() {
        return Err(miette!(
            "failed to check current user id: id -u exited with status {}",
            output.status
        ));
    }

    let uid = String::from_utf8_lossy(&output.stdout);
    Ok(uid.trim() == "0")
}

pub(super) fn setup_requires_root_error() -> miette::Report {
    miette!(
        "g7inst setup must be started with sudo/root.\nPreferred: sudo -v && sudo g7inst setup\nEnter the domain once in the web wizard.\nIf sudo asks for a password, type it in the SSH terminal before the web UI opens.\nIf sudo is not available, log in as root or run su - from the provider console, then run g7inst setup.\nServer account password input is not used in the web UI."
    )
}

#[cfg(unix)]
pub(super) fn reexec_setup_with_sudo() -> Result<()> {
    let sudo_available = Command::new("sudo")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match sudo_available {
        Ok(status) if status.success() => {}
        Ok(status) => {
            return Err(miette!(
                "sudo is installed but not usable: sudo -V exited with status {status}.\nLog in as root or run su - from the provider console, then run g7inst setup.\nServer account password input is not used in the web UI."
            ));
        }
        Err(err) => {
            return Err(miette!(
                "sudo is not available: {err}.\nLog in as root or run su - from the provider console, then run g7inst setup.\nServer account password input is not used in the web UI."
            ));
        }
    }

    let exe = std::env::current_exe().into_diagnostic()?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();

    eprintln!("g7inst setup requires sudo/root.");
    eprintln!(
        "Re-running with sudo now. If prompted, enter the server account password in this SSH terminal."
    );
    eprintln!("The web UI will not ask for the root/ubuntu password.");

    let err = Command::new("sudo").arg("-E").arg(exe).args(args).exec();

    Err(miette!("failed to re-run g7inst setup with sudo: {err}"))
}

#[cfg(not(unix))]
pub(super) fn reexec_setup_with_sudo() -> Result<()> {
    Err(setup_requires_root_error())
}

pub(super) fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

pub(super) fn secure_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fill_random(&mut bytes).map_err(|err| miette!("failed to generate setup token: {err}"))?;
    Ok(hex_encode(&bytes))
}

pub(super) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

pub(super) fn print_startup(addr: SocketAddr, token: &str) {
    let browser_addr = browser_addr_for(addr);
    let port = addr.port();

    println!("G7inst Web Controller");
    println!("Open: http://{browser_addr}/?token={token}");
    println!("Access lock: first valid token client IP only");
    println!("Remote access:");
    println!("ssh -L {port}:127.0.0.1:{port} ubuntu@SERVER_IP");
    if !is_loopback(addr.ip()) {
        println!("Remote bind is enabled; keep this port firewalled.");
    }
    println!(
        "Server password: handled in SSH/root shell before this controller starts; not used in the web UI."
    );
    println!("Stop: Ctrl+C");
}

pub(super) fn browser_addr_for(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    }
}

pub(super) async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

pub(super) async fn index(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut response = Html(index_html()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, max-age=0"),
    );
    let client_ip = peer.ip();

    if query
        .get("token")
        .is_some_and(|token| secure_eq(token, &state.access_token))
    {
        match create_session(&state, client_ip) {
            Ok(session_id) => {
                if let Ok(value) = HeaderValue::from_str(&session_cookie(&session_id)) {
                    response.headers_mut().insert(header::SET_COOKIE, value);
                }
            }
            Err(error) => return error.into_response(),
        }
    } else if let Err(error) = require_allowed_client_ip(&state, client_ip) {
        return error.into_response();
    }

    response
}

pub(super) fn index_html() -> String {
    INDEX_HTML
        .replace("__G7INST_ASSET_VERSION__", ASSET_VERSION)
        .replace(
            "__G7INST_PROMO_MANIFEST_URL__",
            &html_attr_escape(&promo_manifest_url()),
        )
}

pub(super) fn promo_manifest_url() -> String {
    std::env::var("G7_PROMO_MANIFEST_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROMO_MANIFEST_URL.to_string())
}

pub(super) fn html_attr_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) async fn app_js(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        APP_JS,
    ))
}

pub(super) async fn event_stream_js(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        EVENT_STREAM_JS,
    ))
}

pub(super) async fn app_css(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        APP_CSS,
    ))
}

pub(super) async fn intro_image(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        INTRO_IMAGE,
    ))
}

pub(super) async fn promo_json(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> std::result::Result<impl IntoResponse, ApiError> {
    require_allowed_client_ip(&state, peer.ip())?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store, no-cache, max-age=0"),
        ],
        PROMO_JSON,
    ))
}

pub(super) async fn bootstrap(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers, peer.ip())?;
    let payload = BootstrapPayload {
        domain: state.domain,
        local_test: state.local_test,
        auth: BootstrapAuth {
            mode: "sudo-token",
            status: if session.authenticated {
                "authenticated"
            } else {
                "token-accepted"
            },
            username: session.username.clone(),
            authenticated: session.authenticated,
            client_ip: session.client_ip.to_string(),
        },
        csrf_token: session.csrf_token,
    };

    Ok((StatusCode::OK, Json(payload)))
}

pub(super) async fn api_logout(
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> std::result::Result<impl IntoResponse, ApiError> {
    let session = require_session(&state, &headers, peer.ip())?;
    require_csrf(&headers, &session)?;
    let session_id = require_session_id(&headers)?;
    remove_session(&state, &session_id)?;

    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn api_events(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<WebState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Err(error) = require_session(&state, &headers, peer.ip()) {
        return error.into_response();
    }

    ws.on_upgrade(move |socket| event_socket(socket, state))
        .into_response()
}

pub(super) async fn event_socket(mut socket: WebSocket, state: WebState) {
    let mut events = state.events.subscribe();
    let history = event_history_snapshot(&state, 0);
    let mut last_seq = 0;
    for event in history {
        last_seq = last_seq.max(event.seq);
        if send_event(&mut socket, &event).await.is_err() {
            return;
        }
    }

    let connected = emit_log(&state, "실시간 로그 연결됨");
    last_seq = last_seq.max(connected.seq);
    if send_event(&mut socket, &connected).await.is_err() {
        return;
    }

    loop {
        match events.recv().await {
            Ok(event) if event.seq > last_seq => {
                last_seq = event.seq;
                if send_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => {
                for event in event_history_snapshot(&state, last_seq) {
                    last_seq = event.seq;
                    if send_event(&mut socket, &event).await.is_err() {
                        return;
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

pub(super) async fn send_event(
    socket: &mut WebSocket,
    event: &WebEvent,
) -> std::result::Result<(), ()> {
    let Ok(text) = serde_json::to_string(event) else {
        return Err(());
    };

    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
