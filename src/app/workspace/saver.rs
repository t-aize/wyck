//! Writing a document a moment after the last change, and once more when the app closes.
//!
//! A change of layout or a dragged drawing can change something a dozen times a second, and the
//! disk should not be written each time. [`Saver::schedule`] keeps only the latest value and
//! writes it after a short quiet time; if the app is closed before that, [`Saver::flush`] (also
//! run when the saver is dropped) writes it at once, so a change is never lost to the delay.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use wyck::config::DocumentStore;

use crate::app::runtime;

/// How long a value waits for a newer one before it is written.
const QUIET: Duration = Duration::from_millis(700);

struct Inner<T> {
    store: DocumentStore,
    name: &'static str,
    pending: Mutex<Option<T>>,
    /// Counts the changes, so a delayed write can tell it was overtaken.
    generation: AtomicU64,
}

impl<T: Serialize> Inner<T> {
    fn flush(&self) {
        let value = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(value) = value
            && let Err(error) = self.store.save(self.name, &value)
        {
            tracing::warn!(%error, document = self.name, "could not save the document");
        }
    }
}

pub struct Saver<T: Serialize + Send + 'static> {
    inner: Arc<Inner<T>>,
    quiet: Duration,
}

impl<T: Serialize + Send + 'static> Saver<T> {
    pub fn new(store: DocumentStore, name: &'static str) -> Self {
        Self::with_quiet(store, name, QUIET)
    }

    fn with_quiet(store: DocumentStore, name: &'static str, quiet: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                name,
                pending: Mutex::new(None),
                generation: AtomicU64::new(0),
            }),
            quiet,
        }
    }

    /// Saves `value` soon, replacing any value still waiting.
    pub fn schedule(&self, value: T) {
        *self
            .inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(value);
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (inner, quiet) = (self.inner.clone(), self.quiet);
        runtime::spawn(async move {
            tokio::time::sleep(quiet).await;
            if inner.generation.load(Ordering::SeqCst) == generation {
                let _ = tokio::task::spawn_blocking(move || inner.flush()).await;
            }
        });
    }

    /// Writes the waiting value now, if there is one.
    pub fn flush(&self) {
        self.inner.flush();
    }
}

impl<T: Serialize + Send + 'static> Drop for Saver<T> {
    fn drop(&mut self) {
        self.inner.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use wyck::config::AppPaths;

    #[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
    struct Doc {
        n: i64,
    }

    fn store() -> (tempfile::TempDir, DocumentStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = DocumentStore::global(&AppPaths::at(dir.path()));
        (dir, store)
    }

    #[test]
    fn only_the_latest_of_a_burst_is_written_and_only_after_the_quiet_time() {
        let (_dir, store) = store();
        let saver = Saver::with_quiet(store.clone(), "doc", Duration::from_millis(80));
        for n in 1..=5 {
            saver.schedule(Doc { n });
        }
        assert!(
            store.load::<Doc>("doc").unwrap().is_none(),
            "nothing is written during the quiet time"
        );
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(store.load::<Doc>("doc").unwrap(), Some(Doc { n: 5 }));
    }

    #[test]
    fn a_waiting_value_is_written_when_the_saver_goes_away() {
        let (_dir, store) = store();
        {
            let saver = Saver::with_quiet(store.clone(), "doc", Duration::from_secs(60));
            saver.schedule(Doc { n: 9 });
        }
        assert_eq!(store.load::<Doc>("doc").unwrap(), Some(Doc { n: 9 }));
    }

    #[test]
    fn flushing_twice_writes_once_and_flushing_nothing_is_harmless() {
        let (_dir, store) = store();
        let saver = Saver::with_quiet(store.clone(), "doc", Duration::from_secs(60));
        saver.flush();
        assert!(store.load::<Doc>("doc").unwrap().is_none());
        saver.schedule(Doc { n: 1 });
        saver.flush();
        std::fs::remove_file(store.path("doc")).unwrap();
        saver.flush();
        assert!(
            store.load::<Doc>("doc").unwrap().is_none(),
            "the value was written once and not again"
        );
    }
}
