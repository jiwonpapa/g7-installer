use super::*;

pub const DEFAULT_BIND: &str = "127.0.0.1:7717";

pub(super) const INDEX_HTML: &str = include_str!("../../../../web/index.html");
pub(super) const APP_JS: &str = include_str!("../../../../web/app.js");
pub(super) const EVENT_STREAM_JS: &str = include_str!("../../../../web/modules/event-stream.js");
pub(super) const APP_CSS: &str = include_str!("../../../../web/dist/app.css");
/// Light-theme intro artwork embedded in the standalone release binary.
pub(super) const INTRO_IMAGE: &[u8] =
    include_bytes!("../../../../web/assets/setup-orbit-light.webp");
/// Dark-theme intro artwork embedded in the standalone release binary.
pub(super) const INTRO_DARK_IMAGE: &[u8] =
    include_bytes!("../../../../web/assets/setup-orbit-dark.webp");
pub(super) const PROMO_JSON: &str = include_str!("../../../../web/promo.sample.json");
pub(super) const DEFAULT_PROMO_MANIFEST_URL: &str = "/promo.json";
pub(super) const ASSET_VERSION: &str = match option_env!("G7_ASSET_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
pub(super) const SESSION_COOKIE: &str = "g7inst_session";
pub(super) const CSRF_HEADER: &str = "x-g7-csrf";
pub(super) const SESSION_TTL_SECONDS: u64 = 8 * 60 * 60;
pub(super) const SESSION_TTL: Duration = Duration::from_secs(SESSION_TTL_SECONDS);
