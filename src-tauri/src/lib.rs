pub mod audio;
pub mod buffer;
pub mod crosshair;
pub mod crosshair_overlay;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod engine;
pub mod game_art;
pub mod games;
pub mod geometry;
pub mod health;
pub mod installed_games;
pub mod library;
pub mod media;
pub mod model;
pub mod native_gpu;
pub mod overlay;
pub mod playtime;
pub mod process_job;
pub mod status_overlay;
#[cfg(feature = "desktop")]
pub mod update_flow;
pub mod updates;
pub mod voice;
pub mod winutil;

pub fn native_engine_available() -> bool {
    cfg!(feature = "bundled-native") || native_alpha_enabled()
}

pub fn native_alpha_enabled() -> bool {
    std::env::var_os("REPLAYX_NATIVE_ALPHA").is_some_and(|v| v == "1")
}
