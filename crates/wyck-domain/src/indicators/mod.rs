//! Indicators ("studies"): moving averages, bands, oscillators and the volume profile.
//!
//! - [`crate::base::ta`]: the arithmetic, series in and series out, tested against worked examples.
//! - `kind` and `spec`: what each indicator is ([`StudyKind`](kind::StudyKind) and its [`Spec`](spec::Spec): inputs, plots,
//!   where it is drawn).
//! - `config` and `style`: what the user set ([`StudyConfig`](config::StudyConfig), which is also what is saved) and how
//!   the plots, levels and fills look.
//! - `compute`: [`compute`], which turns bars into the lines to plot, and `data`, what it reads and
//!   gives back ([`StudyInput`](data::StudyInput), [`StudyOutput`](compute::StudyOutput)).
//! - [`profile`]: the volume profile of the visible bars, which is not a line per bar.
//!
//! A config names its inputs and plots by stable keys (`length`, `upper`, `signal`), never by
//! position, and every one of them has a default. A file written by another version still loads:
//! an unknown indicator is dropped, a missing input takes its default, an unknown one is ignored.

pub mod catalog;
pub mod compute;
pub mod config;
pub mod custom;
mod data;
mod extended;
pub mod higher;
pub mod kind;
pub mod profile;
pub mod spec;
pub mod style;

#[cfg(test)]
mod tests;
