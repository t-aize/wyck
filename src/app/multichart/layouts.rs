//! The arrangements of charts on the screen: how many, and how they share the space.
//!
//! A layout is a grid of `cols` by `rows` unit squares and a list of cells, each covering a
//! rectangle of those squares. That describes a plain grid, one big chart beside a stack of small
//! ones, two rows of different lengths, and so on, and the same data draws the little icon of a
//! layout in the picker.

use std::sync::OnceLock;

/// The part of the grid a chart covers, in grid squares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub cols: u32,
    pub rows: u32,
    pub cells: Vec<Cell>,
}

impl Layout {
    pub fn count(&self) -> usize {
        self.cells.len()
    }
}

/// Which layout: the number of charts, and the variant among those with that number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutKey {
    pub count: usize,
    pub variant: usize,
}

impl LayoutKey {
    pub const SINGLE: Self = Self {
        count: 1,
        variant: 0,
    };
}

/// Where the big chart goes in a layout with one big chart and a stack beside it.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: u32, b: u32) -> u32 {
    a / gcd(a, b) * b
}

/// `cols` by `rows` charts of the same size.
fn grid(cols: u32, rows: u32) -> Layout {
    let cells = (0..rows)
        .flat_map(|y| (0..cols).map(move |x| Cell { x, y, w: 1, h: 1 }))
        .collect();
    Layout { cols, rows, cells }
}

/// One big chart taking half the space, and `n - 1` charts sharing the other half.
fn main_side(n: u32, side: Side) -> Layout {
    let rest = n - 1;
    match side {
        Side::Left | Side::Right => {
            let big_x = if matches!(side, Side::Left) { 0 } else { 1 };
            let mut cells = vec![Cell {
                x: big_x,
                y: 0,
                w: 1,
                h: rest,
            }];
            cells.extend((0..rest).map(|y| Cell {
                x: 1 - big_x,
                y,
                w: 1,
                h: 1,
            }));
            Layout {
                cols: 2,
                rows: rest,
                cells,
            }
        }
        Side::Top | Side::Bottom => {
            let big_y = if matches!(side, Side::Top) { 0 } else { 1 };
            let mut cells = vec![Cell {
                x: 0,
                y: big_y,
                w: rest,
                h: 1,
            }];
            cells.extend((0..rest).map(|x| Cell {
                x,
                y: 1 - big_y,
                w: 1,
                h: 1,
            }));
            Layout {
                cols: rest,
                rows: 2,
                cells,
            }
        }
    }
}

/// Rows with the given number of charts each, every row spanning the full width.
fn rows_of(counts: &[u32]) -> Layout {
    let cols = counts.iter().copied().fold(1, lcm);
    let mut cells = Vec::new();
    for (y, &n) in counts.iter().enumerate() {
        let w = cols / n;
        cells.extend((0..n).map(|i| Cell {
            x: i * w,
            y: y as u32,
            w,
            h: 1,
        }));
    }
    Layout {
        cols,
        rows: counts.len() as u32,
        cells,
    }
}

/// Columns with the given number of charts each, every column spanning the full height.
fn cols_of(counts: &[u32]) -> Layout {
    let flipped = rows_of(counts);
    Layout {
        cols: flipped.rows,
        rows: flipped.cols,
        cells: flipped
            .cells
            .iter()
            .map(|c| Cell {
                x: c.y,
                y: c.x,
                w: c.h,
                h: c.w,
            })
            .collect(),
    }
}

/// Every layout on offer, grouped by how many charts they hold, fewest first.
pub fn catalog() -> &'static [(usize, Vec<Layout>)] {
    static CATALOG: OnceLock<Vec<(usize, Vec<Layout>)>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        use Side::{Bottom, Left, Right, Top};
        vec![
            (1, vec![grid(1, 1)]),
            (2, vec![grid(2, 1), grid(1, 2)]),
            (
                3,
                vec![
                    grid(3, 1),
                    grid(1, 3),
                    main_side(3, Left),
                    main_side(3, Right),
                    main_side(3, Top),
                    main_side(3, Bottom),
                ],
            ),
            (
                4,
                vec![
                    grid(2, 2),
                    grid(1, 4),
                    grid(4, 1),
                    main_side(4, Left),
                    main_side(4, Right),
                    main_side(4, Top),
                    main_side(4, Bottom),
                    rows_of(&[1, 1, 2]),
                    cols_of(&[1, 1, 2]),
                    rows_of(&[2, 1, 1]),
                ],
            ),
            (
                5,
                vec![
                    rows_of(&[2, 3]),
                    rows_of(&[3, 2]),
                    cols_of(&[2, 3]),
                    cols_of(&[3, 2]),
                    main_side(5, Left),
                    main_side(5, Right),
                    main_side(5, Top),
                    main_side(5, Bottom),
                    grid(5, 1),
                    grid(1, 5),
                ],
            ),
            (
                6,
                vec![
                    grid(3, 2),
                    grid(2, 3),
                    grid(6, 1),
                    grid(1, 6),
                    main_side(6, Left),
                    main_side(6, Top),
                ],
            ),
            (7, vec![rows_of(&[3, 4]), grid(7, 1), main_side(7, Left)]),
            (8, vec![grid(4, 2), grid(2, 4), grid(8, 1), grid(1, 8)]),
            (
                9,
                vec![grid(3, 3), rows_of(&[4, 5]), grid(9, 1), grid(1, 9)],
            ),
            (10, vec![grid(5, 2), grid(10, 1), grid(1, 10)]),
            (12, vec![grid(4, 3), grid(3, 4), grid(6, 2)]),
            (14, vec![grid(7, 2)]),
            (16, vec![grid(4, 4), grid(8, 2)]),
        ]
    })
}

/// The layout for a key, or the single chart if the key names nothing.
pub fn layout(key: LayoutKey) -> &'static Layout {
    catalog()
        .iter()
        .find(|(count, _)| *count == key.count)
        .and_then(|(_, variants)| variants.get(key.variant))
        .unwrap_or_else(|| &catalog()[0].1[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_has_the_count_it_is_filed_under() {
        for (count, variants) in catalog() {
            for (variant, layout) in variants.iter().enumerate() {
                assert_eq!(layout.count(), *count, "{count} variant {variant}");
            }
        }
    }

    #[test]
    fn every_layout_tiles_its_grid_exactly() {
        // No hole and no overlap: each grid square is covered by exactly one chart.
        for (count, variants) in catalog() {
            for (variant, layout) in variants.iter().enumerate() {
                let mut covered = vec![0u32; (layout.cols * layout.rows) as usize];
                for cell in &layout.cells {
                    assert!(cell.w > 0 && cell.h > 0, "{count}/{variant}: empty cell");
                    assert!(
                        cell.x + cell.w <= layout.cols && cell.y + cell.h <= layout.rows,
                        "{count}/{variant}: cell outside the grid"
                    );
                    for y in cell.y..cell.y + cell.h {
                        for x in cell.x..cell.x + cell.w {
                            covered[(y * layout.cols + x) as usize] += 1;
                        }
                    }
                }
                assert!(
                    covered.iter().all(|c| *c == 1),
                    "{count}/{variant}: {covered:?}"
                );
            }
        }
    }

    #[test]
    fn the_counts_on_offer_are_the_ones_of_the_picker() {
        let counts: Vec<usize> = catalog().iter().map(|(c, _)| *c).collect();
        assert_eq!(counts, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 16]);
    }

    #[test]
    fn rows_of_different_lengths_share_the_width() {
        let l = rows_of(&[2, 3]);
        assert_eq!((l.cols, l.rows), (6, 2));
        assert_eq!(l.cells[0].w, 3);
        assert_eq!(l.cells[2].w, 2);
    }

    #[test]
    fn an_unknown_key_falls_back_to_a_single_chart() {
        let l = layout(LayoutKey {
            count: 11,
            variant: 0,
        });
        assert_eq!(l.count(), 1);
        let l = layout(LayoutKey {
            count: 3,
            variant: 99,
        });
        assert_eq!(l.count(), 1);
    }
}
