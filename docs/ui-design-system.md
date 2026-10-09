# UI design system

The kit in `src/ui/kit/` is the only place that configures a gpui-kit component or spells out a
size or a color. A screen builds its UI from the kit and reads colors from `theme`.

## Tokens

| Family | Where | Values |
|---|---|---|
| Text | `tokens::text` | caption 10, small 11, body 12, emphasis 13, title 14, heading 16, display 20, hero 26 |
| Control heights | `tokens::height` | tiny 20, compact 24, control 28, large 34 (for custom rows; buttons and fields use the sizes below) |
| Field widths | `tokens::field` | narrow 84, number 110, wide 130, text 220 |
| Spacing | gpui classes, `tokens::space` | the classes `gap_1`, `p_2`... on the steps 0, 1, 2, 3, 4 and 6 (0, 4, 8, 12, 16 and 24 px, the values of `space`); `space::{xs 4, sm 8, md 12, lg 16, xl 24}` where a size is written with `px(..)`. A screen may not use another step (the design test fails). |
| Lines and marks | `tokens::HAIRLINE`, `tokens::splitter`, `tokens::swatch`, `layout::rule_h` and `rule_v` | 1 px lines, the 5 px splitter handle, the dots and color swatches |
| Row and text widths | `tokens::height::row`, `tokens::measure::{note, caption, card, action}`, `tokens::field::select` | 40; 440, 200, 380, 240; 150 |
| Radius | `tokens::radius` | sm, md (controls), lg (cards), xl (dialogs), full (pills) |
| Colors | `theme` | read through accessors such as `theme::bg()`, `theme::fg()`, `theme::accent()` |

Every size scales with the interface scale (80 to 160 percent) set in the settings.

## Buttons

Three sizes, set in one place (`ui::kit::button::Size`). A screen takes a button from a kit
constructor and never calls a size method. The heights are the control heights of `tokens::height`
and scale with the interface; an icon-only button is as wide as it is tall.

| Size | Height | Constructors | Where |
|---|---|---|---|
| `Sm` | 24 | `dense`, `icon_dense` | a dense strip or a table row |
| `Md` | 28 | `primary`, `accent`, `danger`, `outlined`, `standard`, `quiet`, `action`, `icon`, `wide_danger` | forms, dialogs, menus, panels, footers, the drawing rail; the default |
| `Lg` | 34 | `hero`, `trade` | the main button of the sign-in modal; Buy and Sell in the ticket |

Text fields (`ui::kit::input`) are `Md` high (28), dense ones `Sm` (24), so a button next to a
field lines up. A wide button that sits in a card or a menu (`primary`, `wide_danger`,
`outlined(..).w_full()`) is `Md`: only the sign-in modal and the ticket use `Lg`.

Variants: primary, outlined, ghost, danger, icon. A clickable element that is not a button still
gets a tab stop and answers Enter and Space.

## Rules the architecture test enforces

- `gpui_kit` is named only inside `ui::kit`.
- No `Button::new` and no size method (`.small()`, `.large()`...) outside the kit.
- No `px(<literal>)` outside the kit: take a token, or name the size with a `const` in the file when it is the geometry of one drawing (a theme preview, a grip). `tests/design-baseline.txt` is empty and stays so. `chart/paint.rs` is exempt: it draws into an image, in pixels of that image.

## Menus

A long menu is built from groups (`menu::Section`, joined by `menu::sections`): each group has a
short heading in small capitals, and a line separates two groups. The chart toolbar menus, the account panel menus (row, header, bulk) and the indicator menu of the header are built this way too. The right click menu of a chart
reads: the drawing under the pointer (its own heading, then its arrangement, its visibility and
Delete), Trade at <price>, Alerts, Chart, Copy and paste, Drawings, Export. `Menu::popup` also
drops a line at the top or the bottom, two lines in a row and a heading with nothing under it, so
a menu whose parts come and go never shows a stray line.

## Shortcut hints

A hint is written with `Ctrl` and passed through `ui::kit::shortcut::text`, which says `Cmd` and
`Option` on a Mac. Bindings use `secondary`, so the hint and the key agree.

## Inputs

One text field stack, the kit's. Secret fields mask their content, offer a reveal toggle and do
not copy or cut the real text.

## Modals

One modal system, from the kit (`ui::kit::modal`). `modal::open` replaces the modal that is open;
`modal::open_over` (used by `confirm`) puts a confirmation above it, and the panel below comes
back when the confirmation closes. Escape and a click on the veil close a dismissible modal; the
sign-in modal is not part of this system and ignores both (it shakes).

## Window

The system draws the title bar. `ui::kit::window_bar::fallback` draws a minimal bar only on a
desktop that refuses server-side decorations.
