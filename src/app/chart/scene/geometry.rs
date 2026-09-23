//! Where things go on a chart: the price axis on the right, the time axis at the bottom, and the
//! plot between them cut into horizontal bands, the prices on top and one band per indicator pane
//! under them, sized by their weights.

/// Width of the price axis on the right.
pub const AXIS_W: f32 = 76.0;
/// Height of the time axis at the bottom.
pub const AXIS_H: f32 = 28.0;
/// The least height a pane is given when there is room for it.
pub const MIN_BAND: f64 = 36.0;
/// How close (in pixels) the pointer must be to the line between two panes to drag it.
pub const SEPARATOR_REACH: f64 = 4.0;

/// A horizontal band of the plot, from the top of the chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub top: f64,
    pub h: f64,
}

impl Band {
    pub fn bottom(&self) -> f64 {
        self.top + self.h
    }

    pub fn contains(&self, y: f64) -> bool {
        y >= self.top && y < self.bottom()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub w: f64,
    pub h: f64,
    /// The prices first, then the indicator panes from top to bottom.
    pub bands: Vec<Band>,
}

impl Geometry {
    /// Cuts a chart of `w` by `h` into bands of the given weights (the prices' weight first).
    pub fn new(w: f64, h: f64, weights: &[f32]) -> Self {
        let plot_h = (h - f64::from(AXIS_H)).max(1.0);
        let weights: Vec<f64> = if weights.is_empty() {
            vec![1.0]
        } else {
            weights.iter().map(|w| f64::from(*w).max(0.01)).collect()
        };
        let total: f64 = weights.iter().sum();
        let mut heights: Vec<f64> = weights.iter().map(|w| plot_h * w / total).collect();
        // Give small panes their minimum when the prices can spare it.
        if heights.len() > 1 {
            let mut taken = 0.0;
            for height in heights.iter_mut().skip(1) {
                if *height < MIN_BAND {
                    taken += MIN_BAND - *height;
                    *height = MIN_BAND;
                }
            }
            if heights[0] - taken >= MIN_BAND * 2.0 {
                heights[0] -= taken;
            } else {
                // No room: back to the plain shares.
                heights = weights.iter().map(|w| plot_h * w / total).collect();
            }
        }
        let mut top = 0.0;
        let bands = heights
            .into_iter()
            .map(|h| {
                let band = Band { top, h };
                top += h;
                band
            })
            .collect();
        Self { w, h, bands }
    }

    /// A chart with the prices alone.
    pub fn single(w: f64, h: f64) -> Self {
        Self::new(w, h, &[1.0])
    }

    pub fn plot_w(&self) -> f64 {
        (self.w - f64::from(AXIS_W)).max(1.0)
    }

    /// The height of the whole plot, every band together.
    pub fn plot_h(&self) -> f64 {
        (self.h - f64::from(AXIS_H)).max(1.0)
    }

    pub fn main(&self) -> Band {
        self.bands[0]
    }

    /// The band under `y`, if `y` is on the plot.
    pub fn band_at(&self, y: f64) -> Option<usize> {
        self.bands.iter().position(|band| band.contains(y))
    }

    /// The line between band `i` and band `i + 1` that `y` is on, if any.
    pub fn separator_at(&self, y: f64) -> Option<usize> {
        self.bands
            .iter()
            .take(self.bands.len().saturating_sub(1))
            .position(|band| (band.bottom() - y).abs() <= SEPARATOR_REACH)
    }

    /// The weights that give band `i` and band `i + 1` the heights they have after the line
    /// between them moved by `dy` pixels, the others unchanged. Returns the two new weights.
    pub fn drag_separator(&self, weights: &[f32], i: usize, dy: f64) -> Option<(f32, f32)> {
        let (a, b) = (self.bands.get(i)?, self.bands.get(i + 1)?);
        let (wa, wb) = (f64::from(*weights.get(i)?), f64::from(*weights.get(i + 1)?));
        let pair_h = a.h + b.h;
        let new_a = (a.h + dy).clamp(
            MIN_BAND.min(pair_h / 2.0),
            pair_h - MIN_BAND.min(pair_h / 2.0),
        );
        let pair_w = wa + wb;
        let wa2 = pair_w * new_a / pair_h;
        Some((wa2 as f32, (pair_w - wa2) as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_share_the_plot_by_weight_and_touch() {
        let g = Geometry::new(1_000.0, 628.0, &[3.0, 1.0]);
        assert_eq!(g.plot_h(), 600.0);
        assert_eq!(g.bands.len(), 2);
        assert!((g.bands[0].h - 450.0).abs() < 1e-9);
        assert!((g.bands[1].top - 450.0).abs() < 1e-9);
        assert!((g.bands[1].bottom() - 600.0).abs() < 1e-9);
        assert_eq!(g.band_at(10.0), Some(0));
        assert_eq!(g.band_at(500.0), Some(1));
        assert_eq!(g.band_at(610.0), None);
    }

    #[test]
    fn a_tiny_pane_gets_its_minimum_from_the_prices() {
        let g = Geometry::new(1_000.0, 628.0, &[30.0, 1.0, 1.0]);
        assert!(g.bands[1].h >= MIN_BAND - 1e-9);
        let total: f64 = g.bands.iter().map(|b| b.h).sum();
        assert!((total - 600.0).abs() < 1e-9);
    }

    #[test]
    fn the_line_between_panes_is_found_and_dragged() {
        let g = Geometry::new(1_000.0, 628.0, &[3.0, 1.0]);
        assert_eq!(g.separator_at(452.0), Some(0));
        assert_eq!(g.separator_at(300.0), None);
        let (a, b) = g.drag_separator(&[3.0, 1.0], 0, 60.0).unwrap();
        // 450 + 60 of 600 at a total weight of 4.
        assert!((f64::from(a) - 3.4).abs() < 1e-6 && (f64::from(b) - 0.6).abs() < 1e-6);
        let (a, _) = g.drag_separator(&[3.0, 1.0], 0, 10_000.0).unwrap();
        assert!(f64::from(a) < 4.0, "the lower pane keeps its minimum");
    }

    #[test]
    fn a_chart_with_nothing_but_prices_is_one_band() {
        let g = Geometry::single(800.0, 400.0);
        assert_eq!(g.bands.len(), 1);
        assert_eq!(g.main().h, g.plot_h());
    }
}
