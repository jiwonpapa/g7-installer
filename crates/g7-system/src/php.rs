pub const UBUNTU_FPM_VERSION: &str = "8.3";
pub const DEFAULT_FPM_VERSION: &str = "8.5";
pub const NEXT_FPM_VERSION: &str = DEFAULT_FPM_VERSION;
pub const SUPPORTED_FPM_VERSIONS: [&str; 2] = [UBUNTU_FPM_VERSION, DEFAULT_FPM_VERSION];
pub const PHP_SOURCE_AUTO: &str = "auto";
pub const PHP_SOURCE_UBUNTU: &str = "ubuntu";
pub const PHP_SOURCE_ONDREJ: &str = "ondrej";
pub const SUPPORTED_PHP_SOURCES: [&str; 3] =
    [PHP_SOURCE_AUTO, PHP_SOURCE_UBUNTU, PHP_SOURCE_ONDREJ];
