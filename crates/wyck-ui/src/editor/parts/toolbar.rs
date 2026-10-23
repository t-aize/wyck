//! The toolbar of the indicator editor.

use super::{
    AnyElement, Context, EditorEvent, Entry, FontWeight, IconName, IndicatorEditor, Item,
    Placement, SharedString, TEMPLATES, Window, div, icon, layout, px, theme, tokens,
};
use crate::kit::button::Button;
use crate::kit::prelude::Disableable;
use gpui::prelude::*;

impl IndicatorEditor {
    pub(super) fn tool_button(
        id: &'static str,
        icon: IconName,
        label: Option<&'static str>,
        tip: impl Into<SharedString>,
        enabled: bool,
    ) -> Button {
        let button = crate::kit::button::dense(id)
            .icon(icon)
            .tooltip(tip)
            .disabled(!enabled);
        match label {
            Some(label) => button.label(label),
            None => button,
        }
    }

    pub(super) fn toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let has_doc = self.current().is_some();
        let dirty = self.current().is_some_and(|d| d.dirty);
        let new_menu = Self::menu(window, cx, "editor-new-menu");
        let export_menu = Self::menu(window, cx, "editor-export-menu");

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
        let (one, all) = (this.clone(), this);
        let export_items: Vec<Item> = vec![
            Entry::new("This script...")
                .icon(IconName::FileDown)
                .disabled(!has_doc)
                .on_click(move |_, cx| one.update(cx, Self::export_current))
                .into(),
            Entry::new("Every script to a folder...")
                .icon(IconName::FolderDown)
                .on_click(move |_, cx| all.update(cx, |_, cx| Self::export_all(cx)))
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
            .h(px(tokens::bar::toolbar()))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
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
                    .child(icon::tinted(IconName::CodeXml, 16., theme::accent()))
                    .child(
                        div()
                            .text_size(px(tokens::text::emphasis()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child("Indicator editor"),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(
                        Self::tool_button(
                            "editor-new",
                            IconName::FilePlus,
                            Some("New"),
                            "A new script",
                            true,
                        )
                        .on_click(move |_, _window, cx| new_toggle.toggle(cx)),
                    )
                    .children(new_menu.popup(
                        templates,
                        Placement::Below(tokens::height::tiny()),
                        window,
                        cx,
                    )),
            )
            .child(
                Self::tool_button(
                    "editor-save",
                    IconName::Save,
                    Some("Save"),
                    SharedString::from(format!("Save{}", crate::kit::hints::suffix("editor_save"))),
                    has_doc && dirty,
                )
                .on_click(move |_, window, cx| {
                    save.update(cx, |e, cx| e.save(window, cx));
                }),
            )
            .child(
                Self::tool_button(
                    "editor-add",
                    IconName::Play,
                    Some("Add to chart"),
                    SharedString::from(format!(
                        "Save, and put it on the active chart{}",
                        crate::kit::hints::suffix("editor_add_to_chart")
                    )),
                    has_doc,
                )
                .on_click(move |_, window, cx| {
                    add.update(cx, |e, cx| e.add_to_chart(window, cx));
                }),
            )
            .child(layout::divider())
            .child(
                Self::tool_button(
                    "editor-import",
                    IconName::FileUp,
                    Some("Import"),
                    "Add scripts from files or folders",
                    true,
                )
                .on_click(move |_, window, cx| {
                    import.update(cx, |_, cx| Self::import(window, cx));
                }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        Self::tool_button(
                            "editor-export",
                            IconName::FileDown,
                            Some("Export"),
                            "Save scripts as files",
                            true,
                        )
                        .on_click(move |_, _window, cx| export_toggle.toggle(cx)),
                    )
                    .children(export_menu.popup(
                        export_items,
                        Placement::Below(tokens::height::tiny()),
                        window,
                        cx,
                    )),
            )
            .child(
                Self::tool_button(
                    "editor-reveal",
                    IconName::FolderOpen,
                    None,
                    "Show the script in its folder",
                    has_doc,
                )
                .on_click(move |_, _window, cx| {
                    reveal.update(cx, Self::reveal_current);
                }),
            )
            .child(div().flex_1())
            .child(
                Self::tool_button(
                    "editor-reference",
                    IconName::BookOpen,
                    Some("Reference"),
                    SharedString::from(format!(
                        "Every function of the language{}",
                        crate::kit::hints::suffix("editor_reference")
                    )),
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
                Self::tool_button(
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
                Self::tool_button("editor-close", IconName::X, None, "Close the editor", true)
                    .on_click(move |_, _window, cx| {
                        close.update(cx, |_, cx| cx.emit(EditorEvent::Close));
                    }),
            )
            .into_any_element()
    }
}
