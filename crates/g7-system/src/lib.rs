//! System adapters for apt, web servers, PHP, DB, network, and service checks.

#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod account;
pub mod apache;
pub mod app;
pub mod apt;
pub mod archive;
pub mod certbot;
pub mod command;
pub mod database;
pub mod mail;
pub mod network;
pub mod nginx;
pub mod os;
pub mod package;
pub mod php;
pub mod port;
pub mod privilege;
pub mod probe;
pub mod service;
pub mod systemd;

pub use probe::{FilesystemInfo, MemoryInfo, SystemProbe, SystemProbeError};
