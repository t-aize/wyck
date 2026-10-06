# Why agents get this repo wrong

Concrete, reproducible causes. Each example names the file where a coding agent is likely to make the wrong move.

## Ten traps

1. Three or four copies of the same settings row builders. An agent edits one and the others drift: `choice` in `crates/wyck/src/chart/settings_rows.rs:27`, `chart_settings_ui.rs:402`, `export_ui.rs:657`; `switch` in `settings_rows.rs:49`, `chart_settings_ui.rs:384`, `export_ui.rs:681`, `drawing_props.rs:803`; `number` in `settings_rows.rs:68` and `export_ui.rs:703`.
2. Re-exports hide where things live. `crates/wyck/src/trading/mod.rs:11` makes `trading::book` a `pub use` of `wyck_openapi::account::book`, and `trading/math.rs:4` re-exports `wyck_openapi::trading::contract::*`. `AccountBook` and `Contract` are searched in the wrong crate.
3. `#[path]` modules in `crates/wyck/src/main.rs:3,18,22` put `alerts`, `token_store` and `updates` at `crate::` level although the files sit under `services/`.
4. `chart` (module of the app) vs `wyck_chart` (crate). `chart/mod.rs:80` does `use wyck_chart::scene`; `chart/raster.rs` and `chart/paint.rs` render the scene inside the app.
5. Homonyms: `Plan` x4 (`ticket/mod.rs:122`, `chart/live.rs:81`, `alerts/sound.rs:137`, `chart/flow_sync.rs:258`), `Tone` x2 (`wyck-openapi/src/account/book.rs:24`, `trading/panel/data.rs:29`), `Kind` x3, `Tab` x3, `Load` x2, `Symbol`, `SymbolRef` and `market::Symbol`, `Quote` x3 (`dashboard/mod.rs:143`, `ticks.rs:40`, `export.rs:172`), `Account` as entity, `AccountBook`, `AccountInfo`, `AccountClient`.
6. Giant files that mix state, validation, rendering and network: `trading/ticket/mod.rs` 2150, `trading/panel/view.rs` 1723, `trading/account.rs` 1661, `wyck-chart/src/drawing/book.rs` 2740, `study/mod.rs` 2053.
7. Three color systems: `wyck_ui::theme`, `DesktopPalette` (`chart/mod.rs:823`) and `u32` drawing colors; the guard test `no_inline_colors` (`appearance/mod.rs:808-898`) skips `chart/`, so an agent can add hex there unnoticed.
8. `cfg` scattered by OS: font choice in `chart/export_ui.rs:65` and `indicators/editor/parts.rs:37`, sound backends in `services/alerts/sound.rs:166-247`, `main.rs:95`.
9. A dead parameter that reads as live: `assess` sets `reduces: false` (`trading/account.rs:368`) so the branch `if order.reduces` in `trading/guard.rs:253` never runs. An agent will assume close orders pass through the risk check; `Account::close_position` never calls it.
10. Confirmation rules are scattered: `confirm_close` (`trading/panel/prefs.rs:318`), `one_click` (`trading/ticket/mod.rs:1893`), none for chart drags (`dashboard/trade.rs:645,708`). An agent adding a new order action does not know which to copy.

Other traps: the unit of `Bar` and order prices (scaled `i64` vs `f64`) is not visible in the type, so conversion code is copied (`as f64 / PRICE_SCALE as f64` about 20 times); `StudyKind::ALL` must be kept in sync with `spec()` and `extended::spec_for` by hand and `extended.rs:439` panics when it is not; `wyck-openapi` exposes `transport` as `pub` for tests (`lib.rs:166-169`) so internals look like API.

## Missing levers

- No single verification command. CI runs fmt, clippy, test and doc separately; `scripts/check-config.sh` covers one crate only.
- No written architecture rule. `wyck-ui/src/lib.rs:5` states "Screens never configure a gpui-kit component" but 30 app files do.
- `AGENTS.md` holds only the git attribution rule and the writing style; `CONTRIBUTING.md` says "issues only" and has an empty code style section.
- No GPUI test support in use: `Account` needs a gpui `Context` and a concrete `Session`, so an agent cannot verify entity logic. Whether `#[gpui::test]` works with `gpui-pre 0.3.5` is open (T-008).
- Comments that mislead: `bars.rs:15` mentions "MCP servers" that do not exist in the repo; `profile.rs:34-40` says the profile knows nothing about cTrader but the struct holds `client_id`, `callback_port`, `account_id`.
- Central types without docs: `Timeframe` (`timeframe.rs:17`), `Series` (`data.rs:18`), `InputSpec` (`study/mod.rs:128`), `PlotSpec`, `AtrStop`.

## Positive anchors an agent can rely on

`#![forbid(unsafe_code)]`, no TODO, 0 `println!` in libraries, thorough tests in `wyck-openapi` and `wyck-chart`, deterministic study `compute`, atomic file writes in `wyck-config`.
