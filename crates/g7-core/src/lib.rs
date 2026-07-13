//! Core installer policy, planning, orchestration, and report generation.
//!
//! This crate is the source of truth for server mutation rules. CLI and web UI
//! code should adapt these policies instead of duplicating install defaults.

#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod app_profile;
pub mod commands;
pub mod defaults;
pub mod error;
pub mod installer_paths;
pub mod resource_policy;
pub mod runtime_resources;
mod vite_manifest;

pub use error::{Error, Result};
