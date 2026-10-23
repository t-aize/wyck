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

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Context, Task};

use super::Chart;
use super::study::compute::StudyContext;
use super::study::compute::StudyOutput;
use super::study::config::StudyConfig;
use super::study::custom::library::registry;
use super::study::custom::other::{OtherBars, OtherSymbols};
use super::study::custom::run::Setup;
use super::study::custom::{self, Problem, Severity};
use wyck_app::lost::lost;
use wyck_app::market_data::load::{self, Loaded};
use wyck_app::market_data::now_ms;
use wyck_app::scripts as indicators;
use wyck_app::system::runtime;
use wyck_domain::chart::{study_context, study_input};

/// What a run was made from, to know whether another is wanted.
#[derive(Debug, Clone, PartialEq)]
struct Fingerprint {
    /// Which version of the script.
    stamp: u64,
    inputs: BTreeMap<String, f64>,
    texts: BTreeMap<String, String>,
    /// Which state of the data and the settings.
    wanted: u64,
    /// The zone, timeframe and symbol the script saw.
    frame: String,
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
pub(crate) struct Status<'a> {
    pub(crate) running: bool,
    pub(crate) problems: &'a [Problem],
    pub(crate) log: &'a [String],
    pub(crate) elapsed: Option<Duration>,
}

impl Status<'_> {
    pub(crate) fn first_error(&self) -> Option<&Problem> {
        self.problems.iter().find(|p| p.severity == Severity::Error)
    }
}

/// The prices of another symbol a script reads, on the timeframe of the chart.
enum Other {
    /// Being loaded. Dropping it stops the load.
    Loading(#[expect(dead_code, reason = "held so that dropping it stops the load")] Task<()>),
    Ready(Arc<OtherBars>),
    Failed(String),
}

/// What the symbols a script reads come to, for one run.
enum Others {
    Ready(OtherSymbols),
    /// Some are still being loaded; the run starts when they are in.
    Waiting,
    Failed(String),
}

#[derive(Default)]
pub(super) struct Custom {
    slots: Vec<Slot>,
    /// The other symbols the scripts read, by name (see `key_of`). Forgotten whenever the chart
    /// loads again, since they are on its timeframe.
    others: HashMap<String, Other>,
    /// The time of the first bar the outputs were made from.
    first_time: Option<i64>,
    /// Grows whenever the data or the settings want a new run.
    wanted: u64,
    /// Whether runs must be looked at (at the next frame, which has a context to spawn from).
    pending: bool,
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

    /// The replay went back, or jumped: what the scripts gave was made from more bars than the
    /// chart shows, and may hold what has not happened yet. It is thrown away, and the runs in
    /// flight are stopped and their answers ignored.
    pub(super) fn reset_outputs(&mut self) {
        for slot in &mut self.slots {
            slot.running = None;
            self.next_serial += 1;
            slot.serial = self.next_serial;
            slot.output = None;
            slot.computed = None;
            slot.dirty = false;
        }
        self.wanted += 1;
        self.pending = true;
    }

    /// The chart loads again: the prices of the other symbols were for the old symbol or
    /// timeframe.
    pub(super) fn forget_others(&mut self) {
        self.others.clear();
        self.pending = true;
    }

    /// The names of symbols the account has arrived (or changed): a name that was not found may
    /// be found now.
    pub(super) fn symbols_changed(&mut self) {
        self.others
            .retain(|_, other| !matches!(other, Other::Failed(_)));
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
                .and_then(|at| old.get_mut(at).and_then(Option::take))
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
        let mut context: Option<Arc<StudyContext>> = None;
        let symbol_name = self
            .symbol
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let digits = self.symbol.as_ref().map_or(0, |s| s.digits);
        let live = self.replay.is_none();
        let frame = format!(
            "{}|{}|{}|{}|{}",
            self.settings.zone.code(),
            self.timeframe.code(),
            symbol_name,
            digits,
            live
        );
        for (index, config) in self.settings.studies.clone().into_iter().enumerate() {
            if !config.is_script() {
                continue;
            }
            let id = config.script.clone().unwrap_or_default();
            let Some(slot) = self.custom.slots.get_mut(index) else {
                continue;
            };
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
                slot.problems.clone_from(&entry.problems);
                continue;
            };
            let print = Fingerprint {
                stamp: entry.stamp,
                inputs: config.inputs.clone(),
                texts: config.texts.clone(),
                wanted: self.custom.wanted,
                frame: frame.clone(),
            };
            if slot.running.is_some() {
                slot.dirty = true;
                continue;
            }
            if slot.computed.as_ref() == Some(&print) {
                continue;
            }
            let bars = Arc::clone(input.get_or_insert_with(|| {
                Arc::new(study_input(
                    self.display.shown(&self.series),
                    self.settings.zone,
                ))
            }));
            let context = Arc::clone(context.get_or_insert_with(|| {
                Arc::new(study_context(
                    self.display.shown(&self.series),
                    self.settings.zone,
                    Some(self.timeframe),
                    &symbol_name,
                    digits,
                    live,
                ))
            }));
            let wanted = script.symbols_wanted(&config.texts);
            let mut others = match self.other_symbols(&wanted, cx) {
                Others::Ready(others) => others,
                Others::Waiting => continue,
                Others::Failed(message) => {
                    if let Some(slot) = self.custom.slots.get_mut(index) {
                        slot.output = None;
                        slot.problems = vec![Problem::error(0, 0, message)];
                    }
                    continue;
                }
            };
            // The symbol of the chart, named in an input, is the bars the run already has.
            let own = self
                .symbol
                .as_ref()
                .map(|s| custom::other::key_of(&s.name))
                .filter(|own| wanted.contains(own));
            if let Some(own) = own {
                others.insert(own, OtherBars::from_input(&bars));
            }
            let Some(slot) = self.custom.slots.get_mut(index) else {
                continue;
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let (flag, values, texts) = (
                Arc::clone(&cancel),
                config.inputs.clone(),
                config.texts.clone(),
            );
            let task = cx.spawn({
                let print = print.clone();
                async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let setup = Setup {
                                values: &values,
                                texts: &texts,
                                others: &others,
                                context: &context,
                            };
                            script.compute_in(&bars, &setup, limits, Some(flag))
                        })
                        .await;
                    lost(
                        "custom_runs: schedule_customs",
                        this.update(cx, |this, cx| {
                            this.custom_done(serial, print, result, cx);
                        }),
                    );
                }
            });
            slot.running = Some(Running {
                cancel,
                _task: task,
            });
            slot.problems.clear();
        }
    }

    /// The bars of the other symbols a script reads. A symbol not asked for yet is looked up by
    /// name and loaded on the timeframe of the chart. The symbol of the chart itself is not
    /// handled here.
    fn other_symbols(&mut self, wanted: &[String], cx: &mut Context<Self>) -> Others {
        let own = self.symbol.as_ref().map(|s| custom::other::key_of(&s.name));
        // The prices of another symbol are loaded up to now, which a replay must not show.
        if self.replay.is_some() && wanted.iter().any(|key| Some(key) != own.as_ref()) {
            return Others::Failed(
                "the prices of another symbol are not available in a replay".to_owned(),
            );
        }
        let mut ready = OtherSymbols::new();
        let mut waiting = false;
        for key in wanted.iter().filter(|key| Some(*key) != own.as_ref()) {
            match self.custom.others.get(key) {
                Some(Other::Ready(bars)) => {
                    ready.insert(key.clone(), (**bars).clone());
                }
                Some(Other::Loading(_)) => waiting = true,
                Some(Other::Failed(message)) => {
                    return Others::Failed(format!(
                        "the prices of {key} could not be loaded: {message}"
                    ));
                }
                None => {
                    let Some(id) = self.symbol_ids.get(key).copied() else {
                        // Until the list of symbols is in there is nothing to say against a name.
                        if self.symbol_ids.is_empty() {
                            waiting = true;
                            continue;
                        }
                        return Others::Failed(format!(
                            "{key} is not a symbol of this account, change the symbol input"
                        ));
                    };
                    self.load_other(key.clone(), id, cx);
                    waiting = true;
                }
            }
        }
        if waiting {
            Others::Waiting
        } else {
            Others::Ready(ready)
        }
    }

    /// Loads the bars of another symbol on the timeframe of the chart.
    fn load_other(&mut self, key: String, id: i64, cx: &mut Context<Self>) {
        let (session, timeframe, epoch) = (self.session.clone(), self.timeframe, self.epoch);
        let name = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let result =
                runtime::spawn(
                    async move { load::initial(&session, id, timeframe, now_ms()).await },
                )
                .await;
            lost(
                "custom_runs: load_other",
                this.update(cx, |this, cx| {
                    this.other_loaded(epoch, name, super::flatten(result), cx);
                }),
            );
        });
        self.custom.others.insert(key, Other::Loading(task));
    }

    fn other_loaded(
        &mut self,
        epoch: u64,
        key: String,
        result: wyck_app::broker::Result<Loaded>,
        cx: &mut Context<Self>,
    ) {
        if epoch != self.epoch {
            return;
        }
        let other = match result {
            Ok(Loaded::Bars(bars) | Loaded::Grouped { bars, .. }) if !bars.is_empty() => {
                Other::Ready(Arc::new(OtherBars::from_bars(&bars)))
            }
            Ok(Loaded::Bars(_) | Loaded::Grouped { .. }) => {
                Other::Failed("the server has no bars for it".to_owned())
            }
            Ok(Loaded::Ticks(_)) => {
                Other::Failed("the chart is on ticks, which have no bars to line up".to_owned())
            }
            Err(error) => Other::Failed(error.to_string()),
        };
        self.custom.others.insert(key, other);
        self.custom.wanted += 1;
        self.custom.pending = true;
        cx.notify();
    }

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
        let Some(slot) = self.custom.slots.get_mut(index) else {
            return;
        };
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
    use crate::chart::chart::study::kind::StudyKind;

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
            texts: BTreeMap::new(),
            wanted: custom.wanted,
            frame: String::new(),
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
