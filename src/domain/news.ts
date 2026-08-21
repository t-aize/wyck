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

const CacheFileSchema = z.object({
  fetchedAt: z.string(),
  events: z.array(CalendarEventSchema.extend({ timestamp: z.number() })),
});

/**
 * Échec de récupération du calendrier — réseau, HTTP non-200, ou payload inattendu. Une seule
 * classe plutôt qu'une hiérarchie taguée par cause : rien en dehors de `fetchWithRetry` (ci-dessous)
 * ne discrimine jamais par cause, seulement "est-ce un rate-limit retryable, et avec quel délai" —
 * porté par `retryAfterSeconds` (défini = oui). Même convention que `CtraderMcpError`
 * (ctrader/client.ts), pour la même raison.
 */
export class FetchCalendarError extends Data.TaggedError("FetchCalendarError")<{
  readonly message: string;
  readonly retryAfterSeconds?: number;
}> {}

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
        new FetchCalendarError({
          message: `Calendrier économique : réseau indisponible (${cause instanceof Error ? cause.message : String(cause)})`,
        }),
    });

    if (response.status === 429) {
      // Délai suggéré par le serveur si présent et exploitable, sinon 5s par défaut — la valeur
      // finale posée ici une fois pour toutes, pas recalculée côté appelant (cf. fetchWithRetry).
      const retryAfterHeader = response.headers.get("retry-after");
      const parsedRetryAfter = retryAfterHeader ? Number(retryAfterHeader) : undefined;
      const retryAfterSeconds =
        parsedRetryAfter !== undefined && Number.isFinite(parsedRetryAfter) ? parsedRetryAfter : 5;
      return yield* Effect.fail(
        new FetchCalendarError({
          message: `Calendrier économique : limité par le serveur (réessai dans ${retryAfterSeconds}s)`,
          retryAfterSeconds,
        }),
      );
    }
    if (!response.ok) {
      return yield* Effect.fail(
        new FetchCalendarError({ message: `Calendrier économique : HTTP ${response.status}` }),
      );
    }

    const raw = yield* Effect.tryPromise({
      try: () => response.json(),
      catch: () => new FetchCalendarError({ message: "Calendrier économique : réponse non-JSON" }),
    });

    const parsed = z.array(CalendarEventSchema).safeParse(raw);
    if (!parsed.success) {
      return yield* Effect.fail(
        new FetchCalendarError({
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
 * `MAX_RATE_LIMIT_RETRIES` fois, `retryAfterSeconds` défini = c'est un rate-limit). Toute autre
 * erreur (HTTP non-200, payload invalide, réseau down) n'est pas retentée : pas de valeur à
 * réessayer une 404 ou un JSON cassé immédiatement. */
function fetchWithRetry(
  attemptsLeft = MAX_RATE_LIMIT_RETRIES,
): Effect.Effect<CalendarEvent[], FetchCalendarError> {
  return fetchOnce().pipe(
    Effect.catchTag("FetchCalendarError", (error) => {
      if (error.retryAfterSeconds === undefined || attemptsLeft <= 0) return Effect.fail(error);
      return Effect.sleep(`${error.retryAfterSeconds} seconds`).pipe(
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
