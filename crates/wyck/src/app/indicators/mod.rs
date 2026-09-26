//! The indicators the user writes: where they are kept, keeping the list of them current, and
//! what the rest of the app asks of them.
//!
//! The scripts are files in a folder (by default `indicators` in the settings folder, and the user
//! can point it elsewhere in the settings). The app reads that folder when it starts and again
//! every moment, so a file edited in another program shows up on its own. What was read is
//! published to the registry that charts look their indicators up in
//! ([`crate::app::chart::study::custom::library::registry`]), and every chart is told to look
//! again.
//!
//! Everything that touches a file goes through here and runs off the interface thread, so a
//! slow disk or a script that takes a while to compile never freezes a window.

pub mod editor;
pub mod prefs;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gpui::{App, BorrowAppContext as _, Global, Task};
use wyck_config::{AppPaths, DocumentStore};

use crate::app::chart::study::custom::library::{Changes, Library, LibraryError};
use crate::app::chart::study::custom::{Limits, library};
use prefs::{DOCUMENT, Prefs};

/// How often the folder is read again.
const POLL: Duration = Duration::from_millis(1500);

/// How many renames are kept for the charts to catch up with.
const KEPT_RENAMES: usize = 16;

/// The state of the indicators, shared by the whole app.
struct Service {
    library: Arc<Mutex<Library>>,
    /// The folder being read, kept here so asking for it never waits for a read in progress.
    dir: PathBuf,
    /// The folder used when the user chose none.
    default_dir: PathBuf,
    prefs: Prefs,
    store: Option<DocumentStore>,
    /// Grows whenever what the interface shows about the indicators changes.
    revision: u64,
    /// The indicators that were renamed, so a chart that holds one follows it.
    renames: Vec<(String, String)>,
    _poll: Option<Task<()>>,
}

impl Global for Service {}

/// Starts the service: reads the settings and the folder, and keeps reading it.
///
/// `paths` is `None` when the settings folder cannot be found: the indicators then live in a
/// temporary folder and nothing is remembered.
pub fn init(paths: Option<&AppPaths>, cx: &mut App) {
    let (default_dir, store) = match paths {
        Some(paths) => (paths.indicators_dir(), Some(DocumentStore::global(paths))),
        None => (std::env::temp_dir().join("wyck-indicators"), None),
    };
    let mut prefs = store
        .as_ref()
        .map(|s| s.load_or_default::<Prefs>(DOCUMENT))
        .unwrap_or_default()
        .normalized();
    let dir = prefs.folder_path().unwrap_or_else(|| default_dir.clone());
    let mut library = Library::new(&dir);
    if library.ensure_dir().is_ok() && !prefs.examples_removed {
        match remove_examples_folder(library.dir()) {
            Ok(()) => {
                prefs.examples_removed = true;
                if let Some(store) = &store {
                    let _ = store.save(DOCUMENT, &prefs);
                }
            }
            Err(error) => tracing::warn!(%error, "could not remove the old Examples folder"),
        }
    }
    library.refresh();
    library.publish();
    cx.set_global(Service {
        dir: library.dir().to_path_buf(),
        library: Arc::new(Mutex::new(library)),
        default_dir,
        prefs,
        store,
        revision: 0,
        renames: Vec::new(),
        _poll: None,
    });
    start_polling(cx);
}

/// Remove only the Examples folder directly inside the active indicators directory.
fn remove_examples_folder(dir: &Path) -> std::io::Result<()> {
    let target = dir.join("Examples");
    let metadata = match std::fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::other("Examples is not a regular directory"));
    }
    let base = dir.canonicalize()?;
    let resolved = target.canonicalize()?;
    if resolved.parent() != Some(base.as_path()) {
        return Err(std::io::Error::other(
            "Examples resolves outside the indicators directory",
        ));
    }
    std::fs::remove_dir_all(target)
}

fn start_polling(cx: &mut App) {
    let task = cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL).await;
            let reload = cx.update(|cx| {
                cx.try_global::<Service>()
                    .map(|s| (s.prefs.auto_reload, s.library.clone()))
            });
            let Some((true, library)) = reload else {
                continue;
            };
            let scanned = cx
                .background_executor()
                .spawn(async move {
                    let mut library = library.lock().unwrap_or_else(PoisonError::into_inner);
                    let changes = library.refresh();
                    (!changes.is_empty()).then(|| library.entries())
                })
                .await;
            if let Some(entries) = scanned {
                cx.update(|cx| publish(entries, cx));
            }
        }
    });
    if cx.has_global::<Service>() {
        cx.global_mut::<Service>()._poll = Some(task);
    }
}

/// Publishes what a read found, and tells the app when it changed.
fn publish(entries: Vec<Arc<library::Entry>>, cx: &mut App) {
    if !library::registry::install(entries) {
        return;
    }
    changed(cx);
}

/// Something about the indicators changed: everything that shows them looks again.
fn changed(cx: &mut App) {
    if cx.has_global::<Service>() {
        cx.update_global::<Service, _>(|service, _| {
            service.revision += 1;
            // A rename of a name that is taken again (a new file with the old name) is over.
            service
                .renames
                .retain(|(old, _)| !library::registry::contains(old));
        });
    }
}

// ---- what the interface reads ----

/// The settings of the indicators.
pub fn prefs(cx: &App) -> Prefs {
    cx.try_global::<Service>()
        .map(|s| s.prefs.clone())
        .unwrap_or_default()
}

/// The folder the indicators are read from.
pub fn dir(cx: &App) -> PathBuf {
    cx.try_global::<Service>()
        .map_or_else(PathBuf::new, |s| s.dir.clone())
}

/// The folder used when the user chose none.
pub fn default_dir(cx: &App) -> PathBuf {
    cx.try_global::<Service>()
        .map_or_else(PathBuf::new, |s| s.default_dir.clone())
}

/// What a script may do, from the settings.
pub fn limits(cx: &App) -> Limits {
    cx.try_global::<Service>()
        .map_or_else(Limits::default, |s| s.prefs.budget.limits())
}

/// The renames the charts must catch up with: the old id and the new one.
pub fn renames(cx: &App) -> Vec<(String, String)> {
    cx.try_global::<Service>()
        .map(|s| s.renames.clone())
        .unwrap_or_default()
}

/// Changes the settings and saves them.
pub fn update_prefs(cx: &mut App, change: impl FnOnce(&mut Prefs)) {
    if !cx.has_global::<Service>() {
        return;
    }
    let mut folder_changed = None;
    cx.update_global::<Service, _>(|service, _| {
        let mut next = service.prefs.clone();
        change(&mut next);
        let next = next.normalized();
        if next == service.prefs {
            return;
        }
        if next.folder != service.prefs.folder {
            let dir = next
                .folder_path()
                .unwrap_or_else(|| service.default_dir.clone());
            service.dir.clone_from(&dir);
            folder_changed = Some(dir);
        }
        service.prefs = next;
        if let Some(store) = &service.store
            && let Err(error) = store.save(DOCUMENT, &service.prefs)
        {
            tracing::warn!(%error, "could not save the indicator settings");
        }
        service.revision += 1;
    });
    if let Some(dir) = folder_changed {
        point_at(dir, cx);
    }
}

/// [`observe`], for a view that needs the window to answer.
pub fn observe_in<T: 'static>(
    window: &mut gpui::Window,
    cx: &mut gpui::Context<T>,
    on_change: impl FnMut(&mut T, &mut gpui::Window, &mut gpui::Context<T>) + 'static,
) -> gpui::Subscription {
    cx.observe_global_in::<Service>(window, on_change)
}

/// Calls `on_change` whenever the indicators or their settings change.
pub fn observe<T: 'static>(
    cx: &mut gpui::Context<T>,
    on_change: impl FnMut(&mut T, &mut gpui::Context<T>) + 'static,
) -> gpui::Subscription {
    cx.observe_global::<Service>(on_change)
}

/// Reads another folder from now on.
fn point_at(dir: PathBuf, cx: &mut App) {
    let Some(library) = cx.try_global::<Service>().map(|s| s.library.clone()) else {
        return;
    };
    cx.spawn(async move |cx| {
        let entries = cx
            .background_executor()
            .spawn(async move {
                let mut library = library.lock().unwrap_or_else(PoisonError::into_inner);
                library.set_dir(dir);
                let _ = library.ensure_dir();
                library.refresh();
                library.entries()
            })
            .await;
        cx.update(|cx| {
            // The registry follows even when the new folder is empty: the list is now that folder's.
            library::registry::install(entries);
            changed(cx);
        });
    })
    .detach();
}

// ---- what the interface asks for ----

/// Runs `job` on the library off the interface thread, then publishes what it left and tells the
/// app. The answer is what `job` returned.
pub fn on_library<T: Send + 'static>(
    cx: &mut App,
    job: impl FnOnce(&mut Library) -> T + Send + 'static,
) -> Task<Option<T>> {
    let Some(library) = cx.try_global::<Service>().map(|s| s.library.clone()) else {
        return Task::ready(None);
    };
    cx.spawn(async move |cx| {
        let (answer, entries) = cx
            .background_executor()
            .spawn(async move {
                let mut library = library.lock().unwrap_or_else(PoisonError::into_inner);
                let answer = job(&mut library);
                (answer, library.entries())
            })
            .await;
        cx.update(|cx| publish(entries, cx));
        Some(answer)
    })
}

/// Storage for the editor's open tabs and unsaved buffers.
pub fn draft_store(cx: &App) -> Option<DocumentStore> {
    cx.try_global::<Service>()
        .and_then(|service| service.store.clone())
}

/// Reads the folder now, instead of at the next moment.
pub fn reload(cx: &mut App) -> Task<Option<Changes>> {
    on_library(cx, Library::refresh)
}

/// Renames an indicator (its file), and has every chart that holds it follow. Gives the new id.
pub fn rename(
    cx: &mut App,
    id: String,
    new_name: String,
) -> Task<Option<Result<String, LibraryError>>> {
    let old = id.clone();
    let done = on_library(cx, move |library| library.rename(&id, &new_name));
    cx.spawn(async move |cx| {
        let result = done.await;
        if let Some(Ok(new_id)) = &result
            && *new_id != old
        {
            let (old, new_id) = (old.clone(), new_id.clone());
            cx.update(|cx| {
                if cx.has_global::<Service>() {
                    cx.update_global::<Service, _>(|service, _| {
                        service.renames.push((old, new_id));
                        let extra = service.renames.len().saturating_sub(KEPT_RENAMES);
                        service.renames.drain(..extra);
                        service.revision += 1;
                    });
                }
            });
        }
        result
    })
}

/// Opens the indicators folder in the file explorer of the system.
pub fn open_folder(cx: &mut App) {
    let dir = dir(cx);
    let _ = std::fs::create_dir_all(&dir);
    cx.reveal_path(&dir);
}

/// Shows `file` in the file explorer, selected in its folder.
pub fn reveal(cx: &mut App, file: &Path) {
    cx.reveal_path(file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_old_examples_folder_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let examples = root.path().join("Examples");
        let smc = root.path().join("SMC");
        std::fs::create_dir(&examples).unwrap();
        std::fs::create_dir(&smc).unwrap();
        std::fs::write(examples.join("changed.rhai"), "edited").unwrap();
        std::fs::write(smc.join("mine.rhai"), "kept").unwrap();
        remove_examples_folder(root.path()).unwrap();
        remove_examples_folder(root.path()).unwrap();
        assert!(!examples.exists());
        assert_eq!(
            std::fs::read_to_string(smc.join("mine.rhai")).unwrap(),
            "kept"
        );
    }
}
