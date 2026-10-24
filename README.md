# Wyck

A terminal trading panel for cTrader, built for speed, not for staring at it.

[![CI](https://github.com/t-aize/wyck/actions/workflows/ci.yml/badge.svg)](https://github.com/t-aize/wyck/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

This repository is the **Wyck website**: it lets people download the app and connect to it. The
trading terminal itself is not part of this repository.

> Early stage: the site is currently a minimal Hello world.

## Stack

- [Next.js](https://nextjs.org) 16 (App Router, Cache Components, React Compiler)
- [React](https://react.dev) 19
- [TypeScript](https://www.typescriptlang.org) 7 in strict mode
- [Tailwind CSS](https://tailwindcss.com) 4
- [Biome](https://biomejs.dev) for linting and formatting
- [Bun](https://bun.sh) as package manager

## Getting started

Requirements: Bun 1.4+ and Node.js 22.10+.

```bash
bun install
bun run dev
```

Open <http://localhost:3000>.

## Scripts

| Script                | Description                                   |
| --------------------- | --------------------------------------------- |
| `bun run dev`         | Start the development server                  |
| `bun run build`       | Create a production build                     |
| `bun run start`       | Serve the production build                    |
| `bun run check`       | Biome lint, format and import sorting         |
| `bun run check:fix`   | Same as `check`, applying safe fixes          |
| `bun run typecheck`   | Generate Next.js types and run `tsc --noEmit` |
| `bun run verify`      | Run everything CI runs                        |
| `bun run deps:update` | Upgrade all dependencies to the latest        |

## Contributing

Pull requests are not accepted. Issues are welcome, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Working with AI agents

[`AGENTS.md`](AGENTS.md) holds the project instructions for coding agents. [`CLAUDE.md`](CLAUDE.md)
imports it so Claude Code reads the same file.

## License

Licensed under the [Apache License 2.0](LICENSE).
