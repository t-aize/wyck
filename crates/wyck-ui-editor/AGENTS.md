# wyck-ui-editor

The editor of the indicator scripts, docked under the charts: a code editor (colors, line numbers,
folding, search, the names of the language offered while typing, help under the pointer, the
mistakes of the script underlined as they are made) with the list of the scripts, tabs, a console
for the problems and what the script printed, and a reference of every function. The language, its
checker and its help are `wyck-indicators`; the folder of scripts is `wyck-app::scripts`; this crate
is the screen around them.

Depends on: `wyck-app`, `wyck-drawings`, `wyck-indicators`, `wyck-ui-chart`, `wyck-ui-kit`, `gpui`,
`lsp-types` (the shape of a completion, a hover and a diagnostic), `serde` and `serde_json`,
`anyhow` (only because the completion provider it implements returns an `anyhow::Result`); `toml`
for a test.
Used by: `wyck-ui-shell` (the dock) and `wyck` (`init`).

## Module map

Everything is private except what the root exports: `IndicatorEditor`, `EditorEvent`, `init`,
`CONTEXT` and the eight key actions (`SaveScript`, `AddToChart`...).

| Module | What it holds |
|---|---|
| `lib.rs` | `IndicatorEditor`: the open documents, the active tab, the console, the sizes; the drafts of unsaved text; `init` (the language config, once at start). |
| `tabs` | Opening and closing tabs; what the check says about the text. |
| `files` | Saving, adding the script to the active chart, and following the folder on disk (`library_changed`). |
| `prompts` | Asking for a name; creating, duplicating, deleting and renaming scripts. |
| `actions` | What the toolbar and the menus do: jump to a problem, insert text, export and import. |
| `providers` | Hands the words of a script to the gpui editor: colors, folds, completion, hover. The thinking is in `wyck_indicators::custom::{lexer, assist}`. |
| `parts` (`toolbar`, `explorer`, `console`, `reference`) | What the editor looks like. |

## Invariants and traps

- **Saving writes the file and nothing else.** The app notices the change the way it does for a file
  edited in another program, so every chart that holds the script redraws. `files::library_changed`
  makes a tab follow the file when the tab has nothing of its own; a tab with unsaved text is marked
  `conflict` instead of being overwritten.
- **Unsaved text survives a restart.** The drafts (`indicator_editor_drafts`, saved through a
  `Saver`) are restored only when the scripts folder is the one they were written for.
- **The check runs `CHECK_DELAY` after the last key**, not on every key.
- **Positions are the protocol's.** A line or a column handed to the editor goes through `lsp_index`
  (a `u32`); a column found in a string is a byte index, so slice with `get`, never with `[..]`.
- **An answer can arrive after its tab is gone**: results of the library and of file dialogs go
  through `wyck_app::lost::lost`.
- **No `[]` on a slice or a vector**: the workspace denies it. Use `get`, a slice pattern or an
  iterator.

## Global state

`init` registers the language of the scripts and its auto-closing pairs with the kit
(`set_language_config`), once at start. Nothing else is global; the open documents belong to the
`IndicatorEditor` entity.

## Tests

4 unit tests: the round trip of the drafts (`lib.rs`) and the styles `providers` gives a range. The
rest needs a window. Run: `cargo test -p wyck-ui-editor`. The guards that read the sources of the
screens (`keymap_guard`, `no_inline_colors`) live in `wyck`; the explorer reads keys by hand (Escape
to give up naming a script) and is listed there.

## Where to add X

- A key: its action in `lib.rs` (module `action`), its binding in the keymap of `wyck-ui-shell`.
- A command in the toolbar or the file menu: `parts/toolbar.rs` or `parts/explorer.rs` for the
  button, `actions.rs`, `files.rs` or `prompts.rs` for what it does.
- A thing the language offers (a function, a help text): `wyck-indicators`, not here; the reference
  and the completion pick it up.

## Do not

- Do not put a rule of the language here: the lexer, the checker and the help are in
  `wyck-indicators`.
- Do not write to the scripts folder yourself: go through `wyck_app::scripts` (`on_library`).
- Do not widen a visibility: `pub` is for what `wyck-ui-shell` and `wyck` call.
