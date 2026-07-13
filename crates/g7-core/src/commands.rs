//! Core command modules.
//!
//! Each command module must document its safety boundary at the top of the
//! file. `plan` defines intended server state, while mutating commands must
//! only perform tracked changes that can be reported and reset safely.

pub mod doctor;
pub mod finalize;
pub mod install;
pub mod logs;
pub mod plan;
pub mod reset;
pub mod rollback;
pub mod self_update;
pub mod status;
pub mod update;

pub use doctor::DoctorCheckStatus;
