# wyck

- Real role: desktop binary (GPUI) that wires everything and also holds a lot of pure logic. Intended role: "Desktop trading terminal for cTrader".
- Type: bin (`default-run = "wyck"`), edition 2024, packager metadata for installers. Dependencies include `wyck-config`, `wyck-openapi` (feature `client`), `wyck-chart`, `wyck-ui`, `gpui`, `tokio`, `async-trait`, `serde`, `serde_json`, `tracing` and others. `thiserror` is not a dependency.
- Internal dependencies: outgoing to all four other crates; incoming none.
- Size: 118 files, 56,217 lines (about 50,347 lines of code). 22 files over 800 lines (see below).
- Module map:

| Module | Lines | Real role | Files without gpui |
|---|---|---|---|
| `chart/` | 16,057 | `Chart` entity, overlays, settings panels, export dialog, PNG, history loading, live feed | `live.rs` 247, `load.rs` 517, `lines.rs`, `drawing_props/position.rs`, `coordinates.rs`, `visibility.rs` |
| `trading/` | 15,770 | `Account` entity, risk guard, exit plans, ticket, account panel | `guard.rs` 565, `math.rs`, `plan.rs` 806, `panel/data.rs` 1223, `panel/prefs.rs`, `columns.rs`, `stats.rs`, `ticket/prefs.rs` 863, `templates.rs`, `ticket/view/*` except `positions.rs` |
| `dashboard/` | 6,109 | Main window: session, symbol catalog, header, picker, list editor, `trade.rs` | none |
| `indicators/` | 3,612 | Global script service, code editor | `prefs.rs`, `parts/reference.rs`, `parts/toolbar.rs` |
| `multichart/` | 3,388 | Multi-chart layout, drawing toolbar, favorites | `links.rs`, `split.rs` |
| `services/` | 3,116 | Alerts (model, evaluation, sound), updates, token store | `alerts/model.rs` 1030, `eval.rs` 489, `sound.rs` 526, `token_store.rs` |
| `settings_hub/` | 2,158 + 386 | Settings panels | `about`, `behaviour`, `charts`, `safety` |
| `connection/` | 1,983 | Welcome, credentials, browser, account selection, authorization | `rules.rs` |
| `workspace/` | 1,701 | Preferences, watchlists, layouts, `Saver` | `preferences.rs` 1126, `layouts.rs`, `saver.rs` |
| `appearance/` | 1,473 | Theme, presets, contrast, global `State` | `contrast.rs`, `presets.rs` |
| root | 850 | `main.rs`, `runtime.rs`, `title_bar.rs`, `assets.rs`, `build_info.rs` | |

- Public API: binary crate; `pub` is irrelevant except for cross-module visibility. `#[path]` modules in `main.rs:3,18,22`.
- Misplaced responsibilities (destination in brackets):
  - `trading/guard.rs` (`RiskPrefs`, `check`, `lock`, `DuplicateGuard`) and `trading/plan.rs` (`Label` encode and decode, `split_volume`, `oco_siblings`, `break_even_moves`, `catch_up`, `TimeStop`) [`wyck-trading`].
  - `trading/account.rs`: `ReverseTracker` (`:79-149`), `standing` (`:299-331`), `assess` (`:344-371`), `assess_batch` (`:1047`), `fingerprint` (`:203`), `missed_exits` (`:1492-1598`) [`wyck-trading`].
  - `trading/math.rs`: `SizeMode`, `Offset`, `Scale`, `format_money` [next to `Contract`].
  - `trading/panel/stats.rs:107-121` CSV helpers [`wyck-chart` export].
  - `services/alerts/{model,eval}.rs` [`wyck-chart` or `wyck-indicators`].
  - `chart/load.rs` (history trait and tests with a fake client), `chart/live.rs` (subscription planner), `chart/raster.rs` 793 lines and `chart/paint.rs` pure part [`wyck-chart`].
  - `appearance/{contrast,presets}` [`wyck-ui::theme`].
  - `workspace/preferences.rs` and the `prefs.rs` models [config documents or a prefs module]; `preferences.rs` imports `crate::trading::*`.
- Order path: one file calls `TradingClient` (`trading/account.rs`, through `Account::trade` `:888`); details and the list of UI entry points in `10-cross-cutting.md`. Gaps: close, cancel and amend are not risk checked, `reduces: false` is hard coded (`:368`), live flag is display only, no audit log, confirmation rules scattered.
- Connection usage: `runtime.rs` builds a multi-thread runtime and leaks it with `mem::forget` (`:25-30`); `runtime::spawn` returns a `JoinHandle` awaited inside `cx.spawn`; `Session::start` runs under `runtime::handle().enter()` (`connection/mod.rs:155-158`). 60 `.detach()` calls. Owned tasks: `Account._poll` (`account.rs:189`), `Service._poll` (`indicators/mod.rs:49`), chart clock (`chart/mod.rs:535`), alerts poll (`alerts/mod.rs:125`). In-flight network calls are not abortable; stale replies are filtered by `epoch` and `call_seq`. `follow_session` ignores `RecvError::Lagged` (`dashboard/mod.rs:468`).
- State storage: entities (`Account`, `Alerts`, `MultiChart`, `Workspace`), view fields (`Dashboard.conn`, `catalog`, `details`), globals and statics listed in `20-ui.md`. Quote caches in 4 places (`account.rs:161`, `alerts/mod.rs:247`, `dashboard/mod.rs:143`, `chart/history.rs:352`).
- Degraded states: `Conn` enum (`dashboard/mod.rs:118`) shows Connected, Connecting, Reconnecting, Disconnected in the header (`header.rs:604-620`); `Failed` replaces the body (`mod.rs:806`). `Reconnecting` does not disable panels or the ticket; `trading/` never reads `Conn`. Account panel has `Loading` and `Failed` (`panel/view.rs:1575`); symbol catalog `Load::Failed` with retry (`header.rs:92-97`); chart `Loading` and `Failed` (`overlay.rs:1168-1182`). Stale quote indicator: not verified.
- Secrets: client secret typed into a masked `TextInput` (`credentials.rs:44`) then copied to a `String` (`:194`) for `ClientCredentials`; persisted via `config.set_profile_secret` (`authorizing.rs:136`) and tokens via `ConfigTokenStore`; plain copies through `expose_secret().to_string()` (`browser_handoff.rs:131`, `authorizing.rs:44`); no token or secret logging found.
- UI consistency: see `20-ui.md`.
- Internal redundancy: see `20-ui.md` and `10-cross-cutting.md` (`tint`, `mono`, half-volume computation at `panel/data.rs:442-445` and `ticket/view/positions.rs:48-50`, Buy and Sell text at `panel/data.rs:447-451`, `positions.rs:57-62`, `dashboard/trade.rs:145-154`, `trading/mod.rs:49-53,110-114`).
- External redundancy: CSV helpers vs `wyck-chart/src/export.rs:1541`; `format` in `wyck-ui/src/number.rs:166` vs `wyck-chart/src/format.rs:4`; raw to `f64` price conversion helper at `chart/lines.rs:60` unused by about 20 other sites.
- Dead code: no `#[allow(dead_code)]`. 9 `#[allow]` total: `presets.rs:24` and `ticket/mod.rs:622` (`too_many_arguments`), `eval.rs:111`, and 6 `unused_variables` in `multichart/drawing_ui.rs:558,643,727,858,902,1012`.
- Quality (outside tests): 0 `unwrap`; 5 `expect` (`main.rs` 2, `runtime.rs` 1, `connection/mod.rs` 2); 0 `panic!`, `unreachable!`, `todo!`; 0 `println!`; 0 TODO; `unsafe` forbidden. `Arc` and `Rc` 29, `RefCell` 2, Mutex or RwLock 9 lines (3 in tests). Std unbounded `mpsc` for sound jobs (`sound.rs:302`).
- Errors: `anyhow` only at `indicators/editor/providers.rs:165,206` (LSP trait); `Result<_, String>` in `chart/raster.rs:493`, `paint.rs:118`, `sound.rs:263,373,396`, `updates.rs:232`, `settings_hub/alerts.rs:216`, `ticket/mod.rs:1360,1634,1721`; `NameError` enum in `workspace/preferences.rs:544`; API errors are `wyck_openapi::Error`.
- Concurrency: `Arc<Mutex<Library>>` (`indicators/mod.rs:36`) locked inside background closures without `.await` while held (`:129`, `:258`; `:289` not read); `saver.rs:23` short sections; a per-second chart task (`chart/mod.rs:536`).
- Docs: 3110 lines of `///`, `//!`, `//` over 56,217 (5.5 %). Noise: `account.rs:373,601,786,884,997,1327`, `chart/export_ui.rs:63`. Useful: `account.rs:902-903,1373-1374`, `guard.rs:5-7`, `runtime.rs:25-29`.
- Tests: 247 `#[test]`: trading 79 (13 of 25 files), services 42, workspace 30, chart 25 (8 of 33), appearance 22, dashboard 20, indicators 11, multichart 11, connection 5, settings_hub 0. 40 files over 300 lines have no test (mostly views). Untested critical logic: `standing`, `assess`, `assess_batch`, `fingerprint`, `place_with`, `place_batch`, `missed_exits`, `catch_up`, `manage`, `enforce_time_stops`, `OrderTicket::order_with` and `orders` (`ticket/mod.rs:1640,1721`), `plan()` sizing at ticket level, all of `settings_hub/data.rs` (import, reset, backups). No `#[gpui::test]`; dev-dependencies only `tempfile`; `Account` is not testable without a window.
- 22 files over 800 lines: `ticket/mod.rs` 2150 (state, chart links, plan `:1329`, order building `:1640,1721`, send `:1852`, margin), `panel/view.rs` 1723, `account.rs` 1661, `export_ui.rs` 1448, `study_settings.rs` 1341, `multichart/mod.rs` 1316, `overlay.rs` 1266, `chart_settings_ui.rs` 1259, `panel/data.rs` 1223, `indicators/editor/mod.rs` 1157, `dashboard/mod.rs` 1130, `workspace/preferences.rs` 1126, `multichart/drawing_ui.rs` 1098, `drawing_props.rs` 1055, `alerts/model.rs` 1030, `chart/mod.rs` 954, `trade.rs` 946, `appearance/mod.rs` 898, `header.rs` 872, `ticket/prefs.rs` 863, `dialogs.rs` 849, `plan.rs` 806.
- Verdict: clean and split, no rewrite. Move pure logic out (M3), thin the `Account` entity (M4), migrate every screen to `wyck-ui` (M6), cut the 22 big files by responsibility.
