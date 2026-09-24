//! The list of the drawings of a symbol: pick one, show or hide it, lock it, move it up or down
//! the stack, open its settings or delete it. Also where the icon of each tool is chosen.

use gpui::prelude::*;
use gpui::{App, Context, Entity, SharedString, Subscription, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable, WindowExt};

use super::drawing::Drawings;
use super::drawing::book::Order;
use super::drawing::model::Tool;
use super::drawing_props;
use super::zone::Zone;
use crate::app::connection::ui;
use crate::app::theme;

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
            symbol: symbol.clone(),
            zone,
            digits,
        });
        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .title(format!("Drawings on {symbol}"))
                .w(px(520.))
                .child(tree.clone())
        });
    });
}

struct ObjectTree {
    drawings: Entity<Drawings>,
    symbol: String,
    zone: Zone,
    digits: u32,
    _observe: Subscription,
}

impl ObjectTree {
    fn edit(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut super::drawing::book::Book, &str) -> bool,
    ) {
        let symbol = self.symbol.clone();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }
}

impl Render for ObjectTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let book = self.drawings.read(cx).book();
        let list = book.drawings(&self.symbol).to_vec();
        let selected = book.selected();
        let this = cx.entity();
        if list.is_empty() {
            return div()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .text_size(px(13.))
                .text_color(theme::muted_fg())
                .child(ui::icon_colored(IconName::Pencil, 22., theme::muted_fg()))
                .child("No drawings on this symbol yet. Pick a tool on the left of the chart.")
                .into_any_element();
        }
        let all_hidden = list.iter().all(|d| d.hidden);
        let count = list.len();
        let (show_this, lock_this) = (this.clone(), this.clone());
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pb_2()
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .child(format!(
                        "{count} drawing{}, the top one first",
                        if count == 1 { "" } else { "s" }
                    )),
            )
            .child(
                Button::new("tree-show-all")
                    .ghost()
                    .xsmall()
                    .icon(if all_hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    })
                    .label(if all_hidden { "Show all" } else { "Hide all" })
                    .on_click(move |_, _window, cx| {
                        show_this.update(cx, |tree, cx| {
                            tree.edit(cx, |book, symbol| book.set_all_hidden(symbol, !all_hidden));
                        });
                    }),
            )
            .child(
                Button::new("tree-clear")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Trash)
                    .label("Remove unlocked")
                    .on_click(move |_, _window, cx| {
                        lock_this.update(cx, |tree, cx| {
                            tree.edit(cx, |book, symbol| book.clear(symbol))
                        });
                    }),
            );
        let mut rows = div()
            .id("tree-rows")
            .flex()
            .flex_col()
            .gap_0p5()
            .max_h(px(420.))
            .overflow_y_scroll();
        for (position, drawing) in list.iter().enumerate().rev() {
            let id = drawing.id;
            let is_selected = selected == Some(id);
            let muted = drawing.hidden;
            let restricted = drawing.timeframes.is_some();
            let (pick, up, down, eye, lock, settings, delete) = (
                this.clone(),
                this.clone(),
                this.clone(),
                this.clone(),
                this.clone(),
                this.clone(),
                this.clone(),
            );
            let (hidden, locked) = (drawing.hidden, drawing.locked);
            let small = |name: String, icon: IconName, tip: &'static str| {
                Button::new(SharedString::from(name))
                    .ghost()
                    .xsmall()
                    .icon(icon)
                    .tooltip(tip)
            };
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("tree-row-{id}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(34.))
                    .px_2()
                    .rounded_md()
                    .when(is_selected, |el| el.bg(theme::accent_selected()))
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
                    .child(ui::icon_colored(
                        tool_icon(drawing.tool),
                        15.,
                        if muted {
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
                            .text_color(if muted {
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
                        small(
                            format!("tree-up-{id}"),
                            IconName::ChevronUp,
                            "Bring forward",
                        )
                        .disabled(position + 1 == count)
                        .on_click(move |_, _window, cx| {
                            up.update(cx, |tree, cx| {
                                tree.edit(cx, |book, symbol| {
                                    book.reorder(symbol, id, Order::Forward)
                                });
                            });
                        }),
                    )
                    .child(
                        small(
                            format!("tree-down-{id}"),
                            IconName::ChevronDown,
                            "Send backward",
                        )
                        .disabled(position == 0)
                        .on_click(move |_, _window, cx| {
                            down.update(cx, |tree, cx| {
                                tree.edit(cx, |book, symbol| {
                                    book.reorder(symbol, id, Order::Backward)
                                });
                            });
                        }),
                    )
                    .child(
                        small(
                            format!("tree-eye-{id}"),
                            if hidden {
                                IconName::EyeOff
                            } else {
                                IconName::Eye
                            },
                            if hidden { "Show" } else { "Hide" },
                        )
                        .on_click(move |_, _window, cx| {
                            eye.update(cx, |tree, cx| {
                                tree.edit(cx, |book, symbol| book.set_hidden(symbol, id, !hidden));
                            });
                        }),
                    )
                    .child(
                        small(
                            format!("tree-lock-{id}"),
                            if locked {
                                IconName::Lock
                            } else {
                                IconName::LockOpen
                            },
                            if locked { "Unlock" } else { "Lock" },
                        )
                        .on_click(move |_, _window, cx| {
                            lock.update(cx, |tree, cx| {
                                tree.edit(cx, |book, symbol| book.set_locked(symbol, id, !locked));
                            });
                        }),
                    )
                    .child(
                        small(
                            format!("tree-settings-{id}"),
                            IconName::Settings2,
                            "Settings",
                        )
                        .on_click(move |_, window, cx| {
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
                        }),
                    )
                    .child(
                        small(format!("tree-delete-{id}"), IconName::Trash, "Delete")
                            .disabled(locked)
                            .on_click(move |_, _window, cx| {
                                delete.update(cx, |tree, cx| {
                                    tree.edit(cx, |book, symbol| book.delete(symbol, id));
                                });
                            }),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(rows)
            .into_any_element()
    }
}
