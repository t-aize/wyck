//! A dedicated tokio runtime for a GPUI app.
//!
//! GPUI has its own async executor (backed by `smol`), not tokio: `cx.spawn` and
//! `cx.background_spawn` run futures on it. But [`wyck_openapi`] and its OAuth flow are built on
//! tokio directly (`tokio-tungstenite`, `reqwest`, `tokio::net` for the local redirect listener),
//! and those types panic without a live tokio runtime underneath them.
//!
//! The fix is the standard one for embedding a tokio-based library in a non-tokio async UI: run a
//! real multi-thread tokio [`Runtime`] on its own OS thread, forever, and hand out its [`Handle`].
//! A screen calls [`spawn`] to run tokio-dependent work on that runtime; the returned
//! [`JoinHandle`] is a plain [`Future`] that any executor can poll, gpui's
//! included, so `cx.spawn(async move |cx| { let result = runtime::spawn(...).await; ... })` just
//! works without any manual channel plumbing.

use std::sync::OnceLock;

use tokio::runtime::{Handle, Runtime};
use tokio::task::JoinHandle;

static RUNTIME: OnceLock<Handle> = OnceLock::new();

/// The shared tokio [`Handle`], starting the background runtime thread on first use.
pub fn handle() -> Handle {
    RUNTIME
        .get_or_init(|| {
            let runtime = Runtime::new().expect("failed to start the background tokio runtime");
            let handle = runtime.handle().clone();
            // Leaking the `Runtime` keeps its worker threads (and their reactor/timer) alive for
            // the rest of the process; there is exactly one of these for the app's lifetime, and
            // it must outlive every `JoinHandle` ever produced by `spawn` below.
            std::mem::forget(runtime);
            handle
        })
        .clone()
}

/// Runs `future` on the background tokio runtime, returning as soon as `future` does.
///
/// Await the result from a gpui task: `runtime::spawn(fut).await` inside `cx.spawn`.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle().spawn(future)
}
