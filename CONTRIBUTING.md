# Contributing to Wyck

Wyck is developed by a single maintainer. **Pull requests are not accepted.**

Issues are welcome: bug reports, feature requests, and questions. Please use the issue templates
and search existing issues first.

Never post API keys, tokens, or account credentials in an issue. For security problems, see
[SECURITY.md](SECURITY.md).

## Using AI to write issues

Using AI is fine, but every issue form asks you to say so: whether AI was involved, which tool and
model, and how you verified its output yourself. You must reproduce and check everything you
report. AI output can be confident and wrong.

A workflow reads those fields and labels the issue `ai-assisted` or `ai-generated`. If AI is
mentioned but the tool or the verification is missing, the issue gets `ai-unverified`. Issues
labeled `ai-unverified` or `needs-info` are closed after about three weeks without a reply.

## Local setup (maintainer)

Requirements: Bun 1.4+ and Node.js 22.10+ (see `.node-version`).

```bash
bun install
bun run dev
```

| Script                                  | Purpose                                              |
| --------------------------------------- | ---------------------------------------------------- |
| `bun run dev`                           | Start the dev server on <http://localhost:3000>      |
| `bun run build`                         | Create a production build                            |
| `bun run start`                         | Serve the production build                           |
| `bun run check`                         | Biome lint, format and import sorting                |
| `bun run check:fix`                     | Same as `check`, applying safe fixes                 |
| `bun run typecheck`                     | Generate Next.js types, then run `tsc --noEmit`      |
| `bun run verify`                        | `check`, `typecheck` and `build`: everything CI runs |
| `bun run clean`                         | Remove build output (`.next`, `out`, `coverage`)     |
| `bun run deps:outdated` / `deps:update` | List / upgrade dependencies to the latest            |

## Editor setup

Biome is the only linter and formatter, so keep Prettier and ESLint out of your editor for this
project.

- **VS Code**: install the recommended extensions (`.vscode/extensions.json`). The TypeScript 7
  extension (`TypeScriptTeam.native-preview`) is required for TypeScript IntelliSense, because
  `.vscode/settings.json` turns the built-in language service off in its favor.
- **Zed**: install the Biome extension and the TypeScript 7 extension
  ([`zed-extensions/tsgo`](https://github.com/zed-extensions/tsgo), shown as "TypeScript Language
  Server"). `.zed/settings.json` falls back to `vtsls` if the latter is missing. Tasks are in
  `.zed/tasks.json`.

## License

Licensed under the [Apache License 2.0](LICENSE).
