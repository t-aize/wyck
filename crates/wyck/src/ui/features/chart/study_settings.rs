//! The settings of one indicator: its inputs, the look of each of its plots, and how it shows.
//!
//! Every change applies at once, so the chart behind the panel shows the result live. OK keeps
//! the changes, Cancel puts the indicator back as it was, and Escape or the close button keep
//! them.

mod build;
pub mod inputs;
pub mod style;

use super::Chart;
use super::study::config::StudyConfig;
use super::study::kind::StudyKind;
use super::study::spec::{InputKind, Placement};
use super::study::style::{FillStyle, LevelStyle};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::input::{InputEvent, InputState};
use wyck_ui_kit::{button, controls, form, form::Head, form::Tab, modal, number, theme};

/// How tall a pane is, as the choices the panel offers: a name and its weight against the prices.
const PANE_HEIGHTS: &[(&str, f32)] = &[
    ("Compact", 0.6),
    ("Normal", 1.0),
    ("Tall", 1.6),
    ("Huge", 2.4),
];

/// Opens the settings of indicator `index` of `chart`.
pub fn open(chart: Entity<Chart>, index: usize, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the panel reads it.
    window.defer(cx, move |window, cx| {
        let Some(config) = chart.read(cx).settings.studies.get(index).cloned() else {
            return;
        };
        let editor = cx.new(|cx| StudyEditor::new(chart, index, config, window, cx));
        modal::open(
            editor,
            modal::Options::new(780.0, 600.0).label("Indicator settings"),
            window,
            cx,
        );
    });
}

/// The indicator a panel edits: which chart, where in its list, and of what kind (so a panel left
/// open after the list changed edits nothing).
#[derive(Clone)]
struct Target {
    chart: Entity<Chart>,
    index: usize,
    kind: StudyKind,
}

impl Target {
    fn edit(&self, cx: &mut App, change: impl FnOnce(&mut StudyConfig)) {
        let (index, kind) = (self.index, self.kind);
        self.chart.update(cx, |chart, cx| {
            chart.edit_settings(cx, |settings| {
                if let Some(study) = settings.studies.get_mut(index).filter(|s| s.kind == kind) {
                    change(study);
                }
            });
        });
    }

    fn set_text(&self, key: &'static str, value: String, cx: &mut App) {
        self.edit(cx, |study| {
            study.texts.insert(key.to_owned(), value);
        });
    }

    fn set_input(&self, key: &'static str, value: f64, cx: &mut App) {
        self.edit(cx, |study| {
            study.inputs.insert(key.to_owned(), value);
        });
    }

    fn plot(
        &self,
        key: &'static str,
        cx: &mut App,
        change: impl FnOnce(&mut super::study::style::PlotStyle),
    ) {
        self.edit(cx, |study| {
            if let Some(style) = study.plots.get_mut(key) {
                change(style);
            }
        });
    }

    fn level(&self, index: usize, cx: &mut App, change: impl FnOnce(&mut LevelStyle)) {
        self.edit(cx, |study| change(study.levels.entry(index).or_default()));
    }

    fn fill(
        &self,
        index: usize,
        default: FillStyle,
        cx: &mut App,
        change: impl FnOnce(&mut FillStyle),
    ) {
        self.edit(cx, |study| {
            change(study.fills.entry(index).or_insert(default))
        });
    }

    fn band(&self, default: FillStyle, cx: &mut App, change: impl FnOnce(&mut FillStyle)) {
        self.edit(cx, |study| change(study.band.get_or_insert(default)));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Inputs,
    Style,
    Display,
}

impl Page {
    fn tab(self) -> Tab {
        match self {
            Self::Inputs => Tab {
                label: "Inputs",
                icon: IconName::SlidersHorizontal,
            },
            Self::Style => Tab {
                label: "Style",
                icon: IconName::Palette,
            },
            Self::Display => Tab {
                label: "Display",
                icon: IconName::Eye,
            },
        }
    }
}

struct StudyEditor {
    target: Target,
    chart: Entity<Chart>,
    index: usize,
    kind: StudyKind,
    /// How the indicator was when the panel opened, for Cancel.
    original: StudyConfig,
    page: Page,
    /// The number fields of the inputs.
    fields: Vec<(&'static str, Entity<InputState>)>,
    /// The text fields of the inputs that are text: symbols, timeframes, sessions, free text.
    texts: Vec<(&'static str, Entity<InputState>)>,
    /// The opacity field of each plot, in percent.
    opacity: Vec<(&'static str, Entity<InputState>)>,
    /// The width field of each plot, in pixels: any width, beside the presets.
    widths: Vec<(&'static str, Entity<InputState>)>,
    bar_widths: Vec<(&'static str, Entity<InputState>)>,
    level_values: Vec<(usize, Entity<InputState>)>,
    level_widths: Vec<(usize, Entity<InputState>)>,
    level_opacity: Vec<(usize, Entity<InputState>)>,
    fill_opacity: Vec<(usize, Entity<InputState>)>,
    band_opacity: Option<Entity<InputState>>,
    name: Entity<InputState>,
    precision: Entity<InputState>,
    color_open: Option<&'static str>,
    up_color_open: Option<&'static str>,
    down_color_open: Option<&'static str>,
    level_color_open: Option<usize>,
    fill_color_open: Option<usize>,
    fill_other_open: Option<usize>,
    band_color_open: bool,
    _subscriptions: Vec<Subscription>,
}

impl StudyEditor {
    fn pages(config: &StudyConfig) -> Vec<Page> {
        let mut pages = Vec::new();
        if !config.spec().inputs.is_empty() || config.is_script() {
            pages.push(Page::Inputs);
        }
        pages.push(Page::Style);
        pages.push(Page::Display);
        pages
    }
}

impl Render for StudyEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(config) = self
            .chart
            .read(cx)
            .settings
            .studies
            .get(self.index)
            .filter(|s| s.kind == self.kind)
            .cloned()
        else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_xl()
                .border_1()
                .border_color(theme::border_subtle())
                .bg(theme::bg())
                .child(form::empty(IconName::Info, "This indicator was removed."))
                .into_any_element();
        };
        let pages = Self::pages(&config);
        if !pages.contains(&self.page) {
            self.page = pages[0];
        }
        let active = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        let tabs: Vec<Tab> = pages.iter().map(|p| p.tab()).collect();

        let spec = config.spec();
        let head = Head {
            icon: match spec.placement {
                Placement::Overlay => IconName::ChartLine,
                Placement::Pane => IconName::Activity,
            },
            title: spec.label.into(),
            subtitle: format!(
                "{} - {}",
                config.title(),
                match spec.placement {
                    Placement::Overlay => "drawn on the prices",
                    Placement::Pane => "in a pane of its own",
                }
            )
            .into(),
        };

        let body = match self.page {
            Page::Inputs => self.inputs_page(&config, cx),
            Page::Style => self.style_page(&config, cx),
            Page::Display => self.display_page(&config),
        };

        let this = cx.entity();
        let (defaults, cancel, remove) = (cx.entity(), cx.entity(), cx.entity());
        let footer = form::footer(
            vec![
                button::action(
                    "study-defaults",
                    "Defaults",
                    Some(IconName::RotateCcw),
                    false,
                    move |window, cx| {
                        defaults.update(cx, |e, cx| {
                            let Some(fresh) = e
                                .chart
                                .read(cx)
                                .settings
                                .studies
                                .get(e.index)
                                .map(StudyConfig::fresh)
                            else {
                                return;
                            };
                            let again = fresh.clone();
                            e.target.edit(cx, |study| {
                                let visible = study.visible;
                                *study = again;
                                study.visible = visible;
                            });
                            e.set_fields(&fresh, window, cx);
                        });
                    },
                )
                .into_any_element(),
                button::action(
                    "study-remove",
                    "Remove",
                    Some(IconName::Trash),
                    false,
                    move |window, cx| {
                        let (chart, index) = {
                            let e = remove.read(cx);
                            (e.chart.clone(), e.index)
                        };
                        chart.update(cx, |chart, cx| chart.remove_study(index, cx));
                        modal::close(window, cx);
                    },
                )
                .into_any_element(),
            ],
            vec![
                button::action("study-cancel", "Cancel", None, false, move |window, cx| {
                    cancel.update(cx, |e, cx| {
                        let original = e.original.clone();
                        e.target.edit(cx, |study| *study = original);
                    });
                    modal::close(window, cx);
                })
                .into_any_element(),
                button::action("study-ok", "OK", None, true, |window, cx| {
                    modal::close(window, cx)
                })
                .into_any_element(),
            ],
        );

        form::frame(
            head,
            &tabs,
            active,
            move |index, _window, cx| {
                let page = pages[index];
                this.update(cx, |e, cx| {
                    e.page = page;
                    e.color_open = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
        .into_any_element()
    }
}

/// Puts rows into cards, one card for each group a script named, like the groups of the other
/// settings panels. The rows with no group go in a card called `fallback`. The cards come in the
/// order their group first appears, and the rows of a group that comes back later join its card.
fn grouped(
    icon: IconName,
    fallback: &'static str,
    rows: Vec<(&'static str, AnyElement)>,
) -> Vec<AnyElement> {
    split_by_group(fallback, rows)
        .into_iter()
        .map(|(title, rows)| form::group(icon, title, rows).into_any_element())
        .collect()
}

/// The part of [`grouped`] that does not draw: `items` cut into runs by group name, an empty name
/// standing for `fallback`.
fn split_by_group<T>(
    fallback: &'static str,
    items: Vec<(&'static str, T)>,
) -> Vec<(&'static str, Vec<T>)> {
    let mut sections: Vec<(&'static str, Vec<T>)> = Vec::new();
    for (group, item) in items {
        let title = if group.trim().is_empty() {
            fallback
        } else {
            group
        };
        match sections.iter_mut().find(|(name, _)| *name == title) {
            Some((_, rows)) => rows.push(item),
            None => sections.push((title, vec![item])),
        }
    }
    sections
}

/// The timeframes offered beside a timeframe input; the first one is the chart's own.
const TIMEFRAME_PICKS: [(&str, &str); 8] = [
    ("Chart", ""),
    ("1m", "1m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("1h", "1h"),
    ("4h", "4h"),
    ("1D", "1D"),
    ("1W", "1W"),
];

/// A row of quick picks above the text field of a timeframe input.
fn timeframe_picks(
    key: &'static str,
    target: Target,
    field: Entity<InputState>,
    current: String,
) -> gpui::Div {
    let labels: Vec<&str> = TIMEFRAME_PICKS.iter().map(|(label, _)| *label).collect();
    let chosen = TIMEFRAME_PICKS
        .iter()
        .position(|(_, code)| *code == current)
        .unwrap_or(usize::MAX);
    div().flex().flex_col().gap_2().child(controls::segmented(
        SharedString::from(format!("timeframe-{key}")),
        &labels,
        chosen,
        move |index, window, cx| {
            let code = TIMEFRAME_PICKS[index].1;
            // Setting a field's value does not announce a change, so the study is told here.
            target.set_text(key, code.to_owned(), cx);
            field.update(cx, |state, cx| state.set_value(code, window, cx));
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::split_by_group;

    #[test]
    fn inputs_with_no_group_stay_in_one_card_under_the_fallback_title() {
        let cards = split_by_group("Parameters", vec![("", 1), ("", 2), (" ", 3)]);
        assert_eq!(cards, vec![("Parameters", vec![1, 2, 3])]);
    }

    #[test]
    fn each_named_group_gets_its_own_card_in_the_order_it_first_appears() {
        let cards = split_by_group(
            "Parameters",
            vec![("Data", 1), ("", 2), ("Display", 3), ("Data", 4), ("", 5)],
        );
        assert_eq!(
            cards,
            vec![
                ("Data", vec![1, 4]),
                ("Parameters", vec![2, 5]),
                ("Display", vec![3]),
            ]
        );
    }

    #[test]
    fn no_input_means_no_card() {
        assert!(split_by_group::<u8>("Parameters", Vec::new()).is_empty());
    }
}
