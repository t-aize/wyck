//! The rest of the style bar: extras, positions, the look of the text and the actions.

use super::style_bar::BarState;

use crate::domain::drawings::look::Cap;
use crate::domain::drawings::model::tool::Tool;
use crate::ui::features::chart::DrawingCommand;
use crate::ui::features::multichart::MultiChart;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::Disableable;
use crate::ui::kit::{layout, menu as popup, theme, tokens};
use gpui::prelude::*;
use gpui::{Context, Window, div, px};

impl MultiChart {
    #[allow(unused_variables)]
    #[inline(never)]
    pub(super) fn bar_extras(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.has_extend() {
            let (left, right) = (style.extend_left, style.extend_right);
            bar = bar
                .child(
                    crate::ui::kit::button::quiet("draw-extend-left")
                        .icon(IconName::ArrowLeftToLine)
                        .tooltip("Extend to the left")
                        .toggled(left)
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.extend_left = !s.extend_left)
                            });
                        })),
                )
                .child(
                    crate::ui::kit::button::quiet("draw-extend-right")
                        .icon(IconName::ArrowRightToLine)
                        .tooltip("Extend to the right")
                        .toggled(right)
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.extend_right = !s.extend_right)
                            });
                        })),
                );
        }
        if tool.has_end_cap() {
            let arrow = style.caps.end == Cap::Arrow;
            bar = bar.child(
                crate::ui::kit::button::quiet("draw-end-arrow")
                    .icon(IconName::MoveRight)
                    .tooltip("Arrow at the end")
                    .toggled(arrow)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.edit_book(cx, |book, symbol| {
                            book.edit_style(symbol, |s| {
                                s.caps.end = if s.caps.end == Cap::Arrow {
                                    Cap::None
                                } else {
                                    Cap::Arrow
                                };
                            })
                        });
                    })),
            );
        }
        if tool.has_width() || has_fill || tool.has_extend() || tool.has_end_cap() {
            bar = bar.child(layout::divider());
        }
        if tool.has_text() || tool.takes_label() {
            let size = style.text_size;
            let bold = style.bold;
            bar = bar
                .child(
                    crate::ui::kit::button::quiet("draw-text-smaller")
                        .icon(IconName::Minus)
                        .tooltip("Smaller text")
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| {
                                    s.text_size = (s.text_size - 2.0).clamp(8.0, 48.0);
                                })
                            });
                        })),
                )
                .child(
                    div()
                        .min_w(px(crate::ui::kit::tokens::height::tiny()))
                        .flex()
                        .justify_center()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(format!("{}", size.round() as u32)),
                )
                .child(
                    crate::ui::kit::button::quiet("draw-text-bigger")
                        .icon(IconName::Plus)
                        .tooltip("Bigger text")
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| {
                                    s.text_size = (s.text_size + 2.0).clamp(8.0, 48.0);
                                })
                            });
                        })),
                )
                .child(
                    crate::ui::kit::button::quiet("draw-text-bold")
                        .icon(IconName::Bold)
                        .tooltip("Bold")
                        .toggled(bold)
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.edit_book(cx, |book, symbol| {
                                book.edit_style(symbol, |s| s.bold = !s.bold)
                            });
                        })),
                )
                .child(layout::divider());
        }
        if tool.has_text() {
            bar = bar
                .child(
                    div()
                        .w(px(tokens::field::text()))
                        .child(self.text_input.clone()),
                )
                .child(layout::divider());
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    pub(super) fn bar_position(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if tool.is_position() {
            bar = bar
                .child(
                    crate::ui::kit::button::accent("draw-trade")
                        .icon(IconName::ArrowLeftRight)
                        .label(if tool == Tool::LongPosition {
                            "Buy"
                        } else {
                            "Sell"
                        })
                        .tooltip("Open the order ticket with this entry, stop and target")
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.trade_drawing(id, cx);
                        })),
                )
                .child(
                    crate::ui::kit::button::quiet("draw-flip")
                        .icon(IconName::ArrowUpDown)
                        .tooltip(if tool == Tool::LongPosition {
                            "Flip to a short position"
                        } else {
                            "Flip to a long position"
                        })
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.drawing_command(id, DrawingCommand::Flip, cx);
                        })),
                )
                .child(layout::divider());
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    pub(super) fn bar_look(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        if !locked {
            let menu = popup::Menu::new(("draw-style-menu", 0usize), window, cx);
            let items: Vec<popup::Item> = if menu.is_open(cx) {
                let this = cx.entity();
                let mut items = vec![popup::Item::Title("Look".into())];
                let entry =
                    |label: &str, icon: IconName| popup::Entry::new(label.to_owned()).icon(icon);
                {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Use as the default for this tool", IconName::Save)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, symbol| {
                                        book.save_template(symbol, id)
                                    });
                                });
                            })
                            .into(),
                    );
                }
                {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Back to the default look", IconName::Undo2)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, symbol| {
                                        book.apply_starting_style(symbol, tool)
                                    });
                                });
                            })
                            .into(),
                    );
                }
                if has_default {
                    let (this, menu) = (this.clone(), menu.clone());
                    items.push(
                        entry("Forget the saved default", IconName::StarOff)
                            .on_click(move |_window, cx| {
                                menu.close(cx);
                                this.update(cx, |this, cx| {
                                    this.edit_book(cx, |book, _| book.forget_template(tool));
                                });
                            })
                            .into(),
                    );
                }
                if !template_names.is_empty() {
                    items.push(popup::Item::Separator);
                    items.push(popup::Item::Title("Saved looks".into()));
                    for name in template_names {
                        let (this, menu, applied) = (this.clone(), menu.clone(), name.clone());
                        items.push(
                            entry(&name, IconName::LayoutTemplate)
                                .on_click(move |_window, cx| {
                                    menu.close(cx);
                                    let applied = applied.clone();
                                    this.update(cx, |this, cx| {
                                        this.edit_book(cx, |book, symbol| {
                                            book.apply_named_template(symbol, tool, &applied)
                                        });
                                    });
                                })
                                .into(),
                        );
                    }
                }
                items
            } else {
                Vec::new()
            };
            let toggle = menu.clone();
            bar = bar.child(
                div()
                    .relative()
                    .child(
                        crate::ui::kit::button::quiet("draw-look")
                            .icon(IconName::Palette)
                            .tooltip("Save or reuse this look")
                            .on_click(move |_event, _window, cx| toggle.toggle(cx)),
                    )
                    .children(menu.popup(
                        items,
                        popup::Placement::Below(tokens::height::compact()),
                        window,
                        cx,
                    )),
            );
        }
        bar
    }

    #[allow(unused_variables)]
    #[inline(never)]
    pub(super) fn bar_actions(
        &self,
        mut bar: gpui::Div,
        st: &BarState,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let (style, tool, locked, hidden, id) = (&st.style, st.tool, st.locked, st.hidden, st.id);
        let (has_fill, has_dash) = (st.has_fill, st.has_dash);
        let (has_default, template_names) = (st.has_default, st.template_names.clone());
        bar = bar
            .child(
                crate::ui::kit::button::quiet("draw-settings")
                    .icon(IconName::Settings2)
                    .tooltip("Settings (double click)")
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        let chart = this.active_chart().clone();
                        crate::ui::features::chart::open_drawing_settings(&chart, id, window, cx);
                    })),
            )
            .child(
                crate::ui::kit::button::quiet("draw-hide")
                    .icon(if hidden {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    })
                    .tooltip(if hidden { "Show" } else { "Hide" })
                    .toggled(hidden)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.drawing_command(id, DrawingCommand::Hidden(!hidden), cx);
                    })),
            )
            .child(
                crate::ui::kit::button::quiet("draw-lock")
                    .icon(if locked {
                        IconName::Lock
                    } else {
                        IconName::LockOpen
                    })
                    .tooltip(if locked { "Unlock" } else { "Lock" })
                    .toggled(locked)
                    .on_click(
                        cx.listener(|this, _event, _window, cx| this.toggle_drawing_lock(cx)),
                    ),
            )
            .child(
                crate::ui::kit::button::quiet("draw-copy")
                    .icon(IconName::Copy)
                    .tooltip(crate::ui::kit::shortcut::text("Duplicate (Ctrl+D)"))
                    .on_click(cx.listener(|this, _event, _window, cx| this.duplicate_drawing(cx))),
            )
            .child(
                crate::ui::kit::button::quiet("draw-delete")
                    .icon(IconName::Trash)
                    .tooltip("Delete (Del)")
                    .disabled(locked)
                    .when(locked, |button| button.cursor_not_allowed())
                    .on_click(cx.listener(|this, _event, _window, cx| this.delete_drawing(cx))),
            );
        bar
    }
}
