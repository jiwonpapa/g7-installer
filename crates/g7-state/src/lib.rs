//! Installer lock, state, and owned-file tracking primitives.

#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod atomic;
pub mod lock;
pub mod owned_files;
pub mod state;
