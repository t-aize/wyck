# Testing

## Automated

- `cargo test` runs unit tests (inside the modules) and integration tests (`tests/`).
- `cargo test --test live_ctrader -- --ignored` talks to a real cTrader demo account. It needs a
  `.env` (see `.env.example`), refuses live accounts, and is never run in CI.
- `cargo test keymap_guard` fails when two actions share the same keys in one context.

## Manual smoke checklist

Run it after every phase that touches startup, windows, or the UI, on a demo account.

1. Start the app with a saved profile: it reaches the dashboard without a prompt.
2. Start it with no saved profile: the sign-in flow completes and lands on the dashboard.
3. Open a chart, change symbol and timeframe, scroll and zoom.
4. Add a built-in indicator, change an input, change its style, remove it.
5. Draw a trend line and a rectangle (`Alt+X`), undo, redo, delete.
6. Open the symbol picker (`Ctrl+K`) and the tool finder (`Ctrl+Shift+F`).
7. Open the order ticket, pick a side (`Alt+B`, `Alt+S`), place a demo order, close it.
8. Create a price alert and let it fire.
9. Open settings (`Ctrl+,`), change the theme and the UI scale.
10. Export a chart image.
11. Cut the network for a minute and restore it: the session reconnects.
12. Disconnect from the account menu and sign in again.

## To verify by hand [a verifier]

- Window title bar behavior on Windows 11, macOS, X11, GNOME Wayland (phase 7).
- Masked secret field and Enter key in the sign-in form (phase 11).
