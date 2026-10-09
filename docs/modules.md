# Modules

| Module | Layer | Owns |
|---|---|---|
| `domain::chart` | domain | chart data, timeframes, view and transform, projection, scene commands, export model |
| `domain::drawings` | domain | drawing objects, geometry, the drawing book, the position tool |
| `domain::indicators` | domain | built-in studies, their math and catalog, the Rhai script engine and its library |
| `infra::ctrader` | infra | cTrader Open API: wire, transport, session, OAuth, market, account, trading, margin |
| `infra::platform` | infra | async runtime, build facts, self-update |
| `infra::storage` | infra | paths, documents, backups, secrets, tokens, indicator script store |
| `app::alerts` | app | alert rules, evaluation, sounds and notifications |
| `app::appearance` | app | theme choice, UI scale, fonts |
| `app::token_store` | app | adapter between the session and the secret store |
| `app::workspace` | app | saved layouts, preferences, the saver |
| `ui::kit` | ui | tokens, theme, controls, forms, menus, modal, toast, window bar fallback |
| `ui::shell::connection` | ui | the sign-in flow (to be replaced by the sign-in modal) |
| `ui::shell::dashboard` | ui | header, symbol picker, layout menu, editor dock, trade glue |
| `ui::shell::settings_hub` | ui | the settings pages |
| `ui::features::chart` | ui | the chart view, drawing tools and properties, studies settings, export |
| `ui::features::multichart` | ui | several charts in one layout, favorites, drawing toolbar |
| `ui::features::trading` | ui | order ticket, account panel, account state (to move to `app`) |
| `ui::features::indicators` | ui | the script editor and its library |
