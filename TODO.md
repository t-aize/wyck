# TODO

What is left to do on Wyck, with the state of the project when this list was written
(2026-10-11, version 0.2.0, branch `refactor/structure`, pull request #31).

The `docs/` folder was deleted in #31. The old files (roadmap, architecture, conventions, storage,
testing, decision records) are in the history: `git show ada056c:docs/ROADMAP.md`, and so on.

## State

- `main` builds and its CI is green on Linux, macOS and Windows.
- #27 (gpui-kit 0.7.0) and #30 (dependencies at their latest versions; `RELEASING.md` names
  `v0.2.0`) are merged.
- #31 (structure pass) is open, without conflict, and its CI is running. It contains: typed
  errors (no `Result<_, String>` and no `unwrap` or `expect` outside tests), more than thirty
  long files split by responsibility, real paths instead of re-exports, unit tests back inside the
  files they test, and the removal of `docs/`.
- Checked locally on the branch: `cargo fmt --check`, `cargo clippy --all-targets --all-features
  -- -D warnings`, `cargo test --all-features` (1235 tests), `cargo doc --no-deps` with
  `-D warnings`, `cargo deny check`.
- The app has not been started on a screen since the restructuring. Everything below marked "by
  hand" is untested on a screen.
- No `v0.2.0` tag and no release yet. The last GitHub release is `v0.1.7` (built by the old
  chain, assets named `aurum-...`). The tag `v0.1.8` exists without a release.
- The tag `pre-restructure` (commit `c805e65`, last commit before the restructuring) is pushed.

## Order of work

1. Pull request #31: wait for the CI, merge it.
2. Start the app and check it by hand (section 2).
3. Repository settings on GitHub (section 3).
4. Tag `v0.2.0` (section 4).
5. Check the release (section 5).
6. The rest, when there is time (section 6).

## 1. Merge #31

- [ ] The CI of #31 is green on the three systems (`check` on ubuntu, macos and windows, `fmt`,
      `ci-ok`).
- [ ] Merge it: `gh pr merge 31 --squash --delete-branch` (or the button on GitHub).
- [ ] Update the local `main`: `git checkout main && git pull`, then delete the local branch
      `refactor/structure`.
- [ ] The CI of `main` is green after the merge.

## 2. Check the app by hand

Use a cTrader demo account. Nothing here can be proved by the tests.

Smoke checklist (every step):

- [ ] Start with a saved profile: the dashboard opens without a prompt.
- [ ] Start with no saved profile: the sign-in completes and lands on the dashboard.
- [ ] Open a chart, change the symbol and the timeframe, scroll and zoom.
- [ ] Add a built-in indicator, change an input, change its style, remove it.
- [ ] Draw a trend line and a rectangle (`Alt+X`), undo, redo, delete.
- [ ] Open the symbol picker (`Ctrl+K`) and the tool finder (`Ctrl+Shift+F`).
- [ ] Open the order ticket, pick a side (`Alt+B`, `Alt+S`), place a demo order, close it.
- [ ] Create a price alert and let it fire.
- [ ] Open the settings (`Ctrl+,`), change the theme and the interface scale.
- [ ] Export a chart image.
- [ ] Cut the network for a minute and restore it: the session reconnects.
- [ ] Disconnect from the account menu and sign in again.

Screens touched by the structure pass (the code moved, the behavior must not have):

- [ ] Order ticket: error messages (no price, stop loss on the wrong side, exits that do not add up
      to 100%, no symbol), sizing by risk, an OCO pair, sending an order.
- [ ] Indicator settings: inputs, style and display pages.
- [ ] Indicator script editor: tabs, save, add to chart, rename, delete, import, export.
- [ ] Chart export dialog: data, columns, format, presets and preview pages; export and copy.
- [ ] Chart settings and drawing properties dialogs.
- [ ] Account panel: tabs, filters, row menus, protection dialog, alert dialog.
- [ ] Multichart: layouts, links, clipboard of indicators and settings, drawing tool bars.
- [ ] Dashboard header: timeframes (favorites, custom ones), account menu, panel toggles.
- [ ] Backup: save, export, import, reset (they apply at the next start).

Window and platform checks (from the restructuring):

- [ ] Windows 11, macOS, X11 and GNOME Wayland: the system title bar shows "Wyck" ("Wyck (dev)"
      in debug builds), its buttons work, the window can be dragged and cannot go below 900 x 600.
- [ ] A Wayland desktop without server-side decorations: `ui::kit::window_bar::fallback` draws a
      minimal bar.
- [ ] The dashboard bars changed height when the 34 px bar was removed (`BARS` in
      `dashboard/trade.rs`, the layout menu, the symbol picker): nothing is cut or overlapping.
- [ ] Windows: Explorer and the taskbar show the app icon (`assets/app-icon.ico` through
      `build.rs` and the `winresource` crate). The icon files and `assets/logo.png` come from
      `assets/brand/` through `scripts/make_brand_assets.py` (needs Python and Pillow). The mark
      shows on the first sign-in step and on the About page.
- [ ] Sign-in form: the secret field is masked and Enter submits.
- [ ] A confirmation opened from a settings panel (reset, delete) shows over the panel and the
      panel is back after Cancel. With gpui-kit 0.7 the root now draws dialogs and notices above
      everything, so look at this one with care.
- [ ] Shortcut hints read `Cmd` and `Option` on a Mac.
- [ ] Buttons of the account menu (Settings, Disconnect), the sign-in modal and the ticket line up;
      Disconnect asks for a confirmation.
- [ ] One menu at a time in the dashboard header (account, timeframes, layouts); Escape closes the
      chart's time zone menu.
- [ ] Dividers, splitters and swatches did not move by more than a pixel; field widths follow the
      interface scale.
- [ ] Linux: the AppImage and the `.deb` start (the deb depends on libasound2, libfontconfig1,
      libssl3, libwayland-client0, libxcb1, libxkbcommon0 and libxkbcommon-x11-0).

Optional, against a real server:

- [ ] `cargo test --test ctrader live -- --ignored --nocapture` (needs a `.env` made from
      `.env.example`; it refuses live accounts and is never run in CI).

## 3. Repository settings on GitHub

Today: `main` is not protected, secret scanning and push protection are off.

- [ ] Protect `main`: require the check `ci-ok`, require a pull request, no force push.
- [ ] Turn on secret scanning and push protection.
- [ ] Delete the obsolete secret `TAURI_SIGNING_PRIVATE_KEY` (left from the old Tauri chain).
- [ ] Turn on immutable releases (the release workflow creates a draft, uploads and attests every
      asset, then publishes).
- [ ] Optional: Dependabot security updates (off today). Version updates are configured in
      `.github/dependabot.yml` (weekly, grouped, with a cooldown).
- [ ] Keep a safe backup of the updater private key and its password. They are stored as the
      secrets `CARGO_PACKAGER_SIGN_PRIVATE_KEY` and `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD`
      (both exist). The public key is committed in `assets/update.pubkey`; replacing it stops
      the installed apps from accepting new updates.

## 4. Release `v0.2.0`

`RELEASING.md` describes the whole chain. The workflow `.github/workflows/release.yml` has never
run end to end: the first real run is this one. Plan for a failure and for correction tags
(`v0.2.1`, ...). A tag cannot be reused on an immutable release.

Before the tag:

- [ ] Section 1 and section 2 are done, the CI of `main` is green.
- [ ] `Cargo.toml` says `0.2.0` and `Cargo.lock` agrees (`cargo check --locked`).
- [ ] The checks of `RELEASING.md`: `cargo fmt --check`, `cargo clippy --locked --all-targets
      --all-features -- -D warnings`, `cargo test --locked --all-features`, `cargo deny check`,
      and one local `cargo build --release --locked` if you accept the time (fat LTO is slow; the
      CI builds it with relaxed settings).
- [ ] Look at the About page in a debug and in a release build.

The tag:

- [ ] `git tag -s v0.2.0` then `git push origin v0.2.0` (a signed tag needs a signing key; use
      `git tag -a` otherwise). The workflow refuses a tag that is not `vX.Y.Z` or does not match
      the package version.
- [ ] Wait for the four package jobs (Windows NSIS; macOS Intel and Apple Silicon `.app` and `.dmg`;
      Linux AppImage and deb) and the publish job. Do not create the release by hand for the same
      tag.

## 5. After the release

- [ ] The release has all the assets: `Wyck_0.2.0_windows-x86_64-setup.exe`, the two macOS `.dmg`,
      the Linux `.AppImage` and `.deb`, the updater archives and signatures, `latest.json`,
      `SHA256SUMS`.
- [ ] `latest.json` is there. The app downloads
      `https://github.com/t-aize/wyck/releases/latest/download/latest.json` to look for an
      update, and the old releases have none.
- [ ] Verify the download: `sha256sum -c SHA256SUMS` and `gh attestation verify . --repo
      t-aize/wyck` (see `RELEASING.md`).
- [ ] Install on a clean machine, and over an existing config on each system.
- [ ] Updater test: serve a signed manifest from a temporary endpoint and check that the current
      version gives no notice, a stable newer version is offered and a prerelease is ignored, a bad
      signature is refused, a valid update saves its state, installs and restarts, and the deb and
      raw Linux binaries open the release page instead of replacing themselves.
- [ ] The README table of downloads names the right files.
- [ ] `RELEASING.md` says the first release of this chain is `v0.2.0`; keep it right if the number
      changes.

## 6. Later

### Conventions to restore

- [ ] `docs/` is gone with the conventions written for the structure pass. Put the useful part back
      in `AGENTS.md` (a few lines): files stay under about 600 lines apart from data tables; a split
      module is `foo.rs` plus `foo/` with `pub mod` children, callers use the real path, no
      `pub use` to hide a move, the root qualifies its calls (`axes::dash_pattern(..)`), `mod` and
      `use` lines at the top; unit tests stay in the file that holds most of the code they test;
      one `thiserror` enum per module; no `unwrap` or `expect` outside tests.
- [ ] Decide whether to keep a short architecture note (the four layers are already in
      `AGENTS.md` and in the `AGENTS.md` of each layer).

### Code

- [ ] 22 files still have more than 600 lines of code (40 counting their tests). The largest, in
      lines of code: `infra/ctrader/transport/connection.rs` 766, `app/alerts/model.rs` 766,
      `domain/drawings/figures.rs` 751, `ui/features/trading/ticket/view.rs` 746,
      `app/prefs/ticket.rs` 742, `ui/kit/text_input.rs` 742, `domain/indicators/custom/docs.rs`
      733 (a table), `app/sign_in.rs` 719, `domain/chart/transform.rs` 701,
      `ui/shell/settings_hub/look.rs` 696, `ui/features/chart/study_settings.rs` 696,
      `domain/indicators/kind.rs` 689, `domain/chart/scene.rs` 688. Split them when they change.
- [ ] Some root files carry many tests because the tests share a fixture:
      `domain/drawings/book.rs` (about 1500 lines), `domain/drawings/geometry.rs` (about 1100),
      `domain/chart/export.rs`, `domain/chart/scene.rs` (about 1600).
- [ ] OAuth `state` is optional in the redirect (`infra/ctrader/auth/callback.rs`,
      `parse_redirect`) because the portal's documentation does not say it echoes the parameter.
      Make it mandatory after a live sign-in confirms the echo.
- [ ] The pip guessed from decimals (`market::pip_size_from_digits`) is used only before a contract
      is read; every other pip comes from `domain::market::pip_size`. Remove the guess if it is
      never needed.
- [ ] Indicators: the prices of a second symbol are not wired (inputs of type symbol).
- [ ] Secrets: the app uses the OS keyring only. On a machine without one (Linux without a Secret
      Service) the tokens cannot be kept. `EncryptedFileSecretStore` exists and is tested but
      nothing in the app chooses it. Decide: a fallback with a passphrase prompt, or remove it.
- [ ] The Windows file race (`PermissionDenied` while a document is replaced) is handled by
      `fs_util::retry_when_busy` for the rename and the reads of documents, config, secrets, scripts
      and the staged import. Watch the Windows CI for a regression.
- [ ] The test `a_chart_computes_a_script_it_holds_through_the_config` failed once in a full run
      under load and passes alone (a Rhai time budget). Make its budget independent of the load.

### Product

- [ ] Accessibility has tests that need no screen but was never tried with NVDA, VoiceOver or
      Orca.
- [ ] Binaries are not signed: SmartScreen and Gatekeeper warn. See `RELEASING.md` for Windows
      (Authenticode certificate, `certificate-thumbprint`, timestamp URL in
      `[package.metadata.packager.windows]`) and macOS (Developer ID certificate as
      `APPLE_CERTIFICATE`, its password, `signing-identity`, notarization).
- [ ] Pull requests are not accepted yet (`README.md`, `CONTRIBUTING.md`). Decide when to open
      them.
- [ ] First release notes: nothing says yet what changed since `v0.1.7`.

### Housekeeping

- [ ] `target/` grows to about 18 GB: `cargo clean` when disk space is short.

## Decisions kept from the deleted `docs/`

- One Cargo package at the root, layers `domain` < `infra` < `app` < `ui` kept apart by
  `tests/architecture.rs` (its baseline is empty) instead of by crates. If a rebuild after one
  edit gets too slow, extract `domain` and then `infra::ctrader` back into crates.
- The records of the protocol (`Position`, `Order`, `Deal`) stay the single set of types. They are
  checked where they enter the app: `AccountBook` drops a record whose side it cannot read, and
  the enums of the protocol are built from their type, not from numbers. Risk sizing is the one
  formula `contract::quantity_for_risk`. The side is no longer read as a buy by default.
- Screens take every control, size and color from `src/ui/kit` (`tests/design_system.rs`), and key
  bindings are unique per context (`src/keymap_guard.rs`).
- The build-time baseline of the restructuring was never measured; the decision was dropped.
