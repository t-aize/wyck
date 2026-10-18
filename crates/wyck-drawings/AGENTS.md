# wyck-drawings

The drawings a trader puts on a chart: what a drawing is, how it becomes shapes, how it is made and
changed with the pointer, and the undo history. Pure code: no window, no file, no socket. The chart
is reached through one trait, `geometry::Projection`.

Depends on: `wyck-core`, `wyck-trading`, `serde`, `serde_json` (`toml` in tests only).
Used by: `wyck-app`, `wyck-chart`, `wyck-indicators`, `wyck-ui-chart`, `wyck-ui-editor`,
`wyck-ui-shell`, `wyck-ui-trading`.

## Module map

| Module | What it holds |
|---|---|
| `model` | `Drawing` (a tool, its points, its look), `Point` (time in ms + raw price), `DrawingsDoc` (the saved file), templates, and the size limits. Submodules: `tool` (`Tool`, its groups and how many points it takes), `style` (line, fill, text, levels), `capabilities` (which properties a tool has), `defaults` (the look a new drawing starts with). |
| `look` | The small pieces of a look beyond line and fill: the dash of a line (`Dash`, re-exported by `model::style`), text layout, line ends (`Caps`), level text, measure and volume-profile looks. |
| `prim` | The vocabulary under every tool's shapes: `Projection` (the chart seen from here), `Prim` (a shape to draw), `P`, `Anchor`, `Part` and the helpers the shapes are built from (`seg`, `lead`, `extended`, `fib`). It depends on nothing above it. |
| `geometry` | The public face of `prim`, plus `prims_with` (drawing -> shapes, by tool), `handles` and the hit tests. Submodules: `shapes`, `measure`, `hit`. |
| `book` | `Book`: the drawings of every symbol plus the pointer state machine (create, pick, hover, move, reshape, undo). Submodules: `create`, `pick`, `edit`, `actions`, `history`, `templates`. |
| `position` | `PositionSettings`: the sizing of a long or short position drawing (risk, leverage, lot step, point value) and its look beyond line and fill (colors, zone opacities, outer lines, tags, whether the zones take a press). |

Private, built on `prim` and reached through `geometry`: `figures` (pitchforks, Gann, patterns, Elliott waves), `extras`
(information lines, regression, ranges, cycles, curves, notes, tables, bars, VWAP, volume) and `decor`
(the words and the line ends put on any drawing). Only `ICONS`, `icon_key` and `wave_names` are
re-exported at the root, for the menus.

## Invariants and traps

- A point is a time and a price, never a screen position, so a drawing stays put when the chart is
  scrolled, zoomed or shown on another timeframe. Screen positions exist only inside `geometry` and
  `book`, and only through a `Projection`.
- The saved form is a promise. `Tool` names are written in files (`Tool::code`) and never change;
  an unknown name loads as `Tool::Unknown` and the drawing is dropped by `is_valid`, so a file from
  a newer version still opens. Every field has `#[serde(default)]`; a look piece at its default is
  left out of the file. Raising `SCHEMA_VERSION` needs a step in the app's migrations (a test there
  fails otherwise).
- `DrawingsDoc::normalized` and `Drawing::normalized` are what makes a file safe: finite prices,
  the right number of points for the tool, unique ids, `next_id` past every id, the limits below.
  Anything read from disk or pasted goes through them.
- Limits against a runaway file: `MAX_DRAWINGS_PER_SYMBOL`, `MAX_BRUSH_POINTS`, `MAX_LEVELS`, 64 points
  in an arrow path, 200 steps of undo.
- `Book` has no idea of a window: a press, a move and a release come in with a `Projection`, and
  drawings come out. Keep it that way, it is what makes the fiddly rules testable. The rule for placing
  points is written at the top of `book.rs`; read it before touching `create`.
- A grip is on what is drawn. `geometry::handles` gives a tool whose last point only sets a size or an offset (`grips_may_leave_shape`: channels, rotated rectangle, wedge, cycles, regression) a grip placed on a drawn line where a press changes nothing, and `book/edit.rs` keeps the third point of a channel inside the span of its lines while it is placed or dragged. A new tool of that kind goes in both, and in the test `a_grip_stays_on_what_is_drawn_even_when_its_point_is_far_from_it`.
- Every drawing under the pointer offers its grips (`Book::hit_resolved`), not only the selected one: the chart shows them on hover. `Book::pick_resolved` (the selected one's grips first, then the topmost drawing) is what both a press and the hover ask, so the cursor never disagrees with what a press does.
- A replay sets a volatile `Book::horizon` (never saved, never a change worth saving): `Book::shows` is the one test of whether a drawing shows on a timeframe now, and it hides the drawings whose first anchor is after the horizon, from the screen, the pick and the box select alike. Use it, not `Drawing::shows_on`, wherever a drawing is shown or picked.
- `Book` is shared by every chart. `hold` / `release_hold` / `held_elsewhere` say which chart holds the button, so that a second chart of the same symbol does not move a drawing with its own projection. Escape during a drag puts the drawing back (`Book::cancel`).
- The tags of a position show while the pointer is over it (or always, with `always_stats`), so they are part of it for the hit tests. When the zones are shorter than a tag, or the box narrower than its words, the three tags become one, beside the box (`position_prims`), which is not part of it.
- A position takes a press in its zones only if they are painted and `grab_zones` is on; otherwise its lines, edges and grips do. A new position starts small (a few percent of the price span, 2R) or like the last one edited (`position_shape`, kept for the session, not saved).
- The money on a position drawing is a plan, not a quote: no live rate. The quantity comes from
  `wyck_trading::contract::quantity_for_risk`, the same formula as the order ticket. Do not copy it.
- Raw prices (what the chart holds) and real prices (what a trader reads) differ; only
  `Projection::real_price` converts, and only the tools that work out money should call it.

## Global state

- `geometry::label_size` and `prims_with` read `wyck_core::text_scale`, set by the interface. A test
  that depends on the size of text must not set it: other tests read it.
- `Style::face` interns a font name with `wyck_core::intern` (leaked, capped).

## Tests

Unit tests next to the code. Geometry and tool shapes are tested with the linear `Projection` of
`geometry::tests` (`Linear`, `drawing`), so nothing needs a chart. The save/load round trip of a
look and the rules of `Book` (which click places which point, what undo restores) are the tests
that must not be weakened. Run: `cargo test -p wyck-drawings`.

## Where to add X

- A new tool: a variant of `Tool` (a new name, never a rename), its group, label and `anchors` in
  `model/tool.rs`, its capabilities and default look in `model/`, its shapes in `figures` or
  `extras`, its handles in `geometry/hit.rs` (on the shape, see above), and tests for geometry and for the saved round trip.
- A new property of a look: a field with `#[serde(default)]` in `model/style.rs` or `look.rs`,
  skipped when at its default, with `normalized` keeping it in range.
- A new rule for the pointer: `book/` (`create`, `edit`, `pick`), tested through `Book`.

## Do not

- Do not use `gpui`, `tokio`, files or the network: the crate makes shapes (`Prim`), the chart paints
  them. `cargo xtask deps` fails on such a dependency.
- Do not store a screen position in a drawing.
- Do not rename a `Tool` variant or a serde field without an alias and a migration.
- Do not depend on `wyck-indicators` or `wyck-chart`: the arrow goes the other way.
