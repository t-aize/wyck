/** Calendrier économique ForexFactory (semaine en cours), avec cache disque journalier. */

import { join } from "node:path";
import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Data, Effect } from "effect";
import { z } from "zod";
import { APP_DATA_DIR } from "../constants.ts";

const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
const CACHE_PATH = join(APP_DATA_DIR, "calendar-cache.json");

// Le calendrier est toujours raisonné en heure de Paris, indépendamment du fuseau système —
// autant pour l'affichage (NewsPanel) que pour la limite "un jour" du cache ci-dessous. Utiliser
// le fuseau système ici serait incohérent avec l'affichage si l'app tourne ailleurs qu'à Paris.
export const PARIS_TZ = "Europe/Paris";
const parisDayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

export function parisDayKey(date: Date): string {
  return parisDayKeyFormat.format(date);
}

const CalendarEventSchema = z.object({
  title: z.string(),
  country: z.string(),
  date: z.string(),
  impact: z.string(),
  forecast: z.string(),
  previous: z.string(),
});

export interface CalendarEvent {
  title: string;
  country: string;
  date: string;
  impact: string;
  forecast: string;
  previous: string;
  /** epoch ms, dérivé de `date` */
  timestamp: number;
}

export type NewsImpact = "high" | "medium" | "low" | "other";

export function classifyImpact(raw: string): NewsImpact {
  const value = raw.trim().toLowerCase();
  if (value === "high" || value === "medium" || value === "low") return value;
  return "other";
}

/**
 * XAUUSD est coté en USD : les publications US sont structurellement les plus
 * corrélées. On marque aussi tout titre qui référence l'or explicitement
 * (rare dans ce calendrier, mais ça arrive : rapports miniers, réserves…).
 */
const GOLD_KEYWORD = /gold|precious metal|\bxau\b/i;

export function isGoldRelevant(event: Pick<CalendarEvent, "country" | "title">): boolean {
  return event.country === "USD" || GOLD_KEYWORD.test(event.title);
}

export type Direction = "up" | "down" | "flat";

/** "3.5%" → 3.5, "255K" → 255000, "-1.2M" → -1200000. */
function parseFigure(raw: string): number | undefined {
  const match = raw?.trim().match(/(-?[\d.,]+)\s*([KMB])?/i);
  if (!match) return undefined;
  const num = Number(match[1]!.replace(/,/g, ""));
  if (Number.isNaN(num)) return undefined;
  const multiplier = { K: 1e3, M: 1e6, B: 1e9 }[match[2]?.toUpperCase() as "K" | "M" | "B"] ?? 1;
  return num * multiplier;
}

/** forecast vs previous : le marché anticipe-t-il une lecture plus forte, plus faible, ou stable ? */
function figureDirection(
  event: Pick<CalendarEvent, "forecast" | "previous">,
): Direction | undefined {
  const forecast = parseFigure(event.forecast);
  const previous = parseFigure(event.previous);
  if (forecast === undefined || previous === undefined) return undefined;
  if (forecast === previous) return "flat";
  return forecast > previous ? "up" : "down";
}

/**
 * Heuristique de calendrier, pas un signal de trading : la plupart des indicateurs
 * US à fort impact (NFP, GDP, retail sales, PMI, CPI...) sont "pro-USD" — une lecture
 * anticipée plus forte que la précédente renforce le dollar, donc pèse sur XAUUSD
 * (corrélation inverse). Une poignée d'indicateurs "négatifs" (chômage, jobless
 * claims) vont dans l'autre sens : une hausse traduit un affaiblissement
 * économique, donc plutôt haussier pour l'or. Le marché intègre déjà une bonne
 * part du consensus, donc ceci reste indicatif, pas prédictif.
 */
const INVERSE_FOR_GOLD = /unemployment|jobless claims|claimant count/i;

export function goldDirection(event: CalendarEvent): Direction | undefined {
  const dataDirection = figureDirection(event);
  if (!dataDirection || dataDirection === "flat") return dataDirection;
  const inverse = INVERSE_FOR_GOLD.test(event.title);
  if (dataDirection === "up") return inverse ? "up" : "down";
  return inverse ? "down" : "up";
}

const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(CalendarEventSchema.extend({ timestamp: z.number() })),
});

// Erreurs taguées : le rate-limit distingue explicitement
// `retryAfterSeconds` — c'est cette valeur qui pilote maintenant le retry (avant, elle n'était
// qu'affichée dans le message sans jamais déclencher de nouvelle tentative).
export class CalendarRateLimited extends Data.TaggedError("CalendarRateLimited")<{
  readonly retryAfterSeconds: number | undefined;
  readonly message: string;
}> {}

export class CalendarHttpError extends Data.TaggedError("CalendarHttpError")<{
  readonly status: number;
  readonly message: string;
}> {}

export class CalendarInvalidPayload extends Data.TaggedError("CalendarInvalidPayload")<{
  readonly issues: string;
  readonly message: string;
}> {}

export type FetchCalendarError = CalendarRateLimited | CalendarHttpError | CalendarInvalidPayload;

/** `undefined` si absent, corrompu, ou d'un format antérieur — jamais en échec, on retombe sur un
 * fetch réseau dans tous les cas (comportement inchangé, juste routé par le canal Effect). Passe
 * par le service `FileSystem` (comme config.ts#readConfig) plutôt que `Bun.file` en direct — seul
 * point du code qui contournait encore ce service avant. */
function readCache(): Effect.Effect<
  { fetchedAt: string; events: CalendarEvent[] } | undefined,
  never,
  FileSystem.FileSystem
> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const raw = yield* fs.readFileString(CACHE_PATH).pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined) return undefined;

    return yield* Effect.try(() => CacheFileSchema.parse(JSON.parse(raw))).pipe(
      Effect.orElseSucceed(() => undefined),
    );
  });
}

/** Échoue tel quel (`PlatformError`) — c'est l'appelant (`fetchCalendar`, "écriture best-effort")
 * qui décide de l'ignorer, pas cette fonction elle-même (un seul point qui avale l'erreur, pas deux). */
function writeCache(
  events: CalendarEvent[],
): Effect.Effect<void, PlatformError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const payload = { fetchedAt: new Date().toISOString(), events };
    yield* fs.writeFileString(CACHE_PATH, JSON.stringify(payload, null, 2));
  });
}

const MAX_RATE_LIMIT_RETRIES = 3;

function fetchOnce(): Effect.Effect<CalendarEvent[], FetchCalendarError> {
  return Effect.gen(function* () {
    const response = yield* Effect.tryPromise({
      try: () => fetch(CALENDAR_URL),
      catch: (cause) =>
        new CalendarHttpError({
          status: 0,
          message: `Calendrier économique : réseau indisponible (${cause instanceof Error ? cause.message : String(cause)})`,
        }),
    });

    if (response.status === 429) {
      const retryAfterHeader = response.headers.get("retry-after");
      const retryAfterSeconds = retryAfterHeader ? Number(retryAfterHeader) : undefined;
      const validRetryAfter =
        retryAfterSeconds !== undefined && Number.isFinite(retryAfterSeconds)
          ? retryAfterSeconds
          : undefined;
      return yield* Effect.fail(
        new CalendarRateLimited({
          retryAfterSeconds: validRetryAfter,
          message:
            validRetryAfter === undefined
              ? "Calendrier économique : limité par le serveur"
              : `Calendrier économique : limité par le serveur (réessai dans ${validRetryAfter}s)`,
        }),
      );
    }
    if (!response.ok) {
      return yield* Effect.fail(
        new CalendarHttpError({
          status: response.status,
          message: `Calendrier économique : HTTP ${response.status}`,
        }),
      );
    }

    const raw = yield* Effect.tryPromise({
      try: () => response.json(),
      catch: () =>
        new CalendarInvalidPayload({
          issues: "réponse non-JSON",
          message: "Calendrier économique : réponse non-JSON",
        }),
    });

    const parsed = z.array(CalendarEventSchema).safeParse(raw);
    if (!parsed.success) {
      return yield* Effect.fail(
        new CalendarInvalidPayload({
          issues: parsed.error.message,
          message: `Calendrier économique : réponse inattendue (${parsed.error.message})`,
        }),
      );
    }

    return parsed.data
      .map((event) => ({ ...event, timestamp: new Date(event.date).getTime() }))
      .sort((a, b) => a.timestamp - b.timestamp);
  });
}

/** Réessaie sur 429 en respectant le `retry-after` renvoyé par le serveur (jusqu'à
 * `MAX_RATE_LIMIT_RETRIES` fois) — avant cette passe, ce délai était lu
 * et affiché mais jamais réellement utilisé pour patienter puis réessayer. Toute autre erreur
 * (HTTP non-200, payload invalide, réseau down) n'est pas retentée : pas de valeur à réessayer
 * une 404 ou un JSON cassé immédiatement. */
function fetchWithRetry(
  attemptsLeft = MAX_RATE_LIMIT_RETRIES,
): Effect.Effect<CalendarEvent[], FetchCalendarError> {
  return fetchOnce().pipe(
    Effect.catchTag("CalendarRateLimited", (error) => {
      if (attemptsLeft <= 0) return Effect.fail(error);
      return Effect.sleep(`${error.retryAfterSeconds ?? 5} seconds`).pipe(
        Effect.andThen(() => fetchWithRetry(attemptsLeft - 1)),
      );
    }),
  );
}

/**
 * Le calendrier ("cette semaine") ne change quasiment pas d'un jour à l'autre —
 * un fetch par jour calendaire suffit largement et évite le rate limit du
 * serveur. `force: true` (commande /refresh) bypasse le cache same-day, mais
 * retombe quand même sur les données en cache si le réseau échoue.
 */
export function fetchCalendar(
  options: { force?: boolean } = {},
): Effect.Effect<CalendarEvent[], FetchCalendarError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const cached = yield* readCache();
    if (
      !options.force &&
      cached &&
      parisDayKey(new Date(cached.fetchedAt)) === parisDayKey(new Date())
    ) {
      return cached.events;
    }

    const result = yield* Effect.either(fetchWithRetry());
    if (result._tag === "Right") {
      // Écriture cache best-effort : un échec d'écriture ne doit pas faire échouer le refresh.
      yield* Effect.ignore(writeCache(result.right));
      return result.right;
    }

    // Réseau en échec (rate limit épuisé, offline…) : mieux vaut de la donnée périmée qu'une erreur.
    if (cached) return cached.events;
    return yield* Effect.fail(result.left);
  });
}
