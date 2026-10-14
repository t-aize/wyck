//! The colors a chart draws with, and the conversions between the chart's own color types and gpui's.

use wyck_chart::{ChartColors, Hsla, Palette, Rgba};

pub(super) fn palette_for_chart(colors: &ChartColors) -> Palette {
    let mut base = Palette::new();
    let color = chart_rgba;
    base.up = color(crate::ui::kit::theme::chart_up());
    base.down = color(crate::ui::kit::theme::chart_down());
    base.line = color(crate::ui::kit::theme::chart_line());
    base.grid = color(crate::ui::kit::theme::chart_grid());
    base.text = color(crate::ui::kit::theme::chart_muted());
    base.text_strong = color(crate::ui::kit::theme::chart_fg());
    base.bg = color(crate::ui::kit::theme::chart_bg());
    base.tag = color(crate::ui::kit::theme::chart_tag());
    base.border = color(crate::ui::kit::theme::chart_border());
    base.crosshair = color(crate::ui::kit::theme::chart_crosshair());
    base.accent = color(crate::ui::kit::theme::accent());
    base.kagi_yang = base.up;
    base.kagi_yin = base.down;
    base.pnf_up = base.up;
    base.pnf_down = base.down;
    Palette::for_chart_with_base(colors, base)
}

pub(super) struct DesktopPalette {
    pub(super) up: gpui::Rgba,
    pub(super) down: gpui::Rgba,
    pub(super) text: gpui::Rgba,
    pub(super) text_strong: gpui::Rgba,
    pub(super) bg: gpui::Rgba,
}

// The chart crate has no gpui, so its colors are types of its own: these move them across.

pub(super) fn chart_rgba(color: gpui::Rgba) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

pub(super) fn gpui_rgba(color: Rgba) -> gpui::Rgba {
    gpui::Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

pub(super) fn gpui_hsla(color: Hsla) -> gpui::Hsla {
    gpui::Hsla {
        h: color.h,
        s: color.s,
        l: color.l,
        a: color.a,
    }
}
