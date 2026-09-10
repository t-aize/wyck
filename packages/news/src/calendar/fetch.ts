/**
 * Récupération du calendrier ForexFactory (semaine en cours).
 *
 * Un fetch par jour calendaire Paris suffit : le flux « this week » bouge peu.
 * `force: true` (commande `refresh` côté app) bypasse le cache same-day, mais
 * retombe quand même sur les données en cache si le réseau échoue — mieux vaut
 * de la donnée périmée qu'un panneau vide.
 */

import { join } from "node:path";
import type { FileSystem } from "@effect/platform";
import { Data, Effect, Schedule } from "effect";
import { z } from "zod";
import { CACHE_FILE, readCache, writeCache } from "./cache.ts";
import { type CalendarEvent, CalendarEventSchema } from "./schemas.ts";
import { parisDayKeyFormat } from "./time.ts";

const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
const MAX_RATE_LIMIT_RETRIES = 3;

/**
 * Échec de récupération — réseau, HTTP non-200, ou payload inattendu.
 * Une seule classe : rien ne discrimine par cause hors du retry 429, porté par
 * `retryAfterSeconds` (défini = oui, réessayer).
 */
export class FetchCalendarError extends Data.TaggedError("FetchCalendarError")<{
  readonly message: string;
  /** Présent uniquement sur un HTTP 429 : délai suggéré par le serveur (ou 5 s). */
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
  Schedule.addDelay((error) => `${error.retryAfterSeconds ?? 5} seconds`),
  Schedule.zipLeft(Schedule.recurs(MAX_RATE_LIMIT_RETRIES)),
);

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

/**
 * Charge le calendrier de la semaine, avec cache same-day (Paris).
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
    if (
      !force &&
      cached &&
      parisDayKeyFormat.format(new Date(cached.fetchedAt)) === parisDayKeyFormat.format(new Date())
    ) {
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
