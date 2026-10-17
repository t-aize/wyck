# wyck-ui-shell

The frame of the Wyck window: the root view (`app_view`) that shows the sign-in gate until there is a
session and the dashboard after it, the dashboard (`dashboard`) that lays out the charts, the
trading panels, the symbol picker and the indicator editor, the settings (`settings_hub`), and the
keyboard shortcuts (`keymap`). It is the only crate that knows every screen crate; the screens do
not know it.

Depends on: `wyck-app`, `wyck-chart`, `wyck-clock`, `wyck-core`, `wyck-drawings`, `wyck-indicators`,
`wyck-market`, `wyck-trading`, `wyck-ui-chart`, `wyck-ui-editor`, `wyck-ui-trading`, `wyck-ui-kit`,
`gpui`, `chrono`, `tracing`; `serde` and `serde_json` for the tests of the symbol catalog.
Used by: `wyck` (`AppView`, `keymap::apply`, the two start-up toasts of `settings_hub`).

## Module map

Three modules are public, the ones `wyck` calls; the rest is private.

| Module | What it holds |
|---|---|
| `app_view` | `AppView`: owns the sign-in and the open dashboard; `locked_frame` draws a soft copy of the dashboard behind the sign-in modal. |
| `sign_in_gate` | The sign-in modal, one card per `Phase` of `wyck_app::sign_in` (restoring, editing, verifying, waiting for the browser, choosing an account, authorizing). |
| `dashboard` | `Dashboard`: the screen the app lands on, its actions (module `action`), and what it draws (`view`). |
| `dashboard::{session, catalog, picker, details, lists}` | Following the broker session and the active symbol; the list of symbols an account can trade (no UI); the symbol picker and its details sheet; favorites and watchlists. |
| `dashboard::marks` | The picture of a symbol (`icon_for`): one module per asset class (`forex`, `index`, `metal`, `crypto`, `shares`, `commodity`) says which logo, flag or glyph, and `paint` draws it. |
| `dashboard::{header, chart_buttons, layout_menu, editor_dock, notice}` | The bar across the top and the account menu; the chart type and indicator buttons; the layout picker; the drawer of the script editor; the toast for what the account has to say. |
| `dashboard::trade` | The trading side: account, order ticket, account panel, price alerts and the lines they put on the charts. |
| `keymap` | `DEFS`, the one table of every binding (custom, fixed), `HANDLED` (keys a screen reads by hand), and `apply`, which binds the result in gpui and tells the kit the keys for its tooltips. |
| `settings_hub` | The settings window: one file per page (`look`, `charts`, `behaviour`, `alerts`, `safety`, `scripts`, `shortcuts`, `data`, `folders`, `about`). |

## Invariants and traps

- **Every binding is declared in `keymap::DEFS`.** `KeyBinding::new` anywhere else, or a file that
  reads keys by hand without being listed in `keymap::HANDLED`, fails the guard `keymap_guard` (in
  `wyck`). The settings page shows the table, so a binding outside it is invisible to the user and
  to the clash tests.
- **A screen takes its colors from the theme.** `no_inline_colors` (in `wyck`) fails on a `0xRRGGBB`
  in this crate, except in `dashboard/marks.rs` and `dashboard/marks/`, whose brand colors are the
  point of the files.
- **Preferences are changed through the workspace** (`edit_preferences`, `appearance::update`,
  `indicators::update_prefs`), never by writing a file; saving is `wyck-app`'s business.
- **A backup import or a folder move is staged, not done.** The page records what waits
  (`PendingSummary`, a gpui global) and `wyck` applies it at the next start, before anything reads
  the documents it replaces.
- **Sending and closing orders is `wyck-ui-trading`'s path**: the dashboard only routes
  `TicketEvent` and `PanelEvent` and puts the lines on the charts.
- **No `[]` on a slice or a vector**: the workspace denies it. A control that picks by position
  uses `Page::ALL.get(index)` and does nothing out of range.

## Global state

`keymap::DEFS` and `HANDLED` are statics (constant tables). `wyck_ui_kit::hints` is filled by
`keymap::apply`. `settings_hub` sets one gpui global, `PendingSummary`, so that a reopened window
still says what waits for the next start.

## Tests

34 unit tests next to the code: the symbol catalog (classes, search, ordering), the details sheet,
the marks of symbols, the keymap (default keys valid and without clash, what the user chose, fixed
keys) and the shortcut page.
The windows are not unit tested. Run: `cargo test -p wyck-ui-shell`.

## Where to add X

- A shortcut: its action in the `action` module of the screen that owns it, its row in
  `keymap::DEFS` (or `HANDLED` if a screen reads the key itself, then the file in
  `keymap_guard::READS_KEYS`).
- A settings page: a file in `settings_hub/`, its `Page` and its tab in `settings_hub.rs`.
- A button in the header: `dashboard/header.rs`; what it does belongs in the screen crate.
- A symbol class or a logo: `dashboard/catalog.rs` (the class) and the module of the class in
  `dashboard/marks/` (the logo, with its license next to the file in `wyck-ui-kit/assets/marks`).
- A new screen: its own crate under `wyck-ui-*`, then hosted here.

## Do not

- Do not put the logic of a screen here: a chart, a ticket or an editor behaves the same wherever it
  is hosted, so it lives in its crate and the dashboard only wires its events.
- Do not reach the network, the files or the system: go through `wyck-app`.
- Do not widen a visibility: `pub` is for what `wyck` calls.
