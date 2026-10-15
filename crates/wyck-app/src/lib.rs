//! Application state and use cases: what the app remembers and does between the services
//! below it and the screens above it.

pub mod account;
pub mod alerts;
pub mod appearance;
pub mod broker;
pub mod chart_export;
pub mod diagnostics;
pub mod drawings;
pub mod locations;
pub mod market_data;
pub mod migrations;
pub mod prefs;
pub mod scripts;
pub mod shortcuts;
pub mod sign_in;
pub mod storage;
pub mod system;
pub mod token_store;
pub mod updates;
pub mod workspace;

/// The folders of the app, resolved once: `None` when the system gives it none. Everything that
/// needs a path of the app asks here, so there is one answer for the whole run.
///
/// The first call also makes the move of folders the user asked for (it has to be done before
/// anything reads a document), which is why the first call is the first thing `run` does.
pub fn app_paths() -> Option<&'static wyck_storage::AppPaths> {
    static PATHS: std::sync::OnceLock<Option<wyck_storage::AppPaths>> = std::sync::OnceLock::new();
    PATHS.get_or_init(locations::prepare).as_ref()
}
