//! The charts of a layout as a tree of splits, so the lines between them can be dragged.
//!
//! A layout ([`super::layouts`]) is a grid of cells. Every layout on offer can be cut, one full
//! line at a time, into two or more strips, each of which is a cell or can be cut again (it is a
//! "guillotine" layout). That gives a tree: a split stacks its children side by side or one above
//! the other, and each child has a weight that says how much of the split it takes. Dragging the
//! line between two children moves weight from one to the other.
//!
//! The weights of a layout are saved as one list per split, in the order the tree lists its splits
//! (depth first), which is stable for a given layout.

use super::layouts::{Cell, Layout};

/// The least share of a split a child keeps when a line is dragged.
const MIN_SHARE: f32 = 0.08;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Chart `n` of the layout.
    Leaf(usize),
    Split {
        /// Children stacked left to right (`false`) or top to bottom (`true`).
        vertical: bool,
        children: Vec<Node>,
        /// How much of the split each child takes; they need not add up to anything.
        weights: Vec<f32>,
    },
}

/// A rectangle as shares of the whole area, from the top left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frac {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A line between two children of a split, where it can be dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Divider {
    /// Which split (in depth first order) and which line of it: between child `i` and `i + 1`.
    pub split: usize,
    pub index: usize,
    pub vertical: bool,
    /// Where the line is: for a vertical split a horizontal line at `at` from `from` to `to`
    /// across, for a horizontal split the other way round. All shares of the whole area.
    pub at: f32,
    pub from: f32,
    pub to: f32,
    /// The size of the split along its direction, as a share of the whole, for turning a drag in
    /// pixels into a change of weight.
    pub span: f32,
}

/// The tree of a layout, with weights from the grid (a cell twice as wide weighs twice as much).
pub fn tree(layout: &Layout) -> Node {
    let cells: Vec<(usize, Cell)> = layout.cells.iter().copied().enumerate().collect();
    cut(&cells, 0, 0, layout.cols, layout.rows)
}

fn cut(cells: &[(usize, Cell)], x: u32, y: u32, w: u32, h: u32) -> Node {
    if cells.len() == 1 {
        return Node::Leaf(cells[0].0);
    }
    // A cut is a line no cell crosses. Try vertical lines (children side by side) first.
    for vertical in [false, true] {
        let (start, len) = if vertical { (y, h) } else { (x, w) };
        let mut lines: Vec<u32> = (start + 1..start + len)
            .filter(|line| {
                cells.iter().all(|(_, c)| {
                    let (a, b) = if vertical {
                        (c.y, c.y + c.h)
                    } else {
                        (c.x, c.x + c.w)
                    };
                    *line <= a || *line >= b
                })
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        lines.insert(0, start);
        lines.push(start + len);
        let mut children = Vec::new();
        let mut weights = Vec::new();
        for pair in lines.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let inside: Vec<(usize, Cell)> = cells
                .iter()
                .copied()
                .filter(|(_, c)| {
                    let (lo, hi) = if vertical {
                        (c.y, c.y + c.h)
                    } else {
                        (c.x, c.x + c.w)
                    };
                    lo >= a && hi <= b
                })
                .collect();
            if inside.is_empty() {
                continue;
            }
            children.push(if vertical {
                cut(&inside, x, a, w, b - a)
            } else {
                cut(&inside, a, y, b - a, h)
            });
            weights.push((b - a) as f32);
        }
        return Node::Split {
            vertical,
            children,
            weights,
        };
    }
    // Not a guillotine layout (none on offer): stack the cells one above the other.
    Node::Split {
        vertical: true,
        children: cells.iter().map(|(i, _)| Node::Leaf(*i)).collect(),
        weights: vec![1.0; cells.len()],
    }
}

impl Node {
    /// The weights of every split, depth first.
    pub fn weights(&self) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        self.walk_weights(&mut |weights| out.push(weights.clone()));
        out
    }

    fn walk_weights(&self, f: &mut impl FnMut(&Vec<f32>)) {
        if let Node::Split {
            children, weights, ..
        } = self
        {
            f(weights);
            for child in children {
                child.walk_weights(f);
            }
        }
    }

    /// Puts saved weights back, where each list has the right length and sane numbers.
    pub fn apply_weights(&mut self, saved: &[Vec<f32>]) {
        let mut index = 0;
        self.apply_at(saved, &mut index);
    }

    fn apply_at(&mut self, saved: &[Vec<f32>], index: &mut usize) {
        if let Node::Split {
            children, weights, ..
        } = self
        {
            if let Some(list) = saved.get(*index)
                && list.len() == weights.len()
                && list.iter().all(|w| w.is_finite() && *w > 0.0)
            {
                weights.clone_from(list);
            }
            *index += 1;
            for child in children {
                child.apply_at(saved, index);
            }
        }
    }

    /// Where every chart goes, by chart index, and where the lines between them are.
    pub fn place(&self, count: usize) -> (Vec<Frac>, Vec<Divider>) {
        let mut rects = vec![
            Frac {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            };
            count
        ];
        let mut dividers = Vec::new();
        let mut split = 0;
        self.place_in(
            Frac {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            &mut rects,
            &mut dividers,
            &mut split,
        );
        (rects, dividers)
    }

    fn place_in(
        &self,
        area: Frac,
        rects: &mut [Frac],
        dividers: &mut Vec<Divider>,
        split: &mut usize,
    ) {
        match self {
            Node::Leaf(index) => {
                if let Some(slot) = rects.get_mut(*index) {
                    *slot = area;
                }
            }
            Node::Split {
                vertical,
                children,
                weights,
            } => {
                let me = *split;
                *split += 1;
                let total: f32 = weights.iter().sum::<f32>().max(f32::EPSILON);
                let mut offset = 0.0;
                for (i, (child, weight)) in children.iter().zip(weights).enumerate() {
                    let share = weight / total;
                    let part = if *vertical {
                        Frac {
                            x: area.x,
                            y: area.y + offset * area.h,
                            w: area.w,
                            h: share * area.h,
                        }
                    } else {
                        Frac {
                            x: area.x + offset * area.w,
                            y: area.y,
                            w: share * area.w,
                            h: area.h,
                        }
                    };
                    offset += share;
                    if i + 1 < children.len() {
                        dividers.push(if *vertical {
                            Divider {
                                split: me,
                                index: i,
                                vertical: true,
                                at: area.y + offset * area.h,
                                from: area.x,
                                to: area.x + area.w,
                                span: area.h,
                            }
                        } else {
                            Divider {
                                split: me,
                                index: i,
                                vertical: false,
                                at: area.x + offset * area.w,
                                from: area.y,
                                to: area.y + area.h,
                                span: area.w,
                            }
                        });
                    }
                    child.place_in(part, rects, dividers, split);
                }
            }
        }
    }

    /// Moves the line `index` of split `split` by `delta`, a share of the whole area along the
    /// split's direction. Neither child gets smaller than a minimum share of the split.
    pub fn drag(&mut self, split: usize, index: usize, delta: f32, span: f32) {
        let mut counter = 0;
        self.drag_at(split, index, delta / span.max(f32::EPSILON), &mut counter);
    }

    fn drag_at(&mut self, split: usize, index: usize, delta: f32, counter: &mut usize) -> bool {
        let Node::Split {
            children, weights, ..
        } = self
        else {
            return false;
        };
        if *counter == split {
            if index + 1 < weights.len() {
                let total: f32 = weights.iter().sum();
                let pair = weights[index] + weights[index + 1];
                let min = MIN_SHARE * total;
                let a = (weights[index] + delta * total)
                    .clamp(min.min(pair / 2.0), pair - min.min(pair / 2.0));
                weights[index] = a;
                weights[index + 1] = pair - a;
            }
            return true;
        }
        *counter += 1;
        children
            .iter_mut()
            .any(|child| child.drag_at(split, index, delta, counter))
    }

    /// Every split back to the weights of the grid.
    pub fn reset(&mut self, layout: &Layout) {
        *self = tree(layout);
    }
}

#[cfg(test)]
mod tests {
    use super::super::layouts::catalog;
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn every_layout_on_offer_becomes_a_tree_that_places_every_chart() {
        for (count, variants) in catalog() {
            for (variant, layout) in variants.iter().enumerate() {
                let node = tree(layout);
                let (rects, _) = node.place(layout.count());
                // Each chart lands where the grid puts it.
                for (index, cell) in layout.cells.iter().enumerate() {
                    let r = rects[index];
                    let expected = Frac {
                        x: cell.x as f32 / layout.cols as f32,
                        y: cell.y as f32 / layout.rows as f32,
                        w: cell.w as f32 / layout.cols as f32,
                        h: cell.h as f32 / layout.rows as f32,
                    };
                    assert!(
                        close(r.x, expected.x)
                            && close(r.y, expected.y)
                            && close(r.w, expected.w)
                            && close(r.h, expected.h),
                        "{count}/{variant} chart {index}: {r:?} vs {expected:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_single_chart_has_no_line_and_a_grid_has_its_lines() {
        let one = &catalog()[0].1[0];
        let (_, dividers) = tree(one).place(1);
        assert!(dividers.is_empty());
        let four = catalog().iter().find(|(c, _)| *c == 4).unwrap().1[0].clone();
        let (_, dividers) = tree(&four).place(4);
        // 2 x 2: one line between the columns, and one inside each column.
        assert_eq!(dividers.len(), 3);
    }

    #[test]
    fn dragging_a_line_moves_weight_between_its_two_sides_only() {
        let three = catalog().iter().find(|(c, _)| *c == 3).unwrap().1[0].clone();
        let mut node = tree(&three);
        let (before, dividers) = node.place(3);
        let line = dividers[0];
        node.drag(line.split, line.index, 0.1, line.span);
        let (after, _) = node.place(3);
        assert!(close(after[0].w, before[0].w + 0.1));
        assert!(close(after[1].w, before[1].w - 0.1));
        assert!(
            close(after[2].w, before[2].w),
            "the third chart keeps its width"
        );
        // A huge drag stops at the minimum.
        node.drag(line.split, line.index, 5.0, line.span);
        let (clamped, _) = node.place(3);
        assert!(clamped[1].w > 0.0);
    }

    #[test]
    fn weights_are_saved_and_put_back() {
        let layout = catalog().iter().find(|(c, _)| *c == 4).unwrap().1[0].clone();
        let mut node = tree(&layout);
        let (_, dividers) = node.place(4);
        node.drag(dividers[1].split, dividers[1].index, 0.05, dividers[1].span);
        let saved = node.weights();
        let mut fresh = tree(&layout);
        fresh.apply_weights(&saved);
        assert_eq!(fresh, node);
        // Weights of the wrong shape are ignored.
        let mut other = tree(&layout);
        other.apply_weights(&[vec![1.0], vec![f32::NAN, 1.0]]);
        assert_eq!(other, tree(&layout));
    }
}
