# wyck-app

The application: the layer between `wyck-engine` and the screen, and a GPUI window with the
connection screens. `cargo run -p wyck-app` opens it.

```text
ui (screens, title bar, theme)  <---  shell (window, plumbing)  <---  application layer  <---  wyck-engine
```

## Run

```sh
cargo run -p wyck-app
```

On a first run there is no saved account, and the window opens on "Connect to cTrader":

- **Local session** looks for cTrader Desktop on this machine and needs no token. cTrader's local
  MCP server must be on: in cTrader Desktop, Settings > MCP Server > Enable MCP server. Its default
  address is `http://127.0.0.1:9876/mcp/`; if you changed the port there, set
  `WYCK_LOCAL_ENDPOINT`.
- **Remote** takes the token from cTrader Web (Settings > Remote MCP). Each trading account has its
  own token. The token is checked against `mcp.ctrader.com` before anything is saved.

When a connection succeeds, the account is saved (its token in the OS credential store, never in
the config file) and becomes the active one, so the next start connects by itself and goes straight
to the dashboard. If that automatic connection fails, the window lands on the screen that
explains why. The "switch connection" button of the dashboard goes back to the first screen.

Without the saved account, `WYCK_SERVICE`, `WYCK_ENDPOINT` and `WYCK_TOKEN` connect from the
environment instead (development and demo accounts, nothing is saved). All variables are listed in
`src/settings.rs`. Logs go to a daily file under the data directory (`%APPDATA%\wyck\...\logs` on
Windows), and to stderr in a debug build.

Global shortcuts work while another program has the focus:

| Keys (default) | Effect |
|---|---|
| `ctrl+alt+b` | Plan and submit a **dry-run** buy with the default risk and stop |
| `ctrl+alt+s` | The same for a sell |
| `ctrl+alt+p` | Calls the `on_toggle_panel` hook (nothing by default) |

**Orders sent from a shortcut are dry runs.** Nothing reaches the broker, and the controller refuses
to run at all while the engine is armed. Real orders need an arming flow, which does not exist yet
(`TODO.md` 6.1).

## The screens

Seven screens make the connection flow, in `src/ui/screens.rs`, driven by `src/flow.rs`:

| Screen | When |
|---|---|
| Connect to cTrader | First run, or after "Switch connection" |
| Looking for cTrader Desktop | While the local search runs |
| cTrader Desktop detected | The local session answered: shows its address, version if it gives one, and account |
| Couldn't find cTrader Desktop | Nothing answered: a checklist, the address tried, a link to the setup page |
| Connect with a token | The token form: masked field, show and paste buttons, Enter to connect |
| Verifying your token | While the token is checked |
| That token didn't work | The token was refused, or the server could not be reached: says which |

Once connected, the window lands on the **dashboard** (`src/ui/dashboard.rs`). For now it is its
header and an empty chart area:

- the traded symbol (tile, ticker, long name), its latest bid, an arrow for the direction of its
  last move (the figure takes the color of the move for a moment), and the spread in pips;
- the time frame selector (1m, 5m, 15m, 1H, 4H, 1D), remembered for the chart to come;
- the account kind (DEMO, LIVE or UNKNOWN ACCOUNT), the mode (DRY RUN or ARMED) and, when it is not
  ready, the session state;
- the buttons: full screen and switch connection work; indicators and the full workspace layout are
  dimmed until they exist.

All of it comes from the engine. There is no daily change next to the price yet, because the
engine has no candles: a made-up number would be worse than none. The header data is computed in
`src/dashboard.rs`, without a window.

Escape and the Back link go back one step. Going back during a search or a check cancels it: a
session that opens after that is dropped, so leaving never leaves a hidden connection.

The account kind is never drawn as a demo unless it is known to be one. An account whose kind the
server does not report shows as "UNKNOWN ACCOUNT" in amber.

### Title bar

The window has no system title bar: `src/ui/titlebar.rs` draws it, with the logo, the version and
the minimize, maximize (restore) and close buttons. On Windows the buttons and the drag area are
registered as native control areas, so dragging, double-click to maximize, edge snapping and the
snap layouts flyout behave as in any Windows program. On Linux the bar starts the move itself.
macOS keeps its traffic lights.

### Motion

Everything that moves speaks one language, defined in `src/ui/motion.rs` (durations, curves,
distances) from desktop motion guidelines and the defaults of the component library:

| What | How |
|---|---|
| A screen changes | The old one fades and slides 12 px out in 110 ms (accelerating), then the new one fades and slides 18 px in over 280 ms (decelerating). Going deeper slides from the right, going back from the left. Clicks are ignored while it moves |
| A card fills in | Its pieces (mark, title, text, content, buttons) rise 8 px and fade in one after the other, 32 ms apart, so a card is in place in well under half a second |
| The mark on a card | Pops in: grows from 70 percent, goes slightly past, settles |
| Buttons, links, choices, window buttons | Their colors fade in 120 ms under the pointer instead of flipping; chevrons slide a few pixels toward where they lead |
| The token field | Its border and glow fade with the focus; it shakes sideways (three swings, 8 px, dying out) when the text is refused, and the message under it fades in |
| Toasts | Rise 10 px and fade in; fade out before they are removed, whether they expire or are dismissed |
| Banners | Open like a drawer the first time they show |
| Waiting | A spinning arc, two swelling rings, a breathing dot and a sliding bar |

What arrives decelerates, what leaves accelerates and takes about half the time, and nothing
spatial is linear. The animations are built on GPUI's motion functions (`Presence`, `transition`),
which follow the system's "Animation effects" setting: with it off, every screen and control is
shown in its final state at once. `WYCK_REDUCE_MOTION=on` or `off` overrides the system.

### Looking at a screen without going through the flow

In a debug build, `WYCK_PREVIEW=<name>` opens the window on that screen with sample data and
connects to nothing:

```sh
WYCK_PREVIEW=notfound cargo run -p wyck-app
```

The names are `choose`, `searching`, `found`, `notfound`, `token`, `verifying`, `refused`,
`unreachable`, `dashboard`. A release build ignores the variable.

### Size

The design was drawn for a 13 px body, which is small on a large screen. Every size (text, icons,
paddings, radii, the window itself) goes through `theme::sz`, which multiplies by one scale: **1.25
by default**. `WYCK_UI_SCALE` sets it, from 0.75 to 2 (`WYCK_UI_SCALE=1.5 cargo run -p wyck-app`).
The window never opens larger than 92 percent of the screen it opens on. Hairline borders stay
1 px.

### Size

The design was drawn for a 13 px body, which is small on a large screen. Every size of the content
(text, icons, paddings, radii) and the window itself go through `theme::sz`, which multiplies by one
scale: **1.25 by default**. `WYCK_UI_SCALE` sets it, from 0.75 to 2 (`WYCK_UI_SCALE=1.5 cargo run
-p wyck-app`). The title bar is not scaled: it keeps the size of the design. The window never opens
larger than 92 percent of the screen it opens on. Hairline borders stay 1 px.

### Fonts and icons

The whole application is set in Geist, tokens, addresses and figures included (with tabular
figures so numbers line up, no monospace font). The web design shipped Geist as WOFF2, which the
text engine on Windows cannot read, so the crate embeds the TTF files from the official Geist repository
(`assets/fonts`, SIL Open Font License, text in `assets/fonts/OFL.txt`). The icons are SVG files in
`assets/icons`, drawn with the same paths as the design. Both are compiled into the binary.

## Layers

Every module below `shell` is plain Rust, tested without a window.

| Module | Role |
|---|---|
| `settings` | What the application is configured with, from the environment |
| `startup` | From settings to a connection request (profile or environment), and saving the account |
| `flow` | The connection flow: which screen, what moves it on, token checks, stale results |
| `dashboard` | What the dashboard header shows: symbol, price, spread, time frames |
| `controller` | The use cases: start the engine, connect (local, remote, saved), disconnect, hotkey order |
| `presentation` | Engine state to formatted rows, badges and tones |
| `model` | The data the windows share: state, activity, notices, toasts, banners |
| `messages` | Errors and outcomes to user-facing notices, in one place |
| `hotkeys` | Global shortcuts: parsing, registration, key-repeat debouncing |
| `logging`, `session_marker` | Log files, and detection of a session that did not end cleanly |
| `shell` (feature `gui`) | The GPUI window, engine plumbing, shortcuts |
| `ui` (feature `gui`) | Drawing: theme, assets, motion, widgets, title bar, screens, the root view |

Notices raised by the application (an order result, an account that could not be saved) show as
toasts at the bottom right and expire by themselves, errors last longest. Problems that outlive a
notice (a crashed previous session, a shortcut that could not be registered) show as banners under
the title bar until dismissed.

## Without GPUI

The `gui` feature is on by default. `cargo build -p wyck-app --no-default-features` builds the
application layer alone, with no windowing toolkit in the build. GPUI is pinned to an exact
version, see [../../docs/gpui-dependency.md](../../docs/gpui-dependency.md).

## Not done yet

- Persisted engine configuration and a settings screen (`TODO.md` 4.4).
- Arming, and real orders from the keyboard (`TODO.md` 6.1).
- Editing or removing saved accounts: for now the last successful connection replaces the saved one
  of its kind.
- The trading views: positions, orders, ticket, news, activity.
