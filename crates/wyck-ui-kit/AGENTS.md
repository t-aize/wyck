# wyck-ui-kit

The widget kit of the desktop app: every control, menu, dialog, notice and color, in one look, and
the files the interface embeds (the Inter font, the logo, the flags and logos of the symbols). A
screen builds its UI from these pieces and reads its colors and sizes from `theme` and `tokens`, so
a change made here reaches every screen at once. No domain, no network, no files of the user.

Depends on: `wyck-core`, `gpui`, `gpui-kit` (the component library it restyles), `rust-embed`,
`unicode-segmentation`, `tracing`.
Used by: `wyck` (every screen).

## Module map

All the modules are public: a screen names them (`button::primary`, `tokens::text::body`,
`theme::fg()`), so the module is the namespace. What a screen does not call is `pub(crate)`.

| Module | What it holds |
|---|---|
| `theme` | The colors in force (`fg()`, `bg()`, `accent()`, `chart_*()`...), derived from one palette, `wyck_core::appearance::Colors`. `put_in_force` applies a look chosen by the user (palette, font, scale, animations); `apply` points the component library at it and repaints the windows. |
| `tokens` | The sizes every screen shares, by topic: `text`, `height`, `space`, `bar`, `field`, `swatch`, `measure`, `menu`. They follow the text scale of `wyck_core`. |
| `assets` | `Assets`, the `AssetSource` of the app (Lucide icons from gpui-kit, `marks/...`, the logo), `FONT`, `LOGO_PATH`, `has_mark`. |
| `button`, `controls`, `field`, `number`, `input`, `icon` | Buttons, pickers (segmented choice, switch, swatch, chip), labelled inputs and steppers, text and number fields at the control height, icons. |
| `form`, `layout`, `confirm` | The frame of a settings panel and of a dialog, groups of labelled rows, page blocks (cards, tiles, banners, badges), the "are you sure" dialog with its tone. |
| `modal`, `menu` (with `menu::parts`), `toast` | The one modal window; menus on a right click or under a button; corner notices. |
| `color_picker`, `font_picker`, `text_input` | The color panel; the list of installed fonts; the single-line input with its IME handling (copied from gpui's own example). |
| `anim`, `focus`, `hints`, `window_bar`, `prelude` | Motion helpers; keyboard access for hand-drawn controls (`Keyboard::keyboard`); the keys of the actions for tooltips; the fallback window bar; the gpui-kit traits a screen may use. |

## Invariants and traps

- **No color literal outside `theme`, `color_picker` and the brand colors of the symbol marks** (a
  file of the dashboard). The test `no_inline_colors` (in `wyck`, which holds the screens) fails on
  a `0xRRGGBB` anywhere else. A screen reads `theme::` values.
- **A screen never configures a gpui-kit component itself.** Sizes and variants (`Sizable`,
  `ButtonVariants`) stay out of `prelude` on purpose: the kit picks them.
- **An animation plays once per element id.** To replay one, change the id (the `epoch` values the
  screens thread through). With animations off, the helpers still run their animation, but in 1 ms,
  so the element ends where it should.
- **`text_input` is gpui's example, trimmed.** The grapheme boundaries and the UTF-16 conversions are
  what correct editing takes; do not "simplify" them.
- **A control drawn from a `div` is invisible to the keyboard** until `.keyboard()` makes it a Tab
  stop with a visible focus ring.
- **Marks and fonts keep their licenses**: `assets/fonts/LICENSE.txt`, `assets/marks/*/LICENSE.txt`.
  Marks are trademarks of their owners, shown only to identify an instrument.

## Global state

Each is here because the helpers have no window or context at hand when they are called.

- `theme`: the palette (`RwLock<Colors>`) and the font (`RwLock<Option<String>>`). Written by
  `put_in_force`, read on every paint.
- `anim`: whether things move (`AtomicBool`), set from the same look. The scale of the interface is
  not here: it is `wyck_core::text_scale`, which the charts read too.
- `hints`: the keys of the actions (`RwLock<HashMap>`), filled by the keymap of the shell whenever
  it binds keys.
- `color_picker`: the saved colors, the function that persists them and the open panels (gpui
  globals); `modal`: the handle of the host that shows the modal (gpui global).

## Tests

25 unit tests next to the code: the color maths (hex, RGB, HSV), the palette, the tokens and
the scale, the size of the dialog, the menu, the notices, the number fields, the font search and
the hint table. The tests that read the sources of the screens (`no_inline_colors`, `keymap_guard`)
live in `wyck` and also read this crate. Run: `cargo test -p wyck-ui-kit`.

## Where to add X

- A control: a function or a builder in the module of its family, with sizes from `tokens` and
  colors from `theme`; make it `pub` only when a screen calls it.
- A color the user can change: the palette `wyck_core::appearance::Colors`, then a `theme::` reader.
  A color derived from the palette: a reader in `theme` only.
- A size: `tokens`, in the module of its topic, scaled with `scaled(..)`.
- An icon: `gpui_kit::assets::IconName` (Lucide), through `icon::tinted`. A flag or a logo:
  `assets/marks/<set>/<name>.svg`, with its license next to it.

## Do not

- Do not depend on a domain crate, on `wyck-app`, on `tokio`, `reqwest`, `keyring` or on files of
  the user: `cargo xtask deps` fails on them.
- Do not write a color literal, a pixel size or an animation duration in a screen: it belongs here.
- Do not export a widget "just in case": `pub` is for what a screen calls, so that `dead_code` can
  see what nobody uses.
