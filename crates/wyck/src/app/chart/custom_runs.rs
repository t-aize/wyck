//! The indicators written as scripts, on one chart: running each of them off the interface thread
//! and keeping what it drew until the next run gives something newer.
//!
//! A script can take a while, and a live chart asks for a new run at every price. So a run never
//! blocks the window: it goes to a background thread, and while it is going the chart keeps
//! drawing the last result. Runs are not stacked up: a change that comes while one is going is
//! remembered, and the newest data is used when it ends.
//!
//! What a run gave is kept per indicator of the chart (a slot), matched to the indicators by the
//! script they hold, so adding or removing one does not throw the others away. What is kept is
//! thrown away when the bars are no longer the ones it was made from (a new symbol, older history
//! put in front).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Context, Task};

use super::Chart;
use super::display::study_input;
use super::study::custom::library::registry;
use super::study::custom::{self, Problem, Severity};
use super::study::{StudyConfig, StudyOutput};
use crate::app::indicators;

/// What a run was made from, to know whether another is wanted.
#[derive(Debug, Clone, PartialEq)]
struct Fingerprint {
    /// Which version of the script.
    stamp: u64,
    inputs: BTreeMap<String, f64>,
    /// Which state of the data and the settings.
    wanted: u64,
}

/// A run in flight. Dropping it stops the script.
struct Running {
    cancel: Arc<AtomicBool>,
    _task: Task<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct Slot {
    /// Tells one slot from another when indicators are added, removed or moved, so a run that ends
    /// finds its own.
    serial: u64,
    /// The script this slot is for.
    script: String,
    output: Option<StudyOutput>,
    problems: Vec<Problem>,
    log: Vec<String>,
    elapsed: Option<Duration>,
    computed: Option<Fingerprint>,
    running: Option<Running>,
    /// A change came while a run was going.
    dirty: bool,
}

impl Slot {
    fn new(serial: u64, script: &str) -> Self {
        Self {
            serial,
            script: script.to_owned(),
            output: None,
            problems: Vec::new(),
            log: Vec::new(),
            elapsed: None,
            computed: None,
            running: None,
            dirty: false,
        }
    }
}

/// What the legend and the editor's console can say about an indicator that is a script.
pub struct Status<'a> {
    pub running: bool,
    pub problems: &'a [Problem],
    pub log: &'a [String],
    pub elapsed: Option<Duration>,
}

impl Status<'_> {
    pub fn first_error(&self) -> Option<&Problem> {
        self.problems.iter().find(|p| p.severity == Severity::Error)
    }
}

#[derive(Default)]
pub(super) struct Custom {
    slots: Vec<Slot>,
    /// The time of the first bar the outputs were made from.
    first_time: Option<i64>,
    /// Grows whenever the data or the settings want a new run.
    wanted: u64,
    /// Whether runs must be looked at (at the next frame, which has a context to spawn from).
    pending: bool,
    /// The serial the next slot gets.
    next_serial: u64,
}

impl Custom {
    /// What the last run of the indicator at `index` said.
    pub(super) fn status(&self, index: usize) -> Option<Status<'_>> {
        self.slots.get(index).map(|slot| Status {
            running: slot.running.is_some(),
            problems: &slot.problems,
            log: &slot.log,
            elapsed: slot.elapsed,
        })
    }

    /// The bars or the settings changed: a new run is wanted. `first_time` is the time of the
    /// first bar now; when it is not the one the outputs were made from, they no longer line up
    /// with the bars and are forgotten.
    pub(super) fn data_changed(&mut self, first_time: Option<i64>) {
        if self.first_time != first_time {
            self.first_time = first_time;
            for slot in &mut self.slots {
                slot.output = None;
                slot.computed = None;
            }
        }
        self.wanted += 1;
        self.pending = true;
    }

    /// The scripts changed: every indicator is looked at again. One whose script (by version) and
    /// inputs are what it was run with is not run again.
    pub(super) fn library_changed(&mut self) {
        self.pending = true;
    }

    /// Makes the slots match the indicators of the chart, keeping what was run for the ones that
    /// are still there.
    fn align(&mut self, studies: &[StudyConfig]) {
        let mut old: Vec<Option<Slot>> = std::mem::take(&mut self.slots)
            .into_iter()
            .map(Some)
            .collect();
        for (index, study) in studies.iter().enumerate() {
            let id = study.script.as_deref().unwrap_or("");
            // The slot at the same place when it is for the same script, else any slot for it.
            let same_place = old
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|s| s.script == id);
            let found = if same_place {
                Some(index)
            } else {
                old.iter()
                    .position(|s| s.as_ref().is_some_and(|s| s.script == id))
            };
            let slot = found
                .and_then(|at| old[at].take())
                .filter(|_| study.is_script())
                .unwrap_or_else(|| {
                    self.next_serial += 1;
                    Slot::new(self.next_serial, id)
                });
            self.slots.push(slot);
        }
    }

    /// The outputs run so far, put where the chart draws them.
    pub(super) fn apply(&self, studies: &mut [Option<StudyOutput>]) {
        for (slot, target) in self.slots.iter().zip(studies.iter_mut()) {
            if slot.output.is_some() {
                target.clone_from(&slot.output);
            }
        }
    }
}

/// What the last run of a script said, for the editor.
#[derive(Debug, Clone, Default)]
pub struct ScriptReport {
    pub running: bool,
    pub problems: Vec<Problem>,
    pub log: Vec<String>,
    pub elapsed: Option<Duration>,
    /// How many bars it was run over.
    pub bars: usize,
}

impl Chart {
    /// Starts the runs of the indicators that are scripts and need one. Called at every frame,
    /// and does nothing unless something asked for it.
    pub(super) fn schedule_customs(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.custom.pending) {
            return;
        }
        self.custom.align(&self.settings.studies);
        if self.shown().is_empty() {
            return;
        }
        let limits = indicators::limits(cx);
        let mut input = None;
        for index in 0..self.settings.studies.len() {
            let config = self.settings.studies[index].clone();
            if !config.is_script() {
                continue;
            }
            let id = config.script.clone().unwrap_or_default();
            let slot = &mut self.custom.slots[index];
            let serial = slot.serial;
            if !config.visible {
                continue;
            }
            let Some(entry) = registry::get(&id) else {
                slot.output = None;
                slot.computed = None;
                slot.problems = vec![Problem::error(
                    0,
                    0,
                    format!("the indicator \"{id}\" is not in the indicators folder"),
                )];
                continue;
            };
            let Some(script) = entry.script.clone() else {
                slot.output = None;
                slot.computed = None;
                slot.problems = entry.problems.clone();
                continue;
            };
            let print = Fingerprint {
                stamp: entry.stamp,
                inputs: config.inputs.clone(),
                wanted: self.custom.wanted,
            };
            if slot.running.is_some() {
                slot.dirty = true;
                continue;
            }
            if slot.computed.as_ref() == Some(&print) {
                continue;
            }
            let bars = input
                .get_or_insert_with(|| {
                    Arc::new(study_input(
                        self.display.shown(&self.series),
                        self.settings.zone,
                    ))
                })
                .clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let (flag, values) = (cancel.clone(), config.inputs.clone());
            let task = cx.spawn({
                let print = print.clone();
                async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move { script.compute(&bars, &values, limits, Some(flag)) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.custom_done(serial, print, result, cx);
                    });
                }
            });
            slot.running = Some(Running {
                cancel,
                _task: task,
            });
            slot.problems.clear();
        }
    }

    /// A run ended.
    fn custom_done(
        &mut self,
        serial: u64,
        print: Fingerprint,
        result: Result<custom::Computed, Vec<Problem>>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.custom.slots.iter().position(|s| s.serial == serial) else {
            return;
        };
        let slot = &mut self.custom.slots[index];
        slot.running = None;
        match result {
            Ok(done) => {
                slot.output = Some(done.output);
                slot.problems.clear();
                slot.log = done.log;
                slot.elapsed = Some(done.elapsed);
                slot.computed = Some(print);
                let output = slot.output.clone();
                if let Some(target) = self.display.studies.get_mut(index) {
                    *target = output;
                }
            }
            Err(problems) => {
                slot.output = None;
                slot.problems = problems;
                slot.computed = Some(print);
                if let Some(target) = self.display.studies.get_mut(index) {
                    *target = None;
                }
            }
        }
        if std::mem::take(&mut slot.dirty) {
            self.custom.pending = true;
        }
        cx.notify();
    }

    /// The scripts changed on disk, or one was renamed: follow the renames, take the new
    /// settings of the indicators, and run them again.
    pub(super) fn library_changed(&mut self, cx: &mut Context<Self>) {
        let renames = indicators::renames(cx);
        if !renames.is_empty()
            && self.settings.studies.iter().any(|s| {
                s.script
                    .as_deref()
                    .is_some_and(|id| renames.iter().any(|(old, _)| old == id))
            })
        {
            self.edit_settings(cx, |settings| {
                for study in &mut settings.studies {
                    let renamed = study.script.as_deref().and_then(|id| {
                        renames
                            .iter()
                            .find(|(old, _)| old == id)
                            .map(|(_, new)| new)
                    });
                    if let Some(new) = renamed {
                        study.script = Some(new.clone());
                    }
                }
            });
        }
        // The new declaration of a script may have other inputs and plots.
        self.edit_settings(cx, |_| {});
        self.custom.library_changed();
        cx.notify();
    }

    /// What the last run of the indicator at `index` said, when it is a script.
    pub(super) fn custom_status(&self, index: usize) -> Option<Status<'_>> {
        self.settings
            .studies
            .get(index)
            .filter(|s| s.is_script())
            .and_then(|_| self.custom.status(index))
    }

    /// What the last run of the script `id` said on this chart, when the chart holds it.
    pub fn script_report(&self, id: &str) -> Option<ScriptReport> {
        let index = self
            .settings
            .studies
            .iter()
            .position(|s| s.script.as_deref() == Some(id))?;
        let status = self.custom.status(index)?;
        Some(ScriptReport {
            running: status.running,
            problems: status.problems.to_vec(),
            log: status.log.to_vec(),
            elapsed: status.elapsed,
            bars: self.shown().len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chart::study::StudyKind;

    fn slot_for(custom: &Custom, index: usize) -> u64 {
        custom.slots[index].serial
    }

    #[test]
    fn a_slot_follows_its_indicator_when_others_are_added_or_taken_away() {
        let mut custom = Custom::default();
        let studies = vec![
            StudyConfig::new(StudyKind::Sma),
            StudyConfig::for_script("a"),
            StudyConfig::for_script("b"),
        ];
        custom.align(&studies);
        let (a, b) = (slot_for(&custom, 1), slot_for(&custom, 2));
        assert_ne!(a, b);

        // The first is taken away: the scripts keep their slots, at their new places.
        let after = vec![studies[1].clone(), studies[2].clone()];
        custom.align(&after);
        assert_eq!((slot_for(&custom, 0), slot_for(&custom, 1)), (a, b));

        // One is put in front, and a second copy of a script gets a slot of its own.
        let more = vec![
            StudyConfig::new(StudyKind::Rsi),
            after[0].clone(),
            after[1].clone(),
            StudyConfig::for_script("a"),
        ];
        custom.align(&more);
        assert_eq!((slot_for(&custom, 1), slot_for(&custom, 2)), (a, b));
        let copy = slot_for(&custom, 3);
        assert!(copy != a && copy != b);
    }

    #[test]
    fn what_was_run_is_forgotten_when_the_bars_are_no_longer_the_same_ones() {
        let mut custom = Custom::default();
        custom.data_changed(Some(1_000));
        custom.align(&[StudyConfig::for_script("a")]);
        custom.slots[0].output = Some(StudyOutput::default());
        custom.slots[0].computed = Some(Fingerprint {
            stamp: 1,
            inputs: BTreeMap::new(),
            wanted: custom.wanted,
        });

        // More bars at the end: the first is the same, the output stays (and a new run is wanted).
        custom.data_changed(Some(1_000));
        assert!(custom.slots[0].output.is_some() && custom.pending);

        // Older bars put in front: the first bar is another one, so nothing lines up.
        custom.data_changed(Some(400));
        assert!(custom.slots[0].output.is_none() && custom.slots[0].computed.is_none());
    }

    #[test]
    fn the_outputs_are_put_where_the_chart_draws_them_and_nothing_else_is_touched() {
        let mut custom = Custom::default();
        custom.align(&[
            StudyConfig::new(StudyKind::Sma),
            StudyConfig::for_script("a"),
        ]);
        custom.slots[1].output = Some(StudyOutput {
            levels: vec![70.0],
            ..StudyOutput::default()
        });
        let mut targets = vec![Some(StudyOutput::default()), None];
        custom.apply(&mut targets);
        assert_eq!(targets[0], Some(StudyOutput::default()));
        assert_eq!(targets[1].as_ref().unwrap().levels, [70.0]);
    }
}
