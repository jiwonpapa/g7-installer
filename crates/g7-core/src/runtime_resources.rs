//! Installer-owned GnuBoard7 runtime resources.
//!
//! Finalize and reset share these constants so every service or configuration
//! added after the browser installer can be removed during a reinstall reset.

pub const G7_QUEUE_SERVICE: &str = "g7-queue.service";
pub const G7_SCHEDULER_SERVICE: &str = "g7-scheduler.service";
pub const G7_SCHEDULER_TIMER: &str = "g7-scheduler.timer";
pub const G7_REVERB_SERVICE: &str = "g7-reverb.service";

pub const G7_QUEUE_SERVICE_PATH: &str = "/etc/systemd/system/g7-queue.service";
pub const G7_SCHEDULER_SERVICE_PATH: &str = "/etc/systemd/system/g7-scheduler.service";
pub const G7_SCHEDULER_TIMER_PATH: &str = "/etc/systemd/system/g7-scheduler.timer";
pub const G7_REVERB_SERVICE_PATH: &str = "/etc/systemd/system/g7-reverb.service";

pub const G7_RUNTIME_SERVICES: [&str; 4] = [
    G7_QUEUE_SERVICE,
    G7_SCHEDULER_SERVICE,
    G7_SCHEDULER_TIMER,
    G7_REVERB_SERVICE,
];

pub const G7_RUNTIME_FILES: [&str; 4] = [
    G7_QUEUE_SERVICE_PATH,
    G7_SCHEDULER_SERVICE_PATH,
    G7_SCHEDULER_TIMER_PATH,
    G7_REVERB_SERVICE_PATH,
];
