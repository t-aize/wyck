# app

State and use cases: sign-in, account, alerts, preferences, market data, updates. Read the layer
table in `docs/architecture.md` first.

- Imports: `domain`, `infra`, `app`. gpui is allowed for `Entity`, `Context`, `Global` and tasks;
  never for elements, `Render` or gpui-kit components.
- The UI reaches the network, files and the OS only through `app::broker`, `app::storage` and
  `app::system`. Add a re-export there instead of importing `infra` from `ui`.
- A view holds an `Entity` of the state here and renders it; the state never knows its view.
- Settings of a feature are a document in `prefs/`, with their own schema version.
