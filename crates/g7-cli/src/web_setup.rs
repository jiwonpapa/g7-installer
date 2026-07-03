//! Web setup controller for `g7inst setup`.
//!
//! This module owns the human-facing setup UX. It runs a short-lived local web
//! controller, serves the bundled HTML/CSS/JS assets, and delegates all install
//! policy to `g7_core::commands::plan` and mutating actions to core commands.
//! The controller must not expose arbitrary shell execution or invent install
//! defaults that do not exist in `plan.rs`.

use std::net::{IpAddr, SocketAddr};

use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use getrandom::fill as fill_random;
use miette::{IntoDiagnostic, Result, miette};
use serde::Serialize;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

pub const DEFAULT_BIND: &str = "127.0.0.1:7717";

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_JS: &str = include_str!("../../../web/app.js");
const APP_CSS: &str = include_str!("../../../web/dist/app.css");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSetupConfig {
    pub domain: Option<String>,
    pub local_test: bool,
    pub bind: String,
    pub allow_remote: bool,
}

#[derive(Debug, Clone)]
struct WebState {
    access_token: String,
    domain: Option<String>,
    local_test: bool,
}

#[derive(Debug, Serialize)]
struct BootstrapPayload {
    domain: Option<String>,
    local_test: bool,
    auth: BootstrapAuth,
}

#[derive(Debug, Serialize)]
struct BootstrapAuth {
    mode: &'static str,
    status: &'static str,
}

pub async fn run(config: WebSetupConfig) -> Result<()> {
    let bind = parse_bind(&config.bind)?;
    ensure_remote_binding_is_explicit(bind, config.allow_remote)?;

    let state = WebState {
        access_token: secure_token()?,
        domain: config.domain,
        local_test: config.local_test,
    };

    let listener = TcpListener::bind(bind)
        .await
        .map_err(|err| miette!("failed to bind web setup controller on {bind}: {err}"))?;
    let local_addr = listener.local_addr().into_diagnostic()?;
    print_startup(local_addr, &state.access_token);

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/app.css", get(app_css))
        .route("/api/bootstrap", get(bootstrap))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| miette!("web setup controller failed: {err}"))
}

fn parse_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .map_err(|err| miette!("invalid --bind value `{bind}`: {err}"))
}

fn ensure_remote_binding_is_explicit(bind: SocketAddr, allow_remote: bool) -> Result<()> {
    if is_loopback(bind.ip()) || allow_remote {
        return Ok(());
    }

    Err(miette!(
        "--allow-remote is required when binding setup controller to {bind}"
    ))
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback(),
    }
}

fn secure_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fill_random(&mut bytes).map_err(|err| miette!("failed to generate setup token: {err}"))?;
    Ok(hex_encode(&bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

fn print_startup(addr: SocketAddr, token: &str) {
    println!("G7inst Web Controller");
    println!("Open: http://{addr}/?token={token}");
    println!("Remote access:");
    println!("ssh -L 7717:127.0.0.1:7717 root@SERVER_IP");
    println!("Stop: Ctrl+C");
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_err() {
        return;
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

async fn app_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS)
}

async fn bootstrap(
    axum::extract::State(state): axum::extract::State<WebState>,
) -> impl IntoResponse {
    let payload = BootstrapPayload {
        domain: state.domain,
        local_test: state.local_test,
        auth: BootstrapAuth {
            mode: "token-pam-planned",
            status: "bootstrap-only",
        },
    };

    (StatusCode::OK, Json(payload))
}

#[cfg(test)]
mod tests {
    use super::{ensure_remote_binding_is_explicit, parse_bind, secure_token};

    #[test]
    fn loopback_bind_is_allowed_without_remote_flag()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let bind = parse_bind("127.0.0.1:7717")?;
        ensure_remote_binding_is_explicit(bind, false)?;
        Ok(())
    }

    #[test]
    fn remote_bind_requires_explicit_flag() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let bind = parse_bind("0.0.0.0:7717")?;
        assert!(ensure_remote_binding_is_explicit(bind, false).is_err());
        ensure_remote_binding_is_explicit(bind, true)?;
        Ok(())
    }

    #[test]
    fn setup_token_is_hex_encoded_32_bytes() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let token = secure_token()?;
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|char| char.is_ascii_hexdigit()));
        Ok(())
    }
}
