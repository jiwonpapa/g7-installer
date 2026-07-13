//! Web setup controller for `g7inst setup`.
//!
//! This module owns the human-facing setup UX. It runs a short-lived local web
//! controller, serves the bundled HTML/CSS/JS assets, and delegates all install
//! policy to `g7_core::commands::plan` and mutating actions to core commands.
//! The controller must not expose arbitrary shell execution or invent install
//! defaults that do not exist in `plan.rs`.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use g7_core::commands::{DoctorCheckStatus, doctor, install, plan, reset, rollback, status};
use g7_core::installer_paths::{
    CONFIG_PATH, LOCAL_HOSTS_PATH, REPORT_PATH, ROLLBACK_PATH, SETUP_GUIDE_PATH,
};
use g7_state::owned_files::OWNED_FILES_PATH;
use g7_state::state::{STATE_PATH, read_state_file};
use g7_system::SystemProbe;
use g7_system::command::{CommandEvent, CommandObserver, RealCommandRunner};
use getrandom::fill as fill_random;
use miette::{IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;

mod api;
mod artifacts;
mod defaults;
mod provision_actions;
mod recovery;
mod render;
mod routes;
mod session;

pub use api::WebSetupConfig;
pub use defaults::DEFAULT_BIND;
pub use routes::run;

use api::*;
use artifacts::*;
use defaults::*;
use provision_actions::*;
use recovery::*;
use render::*;
use routes::*;
use session::*;

#[cfg(test)]
mod tests;
