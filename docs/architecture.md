# Architecture

Wyck is one Cargo package. `src/lib.rs` declares four layers and `run()`; `src/main.rs` only
calls `wyck::run()`. Boundaries between layers are not enforced by the compiler, so a test does
it (see "Enforcement").

## Layers

Lower layers never import higher ones.

```
domain   pure: market data, indicators, drawings, chart models. No I/O, no gpui, no async runtime.
infra    I/O: cTrader client, files and secrets, OS services. Uses domain.
app      state and use cases: sign-in, account, alerts, preferences. Uses domain and infra.
ui       gpui rendering: kit (design system), shell (frame), features (one module per feature).
```

| Importer | May import |
|---|---|
| `domain` | `domain` |
| `infra` | `domain`, `infra` |
| `app` | `domain`, `infra`, `app` |
| `ui::kit` | `ui::kit` |
| `ui::features::X` | `domain`, `app`, `ui::kit`, itself, and a lower feature (chart < multichart < the others) |
| `ui::shell` | `domain`, `app`, `ui::kit`, `ui::features`, `ui::shell` |

Crate-level rules checked by the same test:

- `domain` never names gpui, tokio, reqwest, tokio-tungstenite, keyring, rodio, notify-rust,
  directories, or `std::{fs, net, env, process}`.
- `infra` never names gpui or gpui-kit. `app` never names gpui-kit.
- Only `ui::kit` names gpui-kit components. Screens use the kit.
- No `mod.rs`, no `#[path]`, no `use super::*` outside tests.

## Tree

```
src/
  lib.rs, main.rs
  domain/
    chart/        chart models: data, timeframe, view, transform, projection, scene, formats
    drawings/     drawing objects, geometry, the drawing book, the position tool
    indicators/   built-in studies, math, script engine (Rhai), catalog
  infra/
    ctrader/      cTrader Open API: wire, transport, session, oauth, market/account/trading/margin
    platform/     async runtime, build facts, self-update
    storage/      paths, documents, backups, secrets, tokens, indicator script store
  app/
    alerts/       alert rules, evaluation, sounds
    appearance/   theme choice, UI scale, fonts
    token_store/  adapter between the session and the secret store
    workspace/    saved layouts, preferences, the saver
  ui/
    kit/          design system: tokens, theme, controls, forms, menus, modal, toast
    shell/        connection flow, dashboard, settings hub
    features/     chart, trading, indicators, multichart
    assets.rs     embedded fonts and marks
tests/            integration tests: architecture, cTrader session, storage lifecycle
examples/         runnable cTrader and storage tools
docs/             this folder
```

Known deviations, tracked in `tests/architecture-baseline.txt`: some pure code still lives in
`infra::ctrader` (account book, contract math, market types) and `ui::features::trading`
(ticket math, guard, plan); `ui::features::trading::account` is state and belongs in `app`.
Each move shrinks the baseline.

## Enforcement

- `cargo test --test architecture` reads every file in `src/`, resolves each `use` and each
  `crate::` or `super::` path to a module, and compares layers with the table above.
- Violations that predate the rule sit in `tests/architecture-baseline.txt`. The test fails on a
  new violation and on a listed one that was fixed. After fixing some, run
  `BLESS=1 cargo test --test architecture` to rewrite the list.
- `src/keymap_guard.rs` fails when two actions share the same keys in one context.
- Visibility: default to `pub(crate)`; `pub` only for what `tests/` and `examples/` need.

## Where does a new thing go

See `adding-things.md`.
