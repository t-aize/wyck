# Adding things

| I want to add | Put it |
|---|---|
| A calculation (pip value, sizing, PnL, an indicator) | `src/domain/...` as a pure function with a unit test. The screen receives the result. |
| A built-in indicator | `src/domain/indicators/`: the math in `math.rs` or a new file, its spec and compute in `extended.rs` or the module root, its catalog entry in `catalog.rs`. |
| A drawing tool | model and geometry in `src/domain/drawings/`, the toolbar entry and properties in `src/ui/features/chart/`. |
| A cTrader message | the wire types in `src/infra/ctrader/` next to the existing ones in the same area (`market`, `account`, `trading`, `margin`), a client method in that area's `client.rs`, a test against the mock server in `tests/ctrader`. |
| A file the app keeps | a document through `infra::storage::DocumentStore`; version the schema. |
| App state shared by screens | an entity in `src/app/`. A screen never does I/O itself. |
| A screen or panel | `src/ui/features/<feature>/<name>.rs`; wire it from the feature root file. |
| A dialog | built from the kit's modal and form pieces, opened through `ui::kit::modal` (`open_over` for a confirmation above a panel). |
| A setting | a field in the owning document, a control in `src/ui/shell/settings_hub/`. |
| A key binding | next to its action's `actions!` block; `keymap_guard` rejects a duplicate in one context. |
| A color, size or spacing | a token in `src/ui/kit/tokens.rs` or `theme.rs`. Never a literal in a screen. |

Rules that apply to everything:

- New files use `foo.rs` plus a `foo/` folder for children, never `foo/mod.rs`.
- Errors are a `thiserror` enum per module, not `String`.
- No `unwrap` or `expect` outside tests.
- A comment says what the name and the type do not. See `conventions.md`.
