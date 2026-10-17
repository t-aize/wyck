---
paths:
  - "crates/wyck-ui-*/**"
  - "crates/wyck/**"
---

# Screen crates and the binary

- A screen reads colors, sizes and fonts from `wyck_ui_kit` (`theme`, `tokens`); a color written as
  a literal in a screen fails `no_inline_colors` in `crates/wyck`.
- A key binding is declared once, in the keymap of `wyck-ui-shell`; `keymap_guard` in `crates/wyck`
  fails on `KeyBinding::new(` anywhere else.
- A screen does not name `wyck-ctrader`, `wyck-storage` or `wyck-platform`: it goes through the
  `broker`, `storage` and `system` doors of `wyck-app`. Only `wyck-ui-kit` names `gpui-kit`.
- `gpui::actions!` is called inside a module with
  `#[expect(clippy::derive_partial_eq_without_eq, reason = "...")]`, as the other screens do.
- The window cannot be opened without a signed-in account. Never type real credentials to see a
  screen; say what was only compiled and tested.
