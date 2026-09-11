/**
 * Récupération du calendrier ForexFactory (semaine en cours).
 *
 * Source unofficial (`nfs.faireconomy.media`) : pas de SLA, schéma et
 * disponibilité peuvent changer. On timeoute, on parse event par event, et on
 * retombe sur le cache si le réseau lâche — mieux vaut de la donnée périmée
 * qu'un panneau vide.
 *
 * Cache à TTL court ({@link CALENDAR_CACHE_TTL_MS}) : les forecasts bougent
 * dans la journée. `force: true` (commande `refresh`) bypasse le TTL, mais
 * le cache reste un repli réseau.
 */

import { join } from "node:path";
import type { FileSystem } from "@effect/platform";
import { Data, Effect, Schedule } from "effect";
import { CACHE_FILE, readCache, writeCache } from "./cache.ts";
import { type CalendarEvent, decorateEvents } from "./schemas.ts";

/** Miroir communautaire du calendrier ForexFactory « this week » — pas une API officielle. */
export const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";

/** Durée de vie du cache disque. Aligné sur un poll app de quelques minutes. */
export const CALENDAR_CACHE_TTL_MS = 30 * 60_000;

/** Budget réseau d'un fetch (AbortSignal). */
export const CALENDAR_FETCH_TIMEOUT_MS = 10_000;

const MAX_RATE_LIMIT_RETRIES = 3;
const DEFAULT_RETRY_AFTER_SECONDS = 5;
/** Plafond : un `Retry-After: 3600` ne doit pas bloquer l'app une heure × 3. */
export const MAX_RETRY_AFTER_SECONDS = 30;

const USER_AGENT = "Aurum/0.2.0";

/**
 * Échec de récupération — réseau, timeout, HTTP non-200, ou payload inattendu.
 * Une seule classe : rien ne discrimine par cause hors du retry 429, porté par
 * `retryAfterSeconds` (défini = oui, réessayer).
 */
export class FetchCalendarError extends Data.TaggedError("FetchCalendarError")<{
  readonly message: string;
  /** Présent uniquement sur un HTTP 429 : délai suggéré (déjà plafonné). */
  readonly retryAfterSeconds?: number;
}> {}

/**
 * Retry uniquement tant que l'échec porte un `retryAfterSeconds` (429), en
 * respectant le délai serveur, jusqu'à {@link MAX_RATE_LIMIT_RETRIES} fois.
 * Une 404 ou un JSON cassé sort dès la première tentative.
 */
const rateLimitRetry = Schedule.recurWhile<FetchCalendarError>(
  (error) => error.retryAfterSeconds !== undefined,
).pipe(
  Schedule.addDelay((error) => `${error.retryAfterSeconds ?? DEFAULT_RETRY_AFTER_SECONDS} seconds`),
  Schedule.zipLeft(Schedule.recurs(MAX_RATE_LIMIT_RETRIES)),
);

function isAbortCause(cause: unknown): boolean {
  return cause instanceof Error && (cause.name === "AbortError" || cause.name === "TimeoutError");
}

function networkError(cause: unknown): FetchCalendarError {
  if (isAbortCause(cause)) {
    return new FetchCalendarError({
      message: `Calendrier économique : délai dépassé (${CALENDAR_FETCH_TIMEOUT_MS / 1000}s)`,
    });
  }
  return new FetchCalendarError({
    message: `Calendrier économique : réseau indisponible (${cause instanceof Error ? cause.message : String(cause)})`,
  });
}

/** `Retry-After` numérique, borné. Header date HTTP → repli 5 s. `0` / négatif → 5 s. */
export function clampRetryAfter(header: string | null): number {
  const parsed = header ? Number(header) : Number.NaN;
  if (!Number.isFinite(parsed) || parsed <= 0) return DEFAULT_RETRY_AFTER_SECONDS;
  return Math.min(parsed, MAX_RETRY_AFTER_SECONDS);
}

/** Cache encore utilisable sans refetch (TTL, pas le jour calendaire Paris). */
export function isCacheFresh(fetchedAt: string, now = Date.now()): boolean {
  const fetched = Date.parse(fetchedAt);
  if (!Number.isFinite(fetched)) return false;
  return now - fetched < CALENDAR_CACHE_TTL_MS;
}

function fetchOnce(): Effect.Effect<CalendarEvent[], FetchCalendarError> {
  return Effect.gen(function* () {
    const response = yield* Effect.tryPromise({
      try: () =>
        fetch(CALENDAR_URL, {
          signal: AbortSignal.timeout(CALENDAR_FETCH_TIMEOUT_MS),
          headers: { "user-agent": USER_AGENT },
        }),
      catch: (cause) => networkError(cause),
    });

    if (response.status === 429) {
      const retryAfterSeconds = clampRetryAfter(response.headers.get("retry-after"));
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

    const events = decorateEvents(raw);
    if (events === undefined) {
      return yield* Effect.fail(
        new FetchCalendarError({
          message: "Calendrier économique : réponse inattendue (pas un tableau)",
        }),
      );
    }

    return events;
  });
}

/**
 * Charge le calendrier de la semaine, avec cache à TTL.
 *
 * @param options.cacheDir - Dossier où écrire `calendar-cache.json` (l'app passe `APP_DATA_DIR`).
 * @param options.force - Ignorer un cache encore frais ; le cache reste un repli si le réseau lâche.
 */
export function fetchCalendar(options: {
  cacheDir: string;
  force?: boolean;
}): Effect.Effect<CalendarEvent[], FetchCalendarError, FileSystem.FileSystem> {
  const cachePath = join(options.cacheDir, CACHE_FILE);
  const force = options.force ?? false;

  return Effect.gen(function* () {
    const cached = yield* readCache(cachePath);
    if (!force && cached && isCacheFresh(cached.fetchedAt)) {
      return cached.events;
    }

    const result = yield* Effect.either(fetchOnce().pipe(Effect.retry(rateLimitRetry)));
    if (result._tag === "Right") {
      const payload = { fetchedAt: new Date().toISOString(), events: result.right };
      yield* Effect.ignore(writeCache(options.cacheDir, cachePath, payload));
      return result.right;
    }

    if (cached) return cached.events;
    return yield* Effect.fail(result.left);
  });
}
