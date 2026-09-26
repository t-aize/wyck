//! What the editor looks like: the toolbar, the list of scripts, the tabs, the console, the
//! reference and the status bar.

use std::collections::BTreeMap;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Disableable, Sizable};

use super::{
    AddToChart, Ask, CONTEXT, CloseTab, ConsoleTab, EditorEvent, IndicatorEditor, NextProblem,
    NextTab, PreviousProblem, PreviousTab, ResizeDrag, ResizeSide, SaveScript, ToggleReference,
};
use crate::app::chart::drawing::model::Tool;
use crate::app::chart::study::custom::docs::{self, Group};
use crate::app::chart::study::custom::library::{Entry as Script, registry};
use crate::app::chart::study::custom::templates::TEMPLATES;
use crate::app::chart::study::custom::{Problem, Severity};
use crate::app::connection::ui::icon_colored;
use crate::app::indicators;
use crate::app::menu::{Entry, Item, Placement};
use crate::app::{theme, widgets};

/// The width of the list of scripts and of the reference.
const FONT_BODY: f32 = 12.0;
const FONT_META: f32 = 11.0;
const FONT_TITLE: f32 = 13.0;

/// A fixed width font that the system has.
fn mono() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "DejaVu Sans Mono"
    }
}

/// The name of a script in the tree: the last part of its id.
fn stem(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// The folder of a script: its id without the last part.
fn folder_of(id: &str) -> &str {
    id.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn source_match(source: &str, query: &str) -> Option<(usize, usize)> {
    source.lines().enumerate().find_map(|(line, text)| {
        let lowered = text.to_lowercase();
        let column = lowered.find(query)?;
        Some((line + 1, lowered[..column].chars().count() + 1))
    })
}

impl IndicatorEditor {
    fn tool_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: Option<&'static str>,
        tip: &'static str,
        enabled: bool,
    ) -> Button {
        let button = Button::new(id)
            .ghost()
            .xsmall()
            .compact()
            .icon(icon)
            .tooltip(tip)
            .cursor_pointer()
            .disabled(!enabled);
        match label {
            Some(label) => button.label(label),
            None => button,
        }
    }

    fn separator() -> gpui::Div {
        div()
            .flex_none()
            .w(px(1.))
            .h(px(18.))
            .mx_1()
            .bg(theme::border_hairline())
    }

    // ---- the toolbar ----

    fn toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let has_doc = self.current().is_some();
        let dirty = self.current().is_some_and(|d| d.dirty);
        let new_menu = self.menu(window, cx, "editor-new-menu");
        let export_menu = self.menu(window, cx, "editor-export-menu");

        let mut templates: Vec<Item> = vec![Item::Title("New script from".into())];
        for (index, template) in TEMPLATES.iter().enumerate() {
            let this = cx.entity();
            let close = new_menu.clone();
            templates.push(
                Entry::new(template.name)
                    .icon(IconName::FilePlus)
                    .hint("blank")
                    .on_click(move |window, cx| {
                        close.close(cx);
                        this.update(cx, |e, cx| e.ask_new(index, window, cx));
                    })
                    .into(),
            );
        }

        let this = cx.entity();
        let (one, all) = (this.clone(), this.clone());
        let export_items: Vec<Item> = vec![
            Entry::new("This script...")
                .icon(IconName::FileDown)
                .disabled(!has_doc)
                .on_click(move |_, cx| one.update(cx, |e, cx| e.export_current(cx)))
                .into(),
            Entry::new("Every script to a folder...")
                .icon(IconName::FolderDown)
                .on_click(move |_, cx| all.update(cx, |e, cx| e.export_all(cx)))
                .into(),
        ];

        let (save, add, import, reveal, reference, maximize, close) = (
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
        );
        let (new_toggle, export_toggle) = (new_menu.clone(), export_menu.clone());
        let maximized = self.maximized;
        let reference_open = self.reference_open;
        div()
            .flex_none()
            .h(px(40.))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .border_b_1()
            .border_color(theme::border_hairline())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .mr_1()
                    .child(icon_colored(IconName::CodeXml, 16., theme::accent()))
                    .child(
                        div()
                            .text_size(px(FONT_TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child("Indicator editor"),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(
                        self.tool_button(
                            "editor-new",
                            IconName::FilePlus,
                            Some("New"),
                            "A new script",
                            true,
                        )
                        .on_click(move |_, _window, cx| new_toggle.toggle(cx)),
                    )
                    .children(new_menu.popup(templates, Placement::Below(30.), window, cx)),
            )
            .child(
                self.tool_button(
                    "editor-save",
                    IconName::Save,
                    Some("Save"),
                    "Save (Ctrl+S)",
                    has_doc && dirty,
                )
                .on_click(move |_, window, cx| {
                    save.update(cx, |e, cx| e.save(window, cx));
                }),
            )
            .child(
                self.tool_button(
                    "editor-add",
                    IconName::Play,
                    Some("Add to chart"),
                    "Save, and put it on the active chart (Ctrl+Enter or F5)",
                    has_doc,
                )
                .on_click(move |_, window, cx| {
                    add.update(cx, |e, cx| e.add_to_chart(window, cx));
                }),
            )
            .child(Self::separator())
            .child(
                self.tool_button(
                    "editor-import",
                    IconName::FileUp,
                    Some("Import"),
                    "Add scripts from files or folders",
                    true,
                )
                .on_click(move |_, window, cx| {
                    import.update(cx, |e, cx| e.import(window, cx));
                }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        self.tool_button(
                            "editor-export",
                            IconName::FileDown,
                            Some("Export"),
                            "Save scripts as files",
                            true,
                        )
                        .on_click(move |_, _window, cx| export_toggle.toggle(cx)),
                    )
                    .children(export_menu.popup(export_items, Placement::Below(30.), window, cx)),
            )
            .child(
                self.tool_button(
                    "editor-reveal",
                    IconName::FolderOpen,
                    None,
                    "Show the script in its folder",
                    has_doc,
                )
                .on_click(move |_, _window, cx| {
                    reveal.update(cx, |e, cx| e.reveal_current(cx));
                }),
            )
            .child(div().flex_1())
            .child(
                self.tool_button(
                    "editor-reference",
                    IconName::BookOpen,
                    Some("Reference"),
                    "Every function of the language (Ctrl+Alt+R)",
                    true,
                )
                .toggled(reference_open)
                .on_click(move |_, _window, cx| {
                    reference.update(cx, |e, cx| {
                        e.reference_open = !e.reference_open;
                        cx.notify();
                    });
                }),
            )
            .child(
                self.tool_button(
                    "editor-maximize",
                    if maximized {
                        IconName::Minimize2
                    } else {
                        IconName::Maximize2
                    },
                    None,
                    if maximized {
                        "Back to the size it had"
                    } else {
                        "As tall as the window"
                    },
                    true,
                )
                .on_click(move |_, _window, cx| {
                    maximize.update(cx, |_, cx| cx.emit(EditorEvent::ToggleMaximize));
                }),
            )
            .child(
                self.tool_button("editor-close", IconName::X, None, "Close the editor", true)
                    .on_click(move |_, _window, cx| {
                        close.update(cx, |_, cx| cx.emit(EditorEvent::Close));
                    }),
            )
            .into_any_element()
    }

    // ---- the list of scripts ----

    fn explorer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                            .text_size(px(FONT_BODY))
                            .text_color(theme::muted_fg())
                            .child(if query.is_empty() {
                                "No script yet. Make one with New, or drop .rhai files in the folder."
                            } else {
                                "No script matches this search."
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(FONT_META))
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
                        .h(px(26.))
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
                        .child(icon_colored(
                            if folded {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            },
                            13.,
                            theme::muted_fg(),
                        ))
                        .child(icon_colored(IconName::Folder, 14., theme::muted_fg()))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(FONT_BODY))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme::muted_fg())
                                .truncate()
                                .child(folder.rsplit('/').next().unwrap_or(&folder).to_owned()),
                        )
                        .child(
                            div()
                                .text_size(px(FONT_META))
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
        let prompt = self.prompt_row(cx);
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
                    .child(Input::new(&self.filter).xsmall().prefix(icon_colored(
                        IconName::Search,
                        14.,
                        theme::muted_fg(),
                    ))),
            )
            .children(prompt)
            .child(list)
            .children(file_menu.popup(menu_items, Placement::Cursor, window, cx))
            .into_any_element()
    }

    fn script_row(
        &self,
        entry: &std::sync::Arc<Script>,
        number: usize,
        indented: bool,
        match_at: Option<(usize, usize)>,
        menu: &crate::app::menu::Menu,
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
            .h(px(28.))
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
                icon_colored(
                    IconName::FileCode,
                    14.,
                    if active {
                        theme::fg()
                    } else {
                        theme::muted_fg()
                    },
                )
            } else {
                icon_colored(IconName::TriangleAlert, 14., theme::destructive())
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(FONT_BODY))
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
    fn file_menu_items(&self, menu: &crate::app::menu::Menu, cx: &mut Context<Self>) -> Vec<Item> {
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

    /// The row where a name is typed, for a new script or a rename.
    fn prompt_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
        Some(
            div()
                .mx_2()
                .mb_2()
                .p_2()
                .flex()
                .flex_col()
                .gap_1p5()
                .rounded_lg()
                .border_1()
                .border_color(theme::accent())
                .bg(theme::surface())
                .child(
                    div()
                        .text_size(px(FONT_META))
                        .text_color(theme::muted_fg())
                        .child(title),
                )
                .children(description.map(|text| {
                    div()
                        .text_size(px(FONT_META))
                        .text_color(theme::muted_fg())
                        .child(text)
                }))
                .child(Input::new(&prompt.input).xsmall())
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
                )
                .into_any_element(),
        )
    }

    // ---- the tabs and the editor ----

    fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex_none()
            .h(px(34.))
            .flex()
            .flex_row()
            .items_end()
            .gap_0p5()
            .px_2()
            .border_b_1()
            .border_color(theme::border_hairline())
            .overflow_x_scrollbar();
        for (index, doc) in self.docs.iter().enumerate() {
            let active = index == self.active;
            let (pick, close) = (cx.entity(), cx.entity());
            let broken = doc.problems.iter().any(|p| p.severity == Severity::Error);
            row = row.child(
                div()
                    .id(("editor-tab", index))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .h(px(30.))
                    .pl_2p5()
                    .pr_1()
                    .rounded_t_md()
                    .cursor_pointer()
                    .border_1()
                    .border_b_0()
                    .border_color(if active {
                        theme::border_subtle()
                    } else {
                        gpui::rgba(0)
                    })
                    .when(active, |el| el.bg(theme::bg()))
                    .when(!active, |el| el.hover(|s| s.bg(theme::surface_hover())))
                    .on_click(move |_, window, cx| {
                        pick.update(cx, |e, cx| e.activate(index, window, cx));
                    })
                    .child(icon_colored(
                        if broken {
                            IconName::TriangleAlert
                        } else {
                            IconName::FileCode
                        },
                        13.,
                        if broken {
                            theme::destructive()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(FONT_BODY))
                            .text_color(if active {
                                theme::fg()
                            } else {
                                theme::muted_fg()
                            })
                            .when(doc.gone, |el| el.line_through())
                            .child(stem(&doc.id).to_owned()),
                    )
                    .child(
                        div()
                            .id(("editor-tab-close", index))
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .hover(|s| s.bg(theme::surface_pressed()))
                            .on_click(move |_, window, cx| {
                                close.update(cx, |e, cx| e.close_tab(index, window, cx));
                            })
                            .child(if doc.dirty {
                                div()
                                    .size(px(8.))
                                    .rounded_full()
                                    .bg(theme::amber())
                                    .into_any_element()
                            } else {
                                icon_colored(IconName::X, 12., theme::muted_fg()).into_any_element()
                            }),
                    ),
            );
        }
        row.into_any_element()
    }

    /// A line over the editor when the file changed under it, or is gone.
    fn banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.current()?;
        let (text, actions): (&str, bool) = if doc.conflict {
            (
                "This file changed on disk, and this tab has changes of its own.",
                true,
            )
        } else if doc.gone {
            (
                "This file was deleted or moved. Saving brings it back.",
                false,
            )
        } else {
            return None;
        };
        let (reload, keep) = (cx.entity(), cx.entity());
        Some(
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(34.))
                .px_3()
                .bg(theme::amber_bg())
                .border_b_1()
                .border_color(theme::border_hairline())
                .child(icon_colored(IconName::Info, 14., theme::amber()))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(FONT_BODY))
                        .text_color(theme::fg())
                        .child(text.to_owned()),
                )
                .children(actions.then(|| {
                    Button::new("editor-reload")
                        .ghost()
                        .xsmall()
                        .compact()
                        .label("Load the file")
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            reload.update(cx, |e, cx| e.reload_from_disk(window, cx));
                        })
                }))
                .children(actions.then(|| {
                    Button::new("editor-keep")
                        .ghost()
                        .xsmall()
                        .compact()
                        .label("Keep mine")
                        .cursor_pointer()
                        .on_click(move |_, _window, cx| {
                            keep.update(cx, |e, cx| e.keep_mine(cx));
                        })
                }))
                .into_any_element(),
        )
    }

    fn empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let (new, folder) = (cx.entity(), cx.entity());
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(icon_colored(IconName::CodeXml, 32., theme::muted_fg()))
            .child(
                div()
                    .text_size(px(FONT_TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child("Write your own indicators"),
            )
            .child(
                div()
                    .max_w(px(420.))
                    .text_center()
                    .text_size(px(FONT_BODY))
                    .text_color(theme::muted_fg())
                    .child("Pick a script on the left, or start from a template. A saved script shows up in the list of indicators, on every chart."),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        Button::new("editor-empty-new")
                            .primary()
                            .xsmall()
                            .compact()
                            .icon(IconName::FilePlus)
                            .label("New script")
                            .cursor_pointer()
                            .on_click(move |_, window, cx| {
                                new.update(cx, |e, cx| e.ask_new(0, window, cx));
                            }),
                    )
                    .child(
                        Button::new("editor-empty-folder")
                            .ghost()
                            .xsmall()
                            .compact()
                            .icon(IconName::FolderOpen)
                            .label("Open the folder")
                            .cursor_pointer()
                            .on_click(move |_, _window, cx| {
                                folder.update(cx, |_, cx| indicators::open_folder(cx));
                            }),
                    ),
            )
            .into_any_element()
    }

    // ---- the console ----

    fn console(&self, cx: &mut Context<Self>) -> AnyElement {
        let problems: &[Problem] = self.current().map_or(&[], |d| d.problems.as_slice());
        let (a, b, previous, next) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
        let tab = |id: &'static str, label: &'static str, count: Option<usize>, chosen: bool| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(26.))
                .px_2p5()
                .cursor_pointer()
                .text_size(px(FONT_BODY))
                .border_b_2()
                .border_color(if chosen {
                    theme::accent()
                } else {
                    gpui::rgba(0)
                })
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .child(label)
                .children(count.filter(|c| *c > 0).map(|c| {
                    div()
                        .px_1p5()
                        .rounded_full()
                        .bg(theme::destructive_bg())
                        .text_size(px(FONT_META))
                        .text_color(theme::destructive())
                        .child(c.to_string())
                }))
        };
        let body: AnyElement = match self.console {
            ConsoleTab::Problems => self.problems_list(problems, cx),
            ConsoleTab::Output => self.output_list(cx),
        };
        div()
            .flex_none()
            .h(px(self.console_height))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::border_hairline())
            .child(
                div()
                    .id("editor-console-resize")
                    .flex_none()
                    .h(px(5.))
                    .w_full()
                    .cursor_row_resize()
                    .hover(|s| s.bg(theme::accent_alpha(0.35)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.resize = Some(ResizeDrag {
                                side: ResizeSide::Console,
                                start: f32::from(event.position.y),
                                size: this.console_height,
                            });
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .px_1()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        tab(
                            "console-problems",
                            "Problems",
                            Some(problems.len()),
                            self.console == ConsoleTab::Problems,
                        )
                        .on_click(move |_, _window, cx| {
                            a.update(cx, |e, cx| {
                                e.console = ConsoleTab::Problems;
                                cx.notify();
                            });
                        }),
                    )
                    .child(
                        tab(
                            "console-output",
                            "Output",
                            None,
                            self.console == ConsoleTab::Output,
                        )
                        .on_click(move |_, _window, cx| {
                            b.update(cx, |e, cx| {
                                e.console = ConsoleTab::Output;
                                cx.notify();
                            });
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        self.tool_button(
                            "problem-prev",
                            IconName::ChevronUp,
                            None,
                            "Previous problem (Shift+F8)",
                            !problems.is_empty(),
                        )
                        .on_click(move |_, window, cx| {
                            previous.update(cx, |e, cx| e.step_problem(false, window, cx));
                        }),
                    )
                    .child(
                        self.tool_button(
                            "problem-next",
                            IconName::ChevronDown,
                            None,
                            "Next problem (F8)",
                            !problems.is_empty(),
                        )
                        .on_click(move |_, window, cx| {
                            next.update(cx, |e, cx| e.step_problem(true, window, cx));
                        }),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    fn problems_list(&self, problems: &[Problem], cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("console-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1p5()
            .flex()
            .flex_col();
        if problems.is_empty() {
            let text = if self.current().is_some() {
                "No problem found in this script."
            } else {
                "Open a script to see its problems."
            };
            return list
                .child(
                    div()
                        .p_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(FONT_BODY))
                        .text_color(theme::muted_fg())
                        .child(icon_colored(IconName::Check, 14., theme::emerald()))
                        .child(text),
                )
                .into_any_element();
        }
        for (index, problem) in problems.iter().enumerate() {
            let this = cx.entity();
            let (line, column) = (problem.line, problem.column);
            let error = problem.severity == Severity::Error;
            list = list.child(
                div()
                    .id(("console-problem", index))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, window, cx| {
                        this.update(cx, |e, cx| e.jump_to(line, column, window, cx));
                    })
                    .child(icon_colored(
                        if error {
                            IconName::CircleAlert
                        } else {
                            IconName::TriangleAlert
                        },
                        14.,
                        if error {
                            theme::destructive()
                        } else {
                            theme::amber()
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(FONT_BODY))
                            .text_color(theme::fg())
                            .child(problem.message.clone()),
                    )
                    .children((line > 0).then(|| {
                        div()
                            .flex_none()
                            .text_size(px(FONT_META))
                            .text_color(theme::muted_fg())
                            .child(format!("line {line}, column {column}"))
                    })),
            );
        }
        list.into_any_element()
    }

    fn output_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("console-output-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_0p5();
        let Some(doc) = self.current() else {
            return list
                .child(note("Open a script to see what it prints."))
                .into_any_element();
        };
        let chart = self.multi.read(cx).active_chart().clone();
        match chart.read(cx).script_report(&doc.id) {
            None => {
                list = list.child(note(
                    "This script is not on the active chart. Use Add to chart (F5) to see it run, and what it prints here.",
                ));
            }
            Some(report) => {
                let mut line = format!("On the active chart: {} bars", report.bars);
                if let Some(elapsed) = report.elapsed {
                    line.push_str(&format!(
                        ", computed in {:.1} ms",
                        elapsed.as_secs_f64() * 1000.0
                    ));
                }
                if report.running {
                    line.push_str(", running...");
                }
                list = list.child(
                    div()
                        .text_size(px(FONT_META))
                        .text_color(theme::muted_fg())
                        .child(line),
                );
                if let Some(problem) = report.problems.first() {
                    list = list.child(
                        div()
                            .text_size(px(FONT_BODY))
                            .text_color(theme::destructive())
                            .child(problem.message.clone()),
                    );
                }
                if report.log.is_empty() && report.problems.is_empty() {
                    list = list.child(note(
                        "It printed nothing. print(x) in the script writes here.",
                    ));
                }
                for text in report.log {
                    list = list.child(
                        div()
                            .font_family(mono())
                            .text_size(px(FONT_BODY))
                            .text_color(theme::fg())
                            .child(text),
                    );
                }
            }
        }
        list.into_any_element()
    }

    // ---- the reference ----

    fn reference(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.reference_filter.read(cx).value().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let matches = |text: &str| {
            let text = text.to_lowercase();
            words.iter().all(|w| text.contains(w))
        };
        let mut list = div()
            .id("reference-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_2()
            .flex()
            .flex_col();
        let mut row_number = 0usize;
        // The names a script starts with.
        let globals: Vec<_> = docs::GLOBALS
            .iter()
            .filter(|g| matches(&format!("{} {}", g.name, g.summary)))
            .collect();
        if !globals.is_empty() {
            list = list.child(section_title("Names you start with"));
            for global in globals {
                row_number += 1;
                let this = cx.entity();
                let name = global.name;
                list = list.child(reference_row(
                    row_number,
                    name,
                    global.summary,
                    move |window, cx| this.update(cx, |e, cx| e.insert(name, window, cx)),
                ));
            }
        }
        for group in Group::ALL {
            let items: Vec<_> = docs::FUNCTIONS
                .iter()
                .filter(|d| d.group == group)
                .filter(|d| matches(&format!("{} {} {}", d.name, d.signature, d.summary)))
                .collect();
            if items.is_empty() {
                continue;
            }
            list = list.child(section_title(group.label()));
            for doc in items {
                row_number += 1;
                let this = cx.entity();
                let example = doc.example;
                list = list.child(reference_row(
                    row_number,
                    doc.signature,
                    doc.summary,
                    move |window, cx| this.update(cx, |e, cx| e.insert(example, window, cx)),
                ));
            }
        }
        let tools: Vec<_> = Tool::ALL
            .iter()
            .filter_map(|tool| {
                let name = serde_json::to_value(tool).ok()?.as_str()?.to_owned();
                matches(&format!("{} {}", name, tool.label())).then_some((name, tool.label()))
            })
            .collect();
        if !tools.is_empty() {
            list = list.child(section_title("Drawing tool names"));
            for (name, label) in tools {
                row_number += 1;
                let this = cx.entity();
                let snippet = format!("\"{name}\"");
                list = list.child(reference_row(row_number, name, label, move |window, cx| {
                    this.update(cx, |e, cx| e.insert(&snippet, window, cx))
                }));
            }
        }
        div()
            .flex_none()
            .w(px(self.reference_width))
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(theme::border_hairline())
            .bg(theme::fg_alpha(0.02))
            .child(
                div()
                    .flex_none()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .px_1()
                            .text_size(px(FONT_META))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::muted_fg())
                            .child("REFERENCE"),
                    )
                    .child(
                        Input::new(&self.reference_filter)
                            .xsmall()
                            .prefix(icon_colored(IconName::Search, 14., theme::muted_fg())),
                    ),
            )
            .child(list)
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme::border_hairline())
                    .text_size(px(FONT_META))
                    .text_color(theme::muted_fg())
                    .child("Click a line to write its example where the cursor is."),
            )
            .into_any_element()
    }

    // ---- the status bar ----

    fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let position = self
            .cursor(cx)
            .map(|(line, column)| format!("Ln {line}, Col {column}"));
        let doc = self.current();
        let errors = doc.map_or(0, |d| {
            d.problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count()
        });
        div()
            .flex_none()
            .h(px(24.))
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(theme::border_hairline())
            .text_size(px(FONT_META))
            .text_color(theme::muted_fg())
            .child("Rhai")
            .children(position)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(match &self.notice {
                        Some(n) if !n.ok => theme::destructive(),
                        Some(_) => theme::emerald(),
                        None => theme::muted_fg(),
                    })
                    .child(
                        self.notice
                            .as_ref()
                            .map_or_else(String::new, |n| n.text.clone()),
                    ),
            )
            .children(doc.map(|d| {
                if d.dirty {
                    div().text_color(theme::amber()).child("Not saved")
                } else if errors > 0 {
                    div()
                        .text_color(theme::destructive())
                        .child(format!("{errors} problem(s)"))
                } else {
                    div().text_color(theme::emerald()).child("Saved")
                }
            }))
            .into_any_element()
    }
}

fn note(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(FONT_BODY))
        .text_color(theme::muted_fg())
        .child(text)
}

fn section_title(title: &'static str) -> gpui::Div {
    div()
        .px_3()
        .pt_2p5()
        .pb_1()
        .text_size(px(FONT_META))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::muted_fg())
        .child(title.to_uppercase())
}

fn reference_row(
    number: usize,
    title: impl Into<SharedString>,
    summary: impl Into<SharedString>,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(("reference-row", number))
        .mx_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_hover()))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .font_family(mono())
                .text_size(px(FONT_BODY))
                .text_color(theme::accent())
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(FONT_META))
                .text_color(theme::muted_fg())
                .child(summary.into()),
        )
        .into_any_element()
}

impl Render for IndicatorEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = self.toolbar(window, cx);
        let explorer = self.explorer(window, cx);
        let tabs = self.tabs(cx);
        let banner = self.banner(cx);
        let center = match self.current() {
            Some(doc) => div()
                .flex_1()
                .min_h_0()
                .child(
                    Editor::new(&doc.state)
                        .h_full()
                        .bordered(false)
                        .font_family(mono())
                        .text_size(px(FONT_BODY)),
                )
                .into_any_element(),
            None => self.empty(cx),
        };
        let console = self.console(cx);
        let reference = self.reference_open.then(|| self.reference(cx));
        let status = self.status_bar(cx);
        let _ = widgets::child_id;
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .when(self.resize.is_some(), |el| {
                el.on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    this.drag_resize(event, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_resize(cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_resize(cx)),
                )
            })
            .on_action(cx.listener(|this, _: &SaveScript, window, cx| this.save(window, cx)))
            .on_action(
                cx.listener(|this, _: &AddToChart, window, cx| this.add_to_chart(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let at = this.active;
                this.close_tab(at, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleReference, _window, cx| {
                this.reference_open = !this.reference_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextProblem, window, cx| {
                this.step_problem(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousProblem, window, cx| {
                this.step_problem(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.cycle_tab(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
                this.cycle_tab(false, window, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .font_family(crate::app::appearance::font(cx))
            .text_size(px(FONT_BODY))
            .text_color(theme::fg())
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(explorer)
                    .child(
                        div()
                            .id("editor-explorer-resize")
                            .flex_none()
                            .w(px(5.))
                            .h_full()
                            .cursor_col_resize()
                            .hover(|s| s.bg(theme::accent_alpha(0.35)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.resize = Some(ResizeDrag {
                                        side: ResizeSide::Explorer,
                                        start: f32::from(event.position.x),
                                        size: this.explorer_width,
                                    });
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(tabs)
                            .children(banner)
                            .child(center)
                            .child(console),
                    )
                    .children(reference.map(|reference| {
                        div()
                            .flex()
                            .flex_row()
                            .child(
                                div()
                                    .id("editor-reference-resize")
                                    .flex_none()
                                    .w(px(5.))
                                    .h_full()
                                    .cursor_col_resize()
                                    .hover(|s| s.bg(theme::accent_alpha(0.35)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                            this.resize = Some(ResizeDrag {
                                                side: ResizeSide::Reference,
                                                start: f32::from(event.position.x),
                                                size: this.reference_width,
                                            });
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(reference)
                    })),
            )
            .child(status)
    }
}
