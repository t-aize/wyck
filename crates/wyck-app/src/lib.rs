//! # wyck-app
//!
//! The application layer between [`wyck_engine`] and a front end: everything an interface needs
//! that is not drawing. It reads the settings, builds the connection, starts logging, runs the
//! use cases (connect, a hotkey order), turns the engine's state into strings and tones, and
//! translates errors into messages, and drives the connection flow. Apart from the optional `shell` and
//! `ui` modules it contains **no user interface code and no UI toolkit**: a front end, whatever it
//! is built with, draws what this crate computes.
//!
//! # Modules
//!
//! Every module but the last two is plain Rust and tested without a window.
//!
//! | Module | Role |
//! |---|---|
//! | [`settings`] | What the application is configured with |
//! | [`startup`] | From settings to a connection request (profile or environment) |
//! | [`controller`] | The use cases: start the engine, connect, send a hotkey order |
//! | [`presentation`] | Engine state to formatted rows, badges and tones |
//! | [`model`] | The data a front end's windows share (state, activity, notices, banners) |
//! | [`messages`] | Errors and outcomes to user-facing notices, in one place |
//! | [`hotkeys`] | Global shortcuts: parsing, registration, debouncing |
//! | [`logging`], [`session_marker`] | Log files, crash detection |
//! | [`flow`] | The connection flow: screens, transitions, token checks |
//! | [`dashboard`] | What the dashboard header shows: symbol, price, spread, time frames |
//! | [`symbols`] | The symbols an account offers: classes, icons, search, details sheet |
//! | `shell` (feature `gui`) | The GPUI window, engine plumbing and shortcuts |
//! | `ui` (feature `gui`) | The screens, the title bar, the theme: drawing only |
//!
//! # A front end's job
//!
//! 1. Read [`settings::AppSettings`], start [`logging`] and the [`session_marker`], and start an
//!    [`controller::AppController`].
//! 2. Build the connection with [`startup::connect_request`] and call
//!    [`controller::AppController::connect`].
//! 3. Follow `controller.handle().watch_state()`, feed each state to
//!    [`model::AppModel::apply`], and draw [`presentation`]'s output.
//! 4. Register the shortcuts ([`hotkeys`]) on the thread that pumps the window messages, and call
//!    [`controller::AppController::hotkey_order`] on a press.
//!
//! # Safety
//!
//! Orders sent from a global hotkey are dry runs: the controller refuses to send one while the
//! engine is armed. See [`controller`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod controller;
pub mod dashboard;
pub mod flow;
pub mod hotkeys;
pub mod logging;
pub mod messages;
pub mod model;
pub mod presentation;
pub mod session_marker;
pub mod settings;
#[cfg(feature = "gui")]
pub mod shell;
pub mod startup;
pub mod symbols;
#[cfg(feature = "gui")]
pub mod ui;
