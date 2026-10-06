# wyck-ui

- Real role: widget kit and theme for the app, built on `gpui-kit` 0.6.6 (a facade over `gpui-component` 0.6.6). Intended role (description): "Widget kit and theme shared by every screen". Not respected: 30 app files import `gpui_kit` directly.
- Type: lib, edition 2024, no features.
- Internal dependencies: none. Incoming: `wyck`.
- External dependencies: `gpui` (`gpui-pre`), `gpui-kit`, `serde`, `unicode-segmentation`. No unused one measured.
- Size: 19 files, 5,956 lines (about 4,833 lines of code). Largest: `color_picker.rs` 944, `text_input.rs` 739.
- Components: `button` (back, primary, ghost, secondary, action, icon), `controls` (segmented, chips, color_swatch, switch, toggle, width and dash pickers, tooltip), `field` (`SliderField`, text, icon_choice, check), `form` (frame, dialog, footer, group, page, field, `Row`, note, `form::Tab` rail), `menu`, `modal`, `confirm`, `toast`, `layout` (screen, card, badge, status_dot, divider), `number`, `text_input`, `color_picker`, `font_picker`, `icon`, `focus`, `anim`, `theme`, `tokens`.
- Missing: Select, Table, generic Tabs, Radio. The app rebuilds them (`trading/panel/view.rs:189,1118`, `dashboard/header.rs:365`, `panel/dialogs.rs:401`, `ticket/view/order.rs:9`, `indicators/editor/parts.rs:71`).
- Size scale: no enum of its own. `tokens::height::{tiny 20, compact 24, control 28, large 34}` times a global `SCALE: AtomicU32` (`tokens.rs:13`); gpui-component `Size` for buttons (XSmall 20, Small 24, Medium and Large 32). `button::primary` and `secondary` use `.large()`, so 32 vs the 28 token. `tokens::menu::*` widths are not scaled.
- Theme: `Colors` with 17 `u32` fields, `WYCK_DARK`, a global `RwLock`, accessors `bg()`, `fg()`, `surface()`, `accent()`; `theme::apply` copies the palette into gpui-component.
- Hard coded values: 39 literal `px(..)` outside `tokens.rs` and `theme.rs` (`form.rs:52,222` two identical 56 px headers, `form.rs:411`, `layout.rs:19-29`, `controls.rs:185,243`). Colors: only `color_picker.rs` (40 presets, white cursor edge, transparent hsla), which is legitimate.
- Internal redundancy: `controls.rs:39-84 strip` vs `field.rs:204-250 icon_choice`; `layout::card`, `menu::card`, `menu::panel`; `menu::separator` (`:521`) vs `layout::divider` (`:161`).
- Redundancy with the app: 4 `switch` copies, `card`, `tint`, `mono` (see `audit/20-ui.md`); two text input stacks (`TextInput` here vs gpui-kit `InputState`).
- Quality: 2 `unwrap` outside tests (`text_input.rs:643,653`); 0 `expect`, `panic!`, `println!`, `#[allow]`, TODO; globals: `RwLock` x4 in `theme.rs`, `AtomicU32` in `tokens.rs`, gpui globals in `color_picker.rs:204,209,227` and `modal.rs:131`.
- Errors: not applicable.
- Docs: doc plus comment about 13.3 % of code lines. Design rules exist only as comments (`tokens.rs:1-6`); no README and no design document. Useful comment: `theme.rs:122` (poisoned lock on a `Copy` value). Noise: `tokens.rs:28`, `theme.rs:76`.
- Tests: 21 `#[test]`; the scale test mutates global state and says so.
- Verdict: keep and strengthen. Add the size enum and tokens (T-080), the missing components (T-081), absorb the app copies, document in `docs/UI_GUIDELINES.md`, and enforce with `ui-check`.
