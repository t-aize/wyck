# Modules

| Module | Layer | Owns |
|---|---|---|
| `domain::appearance` | domain | the color palette and the look put in force together with it |
| `domain::chart` | domain | chart data, timeframes, view and transform, projection, scene commands, export model |
| `domain::drawings` | domain | drawing objects, geometry, the drawing book, the position tool |
| `domain::indicators` | domain | built-in studies, their math and catalog, the Rhai script engine, the registry of compiled scripts |
| `domain::market` | domain | bars, ticks, quotes, symbols, price scale and pip, trading hours, live bar repair |
| `domain::trading` | domain | positions, orders and deals, the account book, contract math, margin, order guard, exit plans |
| `infra::ctrader` | infra | cTrader Open API: wire, transport, session, OAuth, market, account, trading, margin |
| `infra::platform` | infra | async runtime, build facts |
| `infra::storage` | infra | paths, documents, backups, secrets, tokens, the scripts folder |
| `app::account` | app | the account entity: positions, orders, trading calls, notices |
| `app::alerts` | app | alert rules, evaluation, sounds and notifications |
| `app::appearance` | app | theme choice, UI scale, fonts, saved and handed to the UI as a look |
| `app::broker`, `app::storage`, `app::system` | app | what the UI may use of `infra` |
| `app::drawings` | app | the live copy of the drawings |
| `app::market_data` | app | history loading and the live price hub |
| `app::prefs` | app | one saved document per feature |
| `app::scripts` | app | the indicator scripts folder, read in the background |
| `app::sign_in` | app | the sign-in state machine and its steps |
| `app::token_store` | app | adapter between the session and the secret store |
| `app::updates` | app | the self-update check and install |
| `app::workspace` | app | saved layouts, preferences, the saver |
| `ui::kit` | ui | tokens, theme, controls, forms, menus, modal, toast, shortcut text, window bar fallback |
| `ui::shell` | ui | window root, sign-in modal, dashboard (header, symbol picker, layout menu, editor dock, trade glue), settings pages |
| `ui::features::chart` | ui | the chart view, drawing tools and properties, studies settings, export |
| `ui::features::multichart` | ui | several charts in one layout, favorites, drawing toolbar |
| `ui::features::trading` | ui | order ticket, account panel |
| `ui::features::indicators` | ui | the script editor |
