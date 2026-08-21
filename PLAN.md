# Split `domain/news.ts` → `src/news/*.ts` + reimplement gold bias

## Context
`src/domain/news.ts` mixes 4 distinct concerns (zod schemas, fetch+cache+retry, relevance
filtering, time formatting) in one file. Following the same split already done for
`domain/commands.ts` → `src/commands/*.ts`, we move it to `src/news/*.ts` (top-level module,
dropping `domain/`). At the same time, we reimplement the "is this event bullish/bearish for
gold" feature that was deleted earlier this session (commit `be93b0f`) — the old version
conflated "did the data figure move up" with "is this bullish for gold" under one `Direction`
type, and defaulted every parseable indicator to a "direct" polarity except a tiny inverse
regex list, with no research behind that default. Two research passes (live fetch of the real
ForexFactory feed + macro literature on gold's real-yield/Fed-reaction-function driver) ground
the new version. Confirmed via live fetch: the feed **never** includes an `actual` field (not
even retroactively) — only `forecast`/`previous` are usable, same constraint as before.

## File split (`src/news/`)
- **`time.ts`** — `PARIS_TZ`, `parisDayKeyFormat` (moved verbatim + comment). Thin but genuinely
  shared between `calendar.ts` (same-day cache check) and `NewsPanel.tsx` (display).
- **`schemas.ts`** — `CalendarEventSchema`, `DecoratedCalendarEventSchema` (adds `timestamp`),
  `CalendarEvent` type (`z.infer`), `NewsImpactSchema`, `NewsImpact` type, `CacheFileSchema`.
  Mirrors `src/ctrader/schemas.ts`'s convention of keeping data shapes in one file.
- **`relevance.ts`** — `GOLD_KEYWORD`, `classifyImpact`, `isGoldRelevant` (moved verbatim).
- **`bias.ts`** (new) — the reimplemented feature:
  ```ts
  export type GoldBias = "bullish" | "bearish" | "neutral";

  function parseFigure(raw: string): number | undefined { /* same %/K/M/B regex as before */ }

  interface PolarityRule { pattern: RegExp; polarity: "direct" | "inverse" }

  // Only indicators with a real, research-backed directional call. CPI/PCE/PPI/Trade Balance
  // and all FOMC/Fed text events are deliberately absent (user confirmed: no badge, not a
  // guess) — the latter need no explicit exclusion since ForexFactory never populates
  // forecast/previous for them (verified live), so they fall through to `undefined` naturally.
  const POLARITY_TABLE: PolarityRule[] = [
    // direct: reading above forecast = hawkish Fed read = bearish gold
    { pattern: /non-?farm|\bnfp\b|\badp\b/i, polarity: "direct" }, // incl. ADP (user: same as NFP)
    { pattern: /\bgdp\b(?!.*price)/i, polarity: "direct" }, // exclude "GDP Price Index" (inflation, ambiguous)
    { pattern: /\bpmi\b/i, polarity: "direct" },
    { pattern: /retail sales/i, polarity: "direct" },
    { pattern: /average hourly earnings/i, polarity: "direct" },
    { pattern: /\bjolts\b/i, polarity: "direct" },
    { pattern: /consumer (confidence|sentiment)/i, polarity: "direct" },
    { pattern: /building permits|housing starts|existing home sales/i, polarity: "direct" },
    // inverse: reading above forecast = labor market weakening = dovish Fed read = bullish gold
    { pattern: /unemployment rate/i, polarity: "inverse" },
    { pattern: /jobless claims|claimant count/i, polarity: "inverse" },
  ];

  export function goldBias(event: Pick<CalendarEvent, "title" | "forecast" | "previous">): GoldBias | undefined {
    const rule = POLARITY_TABLE.find((r) => r.pattern.test(event.title));
    if (!rule) return undefined;
    const forecast = parseFigure(event.forecast);
    const previous = parseFigure(event.previous);
    if (forecast === undefined || previous === undefined) return undefined;
    if (forecast === previous) return "neutral";
    const readingUp = forecast > previous;
    const goldUp = rule.polarity === "direct" ? !readingUp : readingUp;
    return goldUp ? "bullish" : "bearish";
  }
  ```
  File-level comment must state: reads forecast-vs-previous (expected trend), not
  actual-vs-forecast (surprise) — the feed has no `actual` field, confirmed by live fetch;
  known limitation, not new. Also comment the `\bgdp\b(?!.*price)` exclusion (GDP Price Index
  is an inflation/deflator series, same ambiguous bucket as CPI, not a growth-direct series).
- **`calendar.ts`** — `CALENDAR_URL`, `CACHE_PATH`, `FetchCalendarError`, `readCache`,
  `writeCache`, `MAX_RATE_LIMIT_RETRIES`, `fetchCalendar` (moved unchanged, behavior identical
  to current file — imports schemas from `./schemas.ts`, `parisDayKeyFormat` from `./time.ts`).
- **`index.ts`** (barrel) — re-exports the full public surface:
  ```ts
  export { PARIS_TZ, parisDayKeyFormat } from "./time.ts";
  export type { CalendarEvent, NewsImpact } from "./schemas.ts";
  export { classifyImpact, isGoldRelevant } from "./relevance.ts";
  export type { GoldBias } from "./bias.ts";
  export { goldBias } from "./bias.ts";
  export { fetchCalendar, FetchCalendarError } from "./calendar.ts";
  ```
- Delete `src/domain/news.ts` once the split is verified working.

## Consumer updates
- **`src/ui/hooks/useCalendar.ts`**: only the import path changes, `"../../domain/news.ts"` →
  `"../../news/index.ts"`. Nothing else touches news.ts's surface in this file.
- **`src/ui/components/NewsPanel.tsx`**:
  - Import path change, plus add `type GoldBias, goldBias` to the import list.
  - Import `DOWN, FLAT, UP` from `../glyphs.ts` (still exported, still used elsewhere —
    PositionsPanel.tsx/PriceHeader.tsx — not orphaned, safe to reuse for this new badge).
  - Add a `BiasBadge` component (same slot/rendering style the old, deleted `DirectionBadge`
    used) mapping `bullish→green/UP`, `bearish→red/DOWN`, `neutral→textMuted/FLAT`, and
    rendering a blank 2-space span when `bias` is `undefined` (no confident call — CPI/PCE/PPI,
    Fed text events, unparseable figures).
  - Wire `<BiasBadge bias={goldBias(event)} />` into `NewsRow`, in the same position the old
    `DirectionBadge` occupied (after the country span, before the title span).
  - No changes to `isDefaultVisible`, `buildRows`, `formatFigures`, panel title strings — purely
    additive to the row.

## Confirmed decisions (user answered)
- CPI/PCE/PPI/Trade Balance: **no badge at all**, not a guess — these are the biggest prints in
  the feed but the direction is genuinely regime-dependent per research; showing nothing is more
  honest than a confident-looking wrong call.
- ADP Non-Farm Employment Change: **included**, same "direct" polarity as NFP (reasonable
  extrapolation — mechanically the same kind of figure, private vs. official payrolls).
- `neutral` (forecast == previous exactly) renders the `FLAT` glyph — a real, distinct state
  ("we have a call and it's flat") from `undefined` ("no call").

## Verified — nothing else needs updating
Only `useCalendar.ts` and `NewsPanel.tsx` import from `domain/news.ts` anywhere in `src/`
(confirmed by grep across all exported symbol names, not just the module path).
`FetchCalendarError` has zero external importers today (kept exported anyway, parity with
`CtraderMcpError`'s convention). `src/commands/refresh.ts` never imports news.ts directly — it
goes through `CommandContext.refreshNews`, untouched by this change. No test files or docs
reference `domain/news.ts`.

## Verification
1. `bun run typecheck` (tsc --noEmit) — must pass clean.
2. `bun run check` (biome) — must pass clean, run `check:fix` if only formatting diffs.
3. Manually sanity-check `goldBias` against a couple of real feed samples (e.g. a mocked
   Unemployment Claims event with previous < forecast should read "bullish"; an NFP event with
   forecast > previous should read "bearish"; a CPI event should return `undefined` regardless
   of its numbers; an FOMC Statement event with empty forecast/previous should return
   `undefined`).
4. `git status` / `git diff` review before any commit — do not commit until asked.
