//! The list of the scripts of the indicator editor, as a tree of folders, and its file menu.

use super::*;

impl IndicatorEditor {
    pub(super) fn explorer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.filter.read(cx).value().to_lowercase();
        let entries: Vec<_> = registry::all()
            .into_iter()
            .filter(|e| {
                query.is_empty()
                    || e.id.to_lowercase().contains(&query)
                    || e.info.name.to_lowercase().contains(&query)
                    || source_match(&e.source, &query).is_some()
            })
            .collect();
        let mut folders: BTreeMap<String, Vec<_>> = BTreeMap::new();
        for entry in entries {
            folders
                .entry(folder_of(&entry.id).to_owned())
                .or_default()
                .push(entry);
        }
        let total: usize = folders.values().map(Vec::len).sum();
        let file_menu = self.menu(window, cx, "editor-file-menu");

        let mut list = div()
            .id("editor-files")
            .flex()
            .flex_col()
            .gap_0p5()
            .overflow_y_scroll()
            .flex_1()
            .min_h_0();
        if total == 0 {
            let dir = indicators::dir(cx);
            list = list.child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(tokens::text::BODY))
                            .text_color(theme::muted_fg())
                            .child(if query.is_empty() {
                                "No script yet. Make one with New, or drop .rhai files in the folder."
                            } else {
                                "No script matches this search."
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::SMALL))
                            .text_color(theme::muted_fg())
                            .child(dir.display().to_string()),
                    ),
            );
        }
        let mut row_number = 0usize;
        for (folder, entries) in folders {
            let folded = self.folded.contains(&folder) && query.is_empty();
            if !folder.is_empty() {
                let (this, name) = (cx.entity(), folder.clone());
                list = list.child(
                    div()
                        .id(SharedString::from(format!("editor-folder-{folder}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .h(px(tokens::height::COMPACT))
                        .px_2()
                        .mt_1()
                        .cursor_pointer()
                        .rounded_md()
                        .hover(|s| s.bg(theme::surface_hover()))
                        .on_click(move |_, _window, cx| {
                            this.update(cx, |e, cx| {
                                if !e.folded.remove(&name) {
                                    e.folded.insert(name.clone());
                                }
                                cx.notify();
                            });
                        })
                        .child(icon::tinted(
                            if folded {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            },
                            13.,
                            theme::muted_fg(),
                        ))
                        .child(icon::tinted(IconName::Folder, 14., theme::muted_fg()))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(tokens::text::BODY))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme::muted_fg())
                                .truncate()
                                .child(folder.rsplit('/').next().unwrap_or(&folder).to_owned()),
                        )
                        .child(
                            div()
                                .text_size(px(tokens::text::SMALL))
                                .text_color(theme::muted_fg())
                                .child(entries.len().to_string()),
                        ),
                );
            }
            if folded {
                continue;
            }
            for entry in entries {
                row_number += 1;
                list = list.child(
                    self.script_row(
                        &entry,
                        row_number,
                        !folder.is_empty(),
                        (!query.is_empty())
                            .then(|| source_match(&entry.source, &query))
                            .flatten(),
                        &file_menu,
                        cx,
                    ),
                );
            }
        }

        let menu_items = self.file_menu_items(&file_menu, cx);
        div()
            .flex_none()
            .w(px(self.explorer_width))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme::border_hairline())
            .bg(theme::fg_alpha(0.02))
            .child(
                div()
                    .p_2()
                    .child(Input::new(&self.filter).xsmall().prefix(icon::tinted(
                        IconName::Search,
                        14.,
                        theme::muted_fg(),
                    ))),
            )
            .child(list)
            .children(file_menu.popup(menu_items, Placement::Cursor, window, cx))
            .into_any_element()
    }

    pub(super) fn script_row(
        &self,
        entry: &std::sync::Arc<Script>,
        number: usize,
        indented: bool,
        match_at: Option<(usize, usize)>,
        menu: &wyck_ui::menu::Menu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id.clone();
        let (open, right) = (cx.entity(), cx.entity());
        let (open_id, right_id, menu) = (id.clone(), id.clone(), menu.clone());
        let open_doc = self.docs.iter().find(|d| d.id == id);
        let active = self.current().is_some_and(|doc| doc.id == id);
        let unsaved = open_doc.is_some_and(|d| d.dirty);
        let ready = entry.is_ready();
        div()
            .id(("editor-script", number))
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .h(px(tokens::height::CONTROL))
            .pl(px(if indented { 24. } else { 8. }))
            .pr_2()
            .mx_1()
            .rounded_md()
            .cursor_pointer()
            .when(active, |el| el.bg(theme::accent_selected()))
            .when(!active, |el| el.hover(|s| s.bg(theme::surface_hover())))
            .on_click(move |_, window, cx| {
                open.update(cx, |e, cx| {
                    e.open(&open_id, window, cx);
                    if let Some((line, column)) = match_at {
                        e.jump_to(line, column, window, cx);
                    }
                });
            })
            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                right.update(cx, |e, cx| {
                    e.menu_target = Some(right_id.clone());
                    cx.notify();
                });
                menu.open(Some(event.position), cx);
            })
            .child(if ready {
                icon::tinted(
                    IconName::FileCode,
                    14.,
                    if active {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    },
                )
            } else {
                icon::tinted(IconName::TriangleAlert, 14., theme::destructive())
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(tokens::text::BODY))
                    .text_color(if active {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    })
                    .truncate()
                    .child(stem(&id).to_owned()),
            )
            .children(unsaved.then(|| div().size(px(7.)).rounded_full().bg(theme::amber())))
            .into_any_element()
    }

    /// The entries of the menu of a right click on a script.
    pub(super) fn file_menu_items(
        &self,
        menu: &wyck_ui::menu::Menu,
        cx: &mut Context<Self>,
    ) -> Vec<Item> {
        let Some(id) = self.menu_target.clone() else {
            return Vec::new();
        };
        let ready = registry::get(&id).is_some_and(|e| e.is_ready());
        let entity = cx.entity();
        let action = |label: &'static str,
                      icon: IconName,
                      run: fn(
            &mut IndicatorEditor,
            &str,
            &mut Window,
            &mut Context<IndicatorEditor>,
        )| {
            let (entity, id, menu) = (entity.clone(), id.clone(), menu.clone());
            Entry::new(label).icon(icon).on_click(move |window, cx| {
                menu.close(cx);
                entity.update(cx, |e, cx| run(e, &id, window, cx));
            })
        };
        vec![
            Item::Title(stem(&id).to_owned().into()),
            action("Open", IconName::FileCode, |e, id, w, cx| e.open(id, w, cx)).into(),
            action("Add to chart", IconName::Play, |e, id, w, cx| {
                e.open(id, w, cx);
                e.add_to_chart(w, cx);
            })
            .disabled(!ready)
            .into(),
            Item::Separator,
            action("Rename...", IconName::Pencil, |e, id, w, cx| {
                e.ask_rename(id, w, cx)
            })
            .into(),
            action("Duplicate", IconName::Copy, |e, id, w, cx| {
                e.duplicate(id, w, cx)
            })
            .into(),
            action("Show in folder", IconName::FolderOpen, |_, id, _, cx| {
                if let Some(entry) = registry::get(id) {
                    indicators::reveal(cx, &entry.path);
                }
            })
            .into(),
            Item::Separator,
            action("Delete...", IconName::Trash, |e, id, w, cx| {
                e.delete(id, w, cx)
            })
            .danger()
            .into(),
        ]
    }

    /// The card where a name is typed, for a new script or a rename: centered over the code, with
    /// a veil behind it so the rest waits.
    pub(super) fn prompt_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let prompt = self.prompt.as_ref()?;
        let title = match &prompt.ask {
            Ask::New(_) => "Name of the new script",
            Ask::Rename(_) => "New name",
        };
        let description = match &prompt.ask {
            Ask::New(index) => TEMPLATES.get(*index).map(|t| t.description),
            Ask::Rename(_) => None,
        };
        let (ok, cancel) = (cx.entity(), cx.entity());
        let card = div()
            .w(px(380.))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_xl()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(theme::surface())
            .shadow_lg()
            .child(
                div()
                    .text_size(px(tokens::text::TITLE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child(title),
            )
            .children(description.map(|text| {
                div()
                    .text_size(px(tokens::text::SMALL))
                    .text_color(theme::muted_fg())
                    .child(text)
            }))
            .child(Input::new(&prompt.input).small())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_1()
                    .child(
                        Button::new("editor-prompt-cancel")
                            .ghost()
                            .xsmall()
                            .compact()
                            .label("Cancel")
                            .cursor_pointer()
                            .on_click(move |_, _window, cx| {
                                cancel.update(cx, |e, cx| e.cancel_prompt(cx));
                            }),
                    )
                    .child(
                        Button::new("editor-prompt-ok")
                            .primary()
                            .xsmall()
                            .compact()
                            .label("OK")
                            .cursor_pointer()
                            .on_click(move |_, window, cx| {
                                ok.update(cx, |e, cx| e.commit_prompt(window, cx));
                            }),
                    ),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme::veil())
                .child(card)
                .into_any_element(),
        )
    }
}
