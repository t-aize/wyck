//! What the rest of the app asks of a chart: its symbol, its timeframe, its settings, its studies and its lines.

use super::events::ChartEvent;
use super::settings;
use super::{
    Chart, ChartKind, ChartLine, ChartSettings, StudyConfig, Symbol, Timeframe, View, footprint,
    view,
};

use crate::app::market_data::now_ms;
use gpui::{Context, SharedString};

impl Chart {
    pub fn timeframe(&self) -> Timeframe {
        self.timeframe
    }

    pub fn settings(&self) -> &ChartSettings {
        &self.settings
    }

    pub fn symbol(&self) -> Option<&Symbol> {
        self.symbol.as_ref()
    }

    /// The last bid and ask seen for the chart's symbol.
    pub fn quote(&self) -> (Option<i64>, Option<i64>) {
        (self.bid, self.ask)
    }

    /// The ids of the symbols of the account by name (see `key_of`), so an indicator that reads
    /// the prices of a second symbol can ask for them.
    pub fn set_symbol_ids(
        &mut self,
        ids: std::sync::Arc<std::collections::HashMap<String, i64>>,
        cx: &mut Context<Self>,
    ) {
        self.symbol_ids = ids;
        self.custom.symbols_changed();
        cx.notify();
    }

    /// The trading hours of the chart's symbol (ignored for another symbol).
    pub fn set_hours(
        &mut self,
        id: i64,
        hours: Option<std::sync::Arc<crate::domain::market::TradingHours>>,
        cx: &mut Context<Self>,
    ) {
        let hours = hours.filter(|_| self.symbol.as_ref().is_some_and(|s| s.id == id));
        let same = match (&self.hours, &hours) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.hours = hours;
            cx.notify();
        }
    }

    /// Whether this is the chart the user works on.
    /// Keeps the hint of a drawing tool `floor` pixels off the bottom of the chart.
    pub fn set_hint_floor(&mut self, floor: f32, cx: &mut Context<Self>) {
        if (self.hint_floor - floor).abs() > 0.5 {
            self.hint_floor = floor;
            cx.notify();
        }
    }

    pub fn set_selected(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.selected != selected {
            self.selected = selected;
            if !selected {
                self.menu = None;
            }
            cx.notify();
        }
    }

    /// Where the market of the chart's symbol stands now, when its hours are known.
    pub fn market_status(&self) -> Option<crate::domain::market::MarketStatus> {
        Some(self.hours.as_ref()?.status_at(now_ms()))
    }

    /// Tells the chart how many charts the layout holds.
    pub fn set_layout_charts(&mut self, count: usize, cx: &mut Context<Self>) {
        if self.layout_charts != count {
            self.layout_charts = count;
            cx.notify();
        }
    }

    pub fn set_symbol(
        &mut self,
        id: i64,
        name: SharedString,
        digits: u32,
        pip_position: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if self.symbol.as_ref().is_some_and(|s| s.id == id) {
            if let Some(symbol) = self.symbol.as_mut()
                && pip_position.is_some()
                && (symbol.digits != digits || symbol.pip_position != pip_position)
            {
                symbol.digits = digits;
                symbol.pip_position = pip_position.or(symbol.pip_position);
                self.data_changed(cx);
            }
            return;
        }
        self.symbol = Some(Symbol {
            id,
            name,
            digits,
            pip_position,
        });
        self.lines.clear();
        self.reload(cx);
        self.request_drawing_atr(cx);
    }

    /// The broker said how the symbol is quoted.
    pub fn set_quote_details(
        &mut self,
        id: i64,
        digits: u32,
        pip_position: i64,
        cx: &mut Context<Self>,
    ) {
        if let Some(symbol) = self.symbol.as_mut().filter(|s| s.id == id)
            && (symbol.digits != digits || symbol.pip_position != Some(pip_position))
        {
            symbol.digits = digits;
            symbol.pip_position = Some(pip_position);
            self.data_changed(cx);
        }
    }

    pub fn set_timeframe(&mut self, timeframe: Timeframe, cx: &mut Context<Self>) {
        if self.timeframe == timeframe {
            return;
        }
        self.timeframe = timeframe;
        self.reload(cx);
        self.request_drawing_atr(cx);
        cx.emit(ChartEvent::SettingsChanged);
    }

    /// Changes the settings with `change`; recomputes what depends on them, redraws, and tells the
    /// layout (which saves them) when they really changed.
    pub fn edit_settings(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut ChartSettings),
    ) {
        let before = self.settings.clone();
        change(&mut self.settings);
        self.settings = std::mem::take(&mut self.settings).normalized();
        if self.settings == before {
            return;
        }
        let layout_changed = before.kind != self.settings.kind
            || (self.settings.kind.is_derived() && before.transform != self.settings.transform);
        let data_changed = layout_changed
            || before.studies != self.settings.studies
            || before.zone != self.settings.zone;
        if data_changed {
            self.rebuild_display();
        }
        if before.kind != self.settings.kind {
            let footprint = self.settings.kind == ChartKind::Footprint;
            if footprint {
                self.view.bar_px = self.view.bar_px.max(footprint::DEFAULT_BAR_PX);
            } else if before.kind == ChartKind::Footprint {
                // Bars as wide as a footprint's would be absurd as candles.
                self.view.bar_px = self.timeframe.default_bar_px();
                self.reset_flow();
            }
        }
        if layout_changed {
            // A different construction has a different number of points: start from the end.
            self.view = View::new(self.view.bar_px);
        }
        if before.scale != self.settings.scale || before.invert != self.settings.invert {
            self.view.price = view::PriceScale::Auto;
        }
        cx.emit(ChartEvent::SettingsChanged);
        cx.notify();
        self.ensure_flow(cx);
    }

    pub fn set_kind(&mut self, kind: ChartKind, cx: &mut Context<Self>) {
        self.edit_settings(cx, |s| s.kind = kind);
    }

    /// Adds an indicator, with its defaults.
    pub fn add_study(&mut self, config: StudyConfig, cx: &mut Context<Self>) {
        if self.settings.studies.len() >= self.max_studies {
            crate::ui::kit::toast::show(
                cx,
                crate::ui::kit::toast::Kind::Warning,
                "Indicator limit reached",
                crate::ui::kit::shortcut::text(&format!(
                    "This chart allows {} indicators. Change the limit in Settings (Ctrl+,).",
                    self.max_studies
                )),
            );
            return;
        }
        self.edit_settings(cx, |s| s.studies.push(config));
    }

    pub fn max_studies(&self) -> usize {
        self.max_studies
    }

    pub fn set_max_studies(&mut self, limit: usize, cx: &mut Context<Self>) {
        self.max_studies = limit.clamp(1, settings::MAX_STUDIES);
        cx.notify();
    }

    pub fn remove_study(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_settings(cx, |s| {
            if index < s.studies.len() {
                s.studies.remove(index);
            }
        });
    }

    /// The orders, positions and alerts to show.
    pub fn set_lines(&mut self, lines: Vec<ChartLine>, cx: &mut Context<Self>) {
        if self.lines != lines {
            self.lines = lines;
            cx.notify();
        }
    }

    pub fn digits(&self) -> u32 {
        self.symbol.as_ref().map_or(5, |s| s.digits)
    }
}
