# UI design system

The kit in `src/ui/kit/` is the only place that configures a gpui-kit component or spells out a
size or a color. A screen builds its UI from the kit and reads colors from `theme`.

## Tokens

| Family | Where | Values |
|---|---|---|
| Text | `tokens::text` | caption 10, small 11, body 12, emphasis 13, title 14, heading 16, display 20, hero 26 |
| Control heights | `tokens::height` | tiny 20, compact 24, control 28, large 34 |
| Field widths | `tokens::field` | narrow 84, number 110, wide 130, text 220 |
| Spacing | `tokens::space` | xs 4, sm 8, md 12, lg 16, xl 24 |
| Radius | `tokens::radius` | sm, md (controls), lg (cards), xl (dialogs), full (pills) |
| Colors | `theme` | read through accessors such as `theme::bg()`, `theme::fg()`, `theme::accent()` |

Every size scales with the interface scale (80 to 160 percent) set in the settings.

## Buttons

One default size. A different size needs a reason written in the component.

| Role | Size |
|---|---|
| Forms, dialogs, menus, modal footers | medium (28) |
| Dense toolbars and table rows | small (24) |
| Buy and Sell in the ticket, the main button of the sign-in modal | large (34) |

Variants: primary, secondary, ghost, icon. A clickable element that is not a button still gets a
tab stop and answers Enter and Space.

## Rules the architecture test enforces

- `gpui_kit` is named only inside `ui::kit`.
- No `Button::new` and no `px(<literal>)` outside the kit (the count in the baseline only goes
  down).

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
