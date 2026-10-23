---
paths:
  - "crates/wyck-ui/**"
  - "crates/wyck/**"
---

# The screens and the binary

- A screen reads colors, sizes and fonts from `wyck_ui::kit` (`theme`, `tokens`); a color written as
  a literal in a screen fails the guard `tests/guards/colors.rs` of `wyck-ui`.
- A key binding is declared once, in the keymap of `wyck_ui::shell`; the guard `tests/guards/keymap.rs` of `wyck-ui`
  fails on `KeyBinding::new(` anywhere else.
- A screen does not name `wyck-ctrader`, `wyck-storage` or `wyck-platform`: it goes through the
  `broker`, `storage` and `system` doors of `wyck-app`. Only the `kit` module names `gpui-kit`.
- `gpui::actions!` is called inside a module with
  `#[expect(clippy::derive_partial_eq_without_eq, reason = "...")]`, as the other screens do.
- The window cannot be opened without a signed-in account. Never type real credentials to see a
  screen; say what was only compiled and tested.
