//! The bands of a chart and the price scale of its prices band.

use super::footprint;
use super::studies;
use super::{Geometry, PriceMap, quote_unit};
use crate::domain::chart::data::Series;
use crate::domain::chart::display::{Display, effective_kind};
use crate::domain::chart::settings::{ChartSettings, ScaleMode};
use crate::domain::chart::view::{PriceScale, View, padded_range};
use wyck_indicators::spec::Placement;

/// The bands of a chart of `w` by `h` with these settings.
pub fn geometry(settings: &ChartSettings, w: f64, h: f64) -> Geometry {
    let mut weights = vec![settings.main_weight];
    weights.extend(settings.panes().iter().map(|pane| pane.weight));
    Geometry::new(w, h, &weights)
}

/// The price scale of the prices band, or `None` without data.
pub fn main_map(
    raw: &Series,
    display: &Display,
    settings: &ChartSettings,
    view: &View,
    geometry: &Geometry,
    digits: u32,
) -> Option<PriceMap> {
    let series = display.shown(raw);
    let plot_w = geometry.plot_w();
    let (first, last) = view.visible(series.len(), plot_w);
    let kind = effective_kind(raw, settings.kind);
    let (lo, hi) = match view.price {
        PriceScale::Manual { lo, hi } if hi > lo => (lo, hi),
        _ => {
            let (mut lo, mut hi) = series
                .price_range(first, last, kind.uses_ohlc())
                .map(|(l, h)| (l as f64, h as f64))?;
            // Indicators on the prices count too, so a band never runs off the top.
            for (config, output) in settings.studies.iter().zip(&display.studies) {
                if config.spec().placement != Placement::Overlay || !config.visible {
                    continue;
                }
                if let Some(output) = output {
                    for plot in &output.plots {
                        let style = config.plot_style(plot.key);
                        if !style.visible {
                            continue;
                        }
                        if let Some((l, h)) = studies::visible_range(
                            plot,
                            style.kind.unwrap_or(plot.kind),
                            first,
                            last,
                        ) {
                            lo = lo.min(l);
                            hi = hi.max(h);
                        }
                    }
                }
            }
            padded_range(lo, hi, quote_unit(digits) * 10.0, settings.margin.share())
        }
    };
    let (lo, hi) = if settings.scale == ScaleMode::Log {
        (lo.max(1.0), hi.max(lo.max(1.0) + 1.0))
    } else {
        (lo, hi)
    };
    let band = geometry.main();
    let reserved = 12.0 + f64::from(footprint::reserved(settings));
    let base = series
        .value_at(first.min(series.len().saturating_sub(1)))
        .map_or(0.0, |v| v as f64);
    Some(PriceMap {
        lo,
        hi,
        top: band.top + 12.0,
        bottom: (band.bottom() - reserved).max(band.top + 30.0),
        mode: settings.scale,
        invert: settings.invert,
        base,
    })
}
