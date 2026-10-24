# Wyck

Wyck is a terminal trading panel for cTrader, built for speed, not for staring at it.

This repository is only the **website**: it lets people download the app and connect to it. The
trading terminal itself lives elsewhere. Do not build terminal or trading UI here.

Stack: Next.js 16 (App Router, Cache Components, React Compiler), React 19, TypeScript 7 (strict,
ES2025), Tailwind CSS 4, Biome, Bun.

## Commands

Use Bun for everything (`packageManager` is pinned in `package.json`).

- `bun run dev`: dev server on http://localhost:3000
- `bun run check:fix`: Biome lint, format and import sorting, with safe fixes
- `bun run typecheck`: `next typegen` then `tsc --noEmit`
- `bun run verify`: everything CI runs (`check`, `typecheck`, `build`). Run it before finishing a task.

Biome is only used through `biome check` (`check` / `check:fix`); there are no separate lint or
format scripts.

## Structure

- `src/app/`: routes, root layout, `fonts.ts` (next/font) and `globals.css` (Tailwind theme tokens).
- `src/components/`: UI. Server Components by default; `mobile-nav.tsx` and
  `hero/ascii-backdrop.tsx` are the only Client Components.
- `src/lib/ascii/`: the hero's ASCII landscape renderer, framework-agnostic (canvas 2D).
- `src/config/site.ts`: name, tagline, navigation and outbound links. Change links there only.

## Conventions

- All code, comments, docs, commit messages and UI text are in English.
- Solo project: the maintainer commits directly. Pull requests are not accepted; only issues are.
- Source lives in `src/`; the `~/*` alias maps to `src/*`.
- Server Components by default; add `"use client"` only where needed.
- Do not add `useMemo`/`useCallback` by reflex: the React Compiler is enabled.
- No `any`, no non-null assertions, no barrel files (enforced by Biome).
- Never commit secrets. Only `.env.example` is tracked; real values go in `.env.local`.
- Do not hand-edit `bun.lock` or `next-env.d.ts`.

<!-- BEGIN:nextjs-agent-rules -->

## This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->
