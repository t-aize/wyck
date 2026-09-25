//! The list of the drawings of a symbol: pick one, show or hide it, lock it, move it up or down
//! the stack, open its settings or delete it. Also where the icon of each tool is chosen.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable};

use super::drawing::Drawings;
use super::drawing::book::{Book, Order};
use super::drawing::model::{Drawing, Tool};
use super::drawing_props;
use super::zone::Zone;
use crate::app::connection::ui::icon_colored;
use crate::app::settings_ui::{self as ui, Head};
use crate::app::{modal, theme};

pub fn tool_icon(tool: Tool) -> IconName {
    match tool {
        Tool::TrendLine => IconName::TrendingUp,
        Tool::Ray => IconName::MoveUpRight,
        Tool::ExtendedLine => IconName::MoveDiagonal,
        Tool::HorizontalLine => IconName::Minus,
        Tool::HorizontalRay => IconName::MoveRight,
        Tool::VerticalLine => IconName::SeparatorVertical,
        Tool::CrossLine => IconName::Plus,
        Tool::Arrow => IconName::ArrowUpRight,
        Tool::ArrowPath => IconName::Spline,
        Tool::InfoLine => IconName::Info,
        Tool::TrendAngle => IconName::Angle,
        Tool::RegressionTrend => IconName::ChartLine,
        Tool::FlatTopBottom => IconName::StretchHorizontal,
        Tool::DisjointChannel => IconName::Layers2,
        Tool::Pitchfan => IconName::Fan,
        Tool::FibTimeExtension => IconName::Timer,
        Tool::FibCircles => IconName::CircleDot,
        Tool::FibSpiral => IconName::RotateCw,
        Tool::FibArcs => IconName::Orbit,
        Tool::FibWedge => IconName::TriangleRight,
        Tool::GannSquareFixed => IconName::Blocks,
        Tool::CyclicLines => IconName::Repeat,
        Tool::TimeCycles => IconName::AudioWaveform,
        Tool::SineLine => IconName::ChartSpline,
        Tool::RotatedRectangle => IconName::VectorSquare,
        Tool::Circle => IconName::Circle,
        Tool::Arc => IconName::Radius,
        Tool::Curve => IconName::SplinePointer,
        Tool::DoubleCurve => IconName::Spline,
        Tool::Highlighter => IconName::Highlighter,
        Tool::ArrowMarker => IconName::ArrowUpFromLine,
        Tool::ArrowMarkUp => IconName::ArrowBigUp,
        Tool::ArrowMarkDown => IconName::ArrowBigDown,
        Tool::PriceRange => IconName::MoveVertical,
        Tool::DateRange => IconName::MoveHorizontal,
        Tool::DatePriceRange => IconName::Scan,
        Tool::Forecast => IconName::ChartArea,
        Tool::BarsPattern => IconName::ChartCandlestick,
        Tool::GhostFeed => IconName::Ghost,
        Tool::AnchoredVwap => IconName::Sigma,
        Tool::FixedRangeVolumeProfile => IconName::ChartBarBig,
        Tool::AnchoredVolumeProfile => IconName::ChartColumn,
        Tool::Note => IconName::StickyNote,
        Tool::PriceNote => IconName::NotebookPen,
        Tool::Callout => IconName::MessageSquareText,
        Tool::Comment => IconName::MessageCircle,
        Tool::Pin => IconName::Pin,
        Tool::Signpost => IconName::Signpost,
        Tool::FlagMark => IconName::Flag,
        Tool::Table => IconName::Table,
        Tool::Icon => IconName::Star,
        Tool::ParallelChannel => IconName::Rows2,
        Tool::Pitchfork => IconName::GitFork,
        Tool::SchiffPitchfork => IconName::GitBranch,
        Tool::ModifiedSchiffPitchfork => IconName::GitMerge,
        Tool::InsidePitchfork => IconName::GitPullRequest,
        Tool::FibRetracement | Tool::FibExtension => IconName::ChartNoAxesGantt,
        Tool::FibChannel => IconName::Rows3,
        Tool::FibTimeZone => IconName::Columns3,
        Tool::FibFan | Tool::GannFan => IconName::Fan,
        Tool::GannBox => IconName::Grid2x2,
        Tool::GannSquare => IconName::Grid3x3,
        Tool::Xabcd => IconName::Waypoints,
        Tool::Cypher => IconName::Route,
        Tool::Abcd => IconName::Spline,
        Tool::HeadAndShoulders => IconName::Mountain,
        Tool::TrianglePattern => IconName::ChevronsLeftRight,
        Tool::ThreeDrives => IconName::Activity,
        Tool::ElliottImpulse => IconName::TrendingUpDown,
        Tool::ElliottCorrection => IconName::TrendingDown,
        Tool::ElliottTriangle => IconName::GitCommitHorizontal,
        Tool::ElliottDoubleCombo => IconName::Workflow,
        Tool::ElliottTripleCombo => IconName::Share2,
        Tool::Triangle => IconName::Triangle,
        Tool::Rectangle => IconName::Square,
        Tool::Ellipse => IconName::Ellipse,
        Tool::Brush => IconName::Brush,
        Tool::Measure => IconName::Ruler,
        Tool::LongPosition => IconName::ArrowBigUp,
        Tool::ShortPosition => IconName::ArrowBigDown,
        Tool::Text => IconName::Type,
        Tool::PriceLabel => IconName::Tag,
        Tool::Unknown => IconName::Pen,
    }
}

/// Opens the list of the drawings of `symbol`.
pub fn open(
    drawings: Entity<Drawings>,
    symbol: String,
    zone: Zone,
    digits: u32,
    window: &mut Window,
    cx: &mut App,
) {
    // Opened once whatever asked is done updating, since the dialog reads the drawings.
    window.defer(cx, move |window, cx| {
        let tree = cx.new(|cx| ObjectTree {
            _observe: cx.observe(&drawings, |_this, _drawings, cx| cx.notify()),
            drawings,
            symbol,
            zone,
            digits,
        });
        modal::open(tree, modal::Options::new(600.0, 560.0), window, cx);
    });
}

struct ObjectTree {
    drawings: Entity<Drawings>,
    symbol: String,
    zone: Zone,
    digits: u32,
    _observe: Subscription,
}

/// What a button of a row does to the drawing: it gets the book, the symbol, the id and a flag.
type RowChange = fn(&mut Book, &str, u64, bool) -> bool;

impl ObjectTree {
    fn edit(&self, cx: &mut App, change: impl FnOnce(&mut Book, &str) -> bool) {
        let symbol = self.symbol.clone();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }

    /// One row of the list: the drawing, and the buttons that act on it.
    fn row(
        &self,
        drawing: &Drawing,
        position: usize,
        count: usize,
        selected: bool,
        this: &Entity<Self>,
    ) -> AnyElement {
        let id = drawing.id;
        let (hidden, locked) = (drawing.hidden, drawing.locked);
        let restricted = drawing.timeframes.is_some();
        let button = |name: &str, icon: IconName, tip: &'static str| {
            Button::new(SharedString::from(format!("{name}-{id}")))
                .cursor_pointer()
                .ghost()
                .xsmall()
                .icon(icon)
                .tooltip(tip)
        };
        // A click that runs `change` on this drawing with `flag`.
        let act = |change: RowChange, flag: bool| {
            let this = this.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |tree, cx| {
                    tree.edit(cx, |book, symbol| change(book, symbol, id, flag));
                });
            }
        };
        let (pick, settings) = (this.clone(), this.clone());
        div()
            .id(SharedString::from(format!("tree-row-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(34.))
            .px_2()
            .rounded_md()
            .when(selected, |el| el.bg(theme::accent_selected()))
            .hover(|s| s.bg(theme::surface_hover()))
            .cursor_pointer()
            .on_click(move |_, _window, cx| {
                pick.update(cx, |tree, cx| {
                    tree.edit(cx, |book, _| {
                        book.select(Some(id));
                        true
                    });
                });
            })
            .child(icon_colored(
                tool_icon(drawing.tool),
                15.,
                if hidden {
                    theme::muted_fg()
                } else {
                    gpui::rgb(drawing.style.color)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(if hidden {
                        theme::muted_fg()
                    } else {
                        theme::fg()
                    })
                    .child(drawing.title()),
            )
            .when(restricted, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child("Some timeframes"),
                )
            })
            .child(
                button("tree-up", IconName::ChevronUp, "Bring forward")
                    .disabled(position + 1 == count)
                    .on_click(act(
                        |book, symbol, id, _| book.reorder(symbol, id, Order::Forward),
                        false,
                    )),
            )
            .child(
                button("tree-down", IconName::ChevronDown, "Send backward")
                    .disabled(position == 0)
                    .on_click(act(
                        |book, symbol, id, _| book.reorder(symbol, id, Order::Backward),
                        false,
                    )),
            )
            .child(
                button(
                    "tree-eye",
                    if hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    },
                    if hidden { "Show" } else { "Hide" },
                )
                .on_click(act(
                    |book, symbol, id, flag| book.set_hidden(symbol, id, flag),
                    !hidden,
                )),
            )
            .child(
                button(
                    "tree-lock",
                    if locked {
                        IconName::Lock
                    } else {
                        IconName::LockOpen
                    },
                    if locked { "Unlock" } else { "Lock" },
                )
                .on_click(act(
                    |book, symbol, id, flag| book.set_locked(symbol, id, flag),
                    !locked,
                )),
            )
            .child(
                button("tree-settings", IconName::Settings2, "Settings").on_click(
                    move |_, window, cx| {
                        let (drawings, symbol, zone, digits) = {
                            let tree = settings.read(cx);
                            (
                                tree.drawings.clone(),
                                tree.symbol.clone(),
                                tree.zone,
                                tree.digits,
                            )
                        };
                        drawing_props::open(drawings, symbol, id, zone, digits, window, cx);
                    },
                ),
            )
            .child(
                button("tree-delete", IconName::Trash, "Delete")
                    .disabled(locked)
                    .on_click(act(|book, symbol, id, _| book.delete(symbol, id), false)),
            )
            .into_any_element()
    }
}

impl Render for ObjectTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let book = self.drawings.read(cx).book();
        let list = book.drawings(&self.symbol).to_vec();
        let selected = book.selected();
        let this = cx.entity();
        let count = list.len();
        // Some hidden is enough to offer showing them again.
        let some_hidden = list.iter().any(|d| d.hidden);
        let head = Head {
            icon: IconName::ListTree,
            title: format!("Drawings on {}", self.symbol).into(),
            subtitle: match count {
                0 => "Nothing drawn yet".into(),
                1 => "1 drawing".into(),
                n => format!("{n} drawings, the top one first").into(),
            },
        };
        let body = if list.is_empty() {
            ui::empty(
                IconName::Pencil,
                "No drawings on this symbol yet. Pick a tool on the left of the chart.",
            )
            .into_any_element()
        } else {
            let mut rows = div().flex().flex_col().gap_0p5();
            for (position, drawing) in list.iter().enumerate().rev() {
                let is_selected = selected == Some(drawing.id);
                rows = rows.child(self.row(drawing, position, count, is_selected, &this));
            }
            rows.into_any_element()
        };
        let (show, clear) = (this.clone(), this);
        let footer = ui::footer(
            vec![
                ui::action(
                    "tree-show-all",
                    if some_hidden { "Show all" } else { "Hide all" },
                    Some(if some_hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    }),
                    false,
                    move |_window, cx| {
                        show.update(cx, |tree, cx| {
                            tree.edit(cx, |book, symbol| book.set_all_hidden(symbol, !some_hidden));
                        });
                    },
                )
                .disabled(list.is_empty())
                .into_any_element(),
                ui::action(
                    "tree-clear",
                    "Remove unlocked",
                    Some(IconName::Trash),
                    false,
                    move |_window, cx| {
                        clear.update(cx, |tree, cx| {
                            tree.edit(cx, |book, symbol| book.clear(symbol));
                        });
                    },
                )
                .disabled(list.is_empty())
                .into_any_element(),
            ],
            vec![ui::action("tree-close", "Close", None, true, modal::close).into_any_element()],
        );
        ui::dialog(head, modal::dismiss, body, footer)
    }
}
