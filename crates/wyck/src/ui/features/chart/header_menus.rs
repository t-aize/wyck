//! The menus the header of the dashboard shows for the chart being worked on: its type and its
//! indicators.
//!
//! They are built here, next to the chart they change, so each entry runs on the chart itself.
//! The header only owns the buttons and the open or closed state of the cards.

use gpui::{Context, Entity, Window};
use wyck_ui_kit::icon::IconName;

use super::overlay::{KIND_SECTIONS, kind_icon};
use super::study::catalog::{self, Source};
use super::{Chart, indicator_picker};
use crate::app::scripts as indicators;
use wyck_ui_kit::menu::{self, Entry, Item, Menu, Section};

/// How many starred and recent indicators the quick menu lists.
const QUICK: usize = 8;

impl Chart {
    /// The entry of a menu that runs `run` on this chart.
    fn menu_entry(
        &self,
        menu: &Menu,
        entry: Entry,
        run: impl Fn(&mut Chart, &mut Window, &mut Context<Chart>) + 'static,
        cx: &mut Context<Self>,
    ) -> Item {
        let (chart, menu) = (cx.entity(), menu.clone());
        entry
            .on_click(move |window, cx| {
                menu.close(cx);
                chart.update(cx, |this, cx| run(this, window, cx));
            })
            .into()
    }

    /// The types of chart, in sections, the current one checked.
    pub fn kind_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let mut groups = Vec::new();
        for (title, kinds) in KIND_SECTIONS {
            let mut items = Vec::new();
            for kind in kinds.iter().copied() {
                items.push(
                    self.menu_entry(
                        menu,
                        Entry::new(kind.label())
                            .icon(kind_icon(kind))
                            .checked(self.settings.kind == kind),
                        move |this, _, cx| this.set_kind(kind, cx),
                        cx,
                    ),
                );
            }
            groups.push(Section::new(title, items));
        }
        menu::sections(groups)
    }

    /// The starred and the recent indicators, one click to add, and the ways to the rest.
    pub fn indicator_items(&self, menu: &Menu, cx: &mut Context<Self>) -> Vec<Item> {
        let prefs = indicators::prefs(cx);
        let all = catalog::items();
        let mut groups: Vec<Section> = Vec::new();
        let by_key = |key: &String| all.iter().find(|item| item.key() == *key && item.ready);
        let starred: Vec<_> = prefs
            .favorites
            .iter()
            .filter_map(by_key)
            .take(QUICK)
            .collect();
        let recent: Vec<_> = prefs
            .recent
            .iter()
            .filter(|key| !prefs.is_favorite(key))
            .filter_map(by_key)
            .take(QUICK)
            .collect();
        let mut section =
            |title: &'static str, list: Vec<&catalog::Item>, groups: &mut Vec<Section>| {
                let mut items = Vec::new();
                for item in list {
                    let (icon, config, key) = (
                        match item.source {
                            Source::Builtin(_) => IconName::Plus,
                            Source::Script(_) => IconName::CodeXml,
                        },
                        item.config(),
                        item.key(),
                    );
                    items.push(
                        self.menu_entry(
                            menu,
                            Entry::new(item.label.clone())
                                .icon(icon)
                                .hint(item.short.clone()),
                            move |this, _, cx| {
                                this.add_study(config.clone(), cx);
                                let key = key.clone();
                                indicators::update_prefs(cx, |p| p.note_recent(&key));
                            },
                            cx,
                        ),
                    );
                }
                groups.push(Section::new(title, items));
            };
        section("Favorites", starred, &mut groups);
        section("Recent", recent, &mut groups);
        groups.push(Section::new(
            "Add",
            vec![
                self.menu_entry(
                    menu,
                    Entry::new("All indicators...").icon(IconName::ChartSpline),
                    |_, window, cx| indicator_picker::open(cx.entity(), window, cx),
                    cx,
                ),
                self.menu_entry(
                    menu,
                    Entry::new("On this chart...").icon(IconName::ListTree),
                    |_, window, cx| {
                        super::chart_settings_ui::open_indicators(cx.entity(), window, cx)
                    },
                    cx,
                ),
            ],
        ));
        menu::sections(groups)
    }

    /// Opens the picker of indicators for `chart`.
    pub fn open_indicator_picker(chart: &Entity<Chart>, window: &mut Window, cx: &mut gpui::App) {
        indicator_picker::open(chart.clone(), window, cx);
    }
}
