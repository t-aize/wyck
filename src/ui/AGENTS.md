# ui

gpui rendering. Read the layer table in `docs/architecture.md` and `docs/ui-design-system.md`.

- `kit` is the design system and the only place that names gpui-kit components. A screen builds
  buttons, fields, menus and dialogs with `ui::kit` constructors, sizes with `tokens` and colors
  with `theme`. No `Button::new`, no literal `px(..)` in a screen (`tests/design_system.rs`
  counts them and the count only goes down).
- `features/<name>` imports `domain`, `app`, `kit` and a lower feature (chart < multichart < the
  others). Features do not import `infra`.
- `shell` assembles features: the window root, the dashboard, the sign-in modal, the settings.
- Key bindings sit next to the actions they bind; `src/keymap_guard.rs` fails on a duplicate in
  one context.
- Bindings use `secondary` (Cmd on macOS, Ctrl elsewhere). A hint is written with "Ctrl" and
  passed through `ui::kit::shortcut::text`, which says "Cmd" on a Mac.
