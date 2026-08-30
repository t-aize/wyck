/** Calendrier économique ForexFactory (semaine en cours), avec cache disque journalier — I/O
 * déléguée à `storage/jsonFile.ts` (cf. ce fichier pour la discipline lecture/écriture partagée). */

import { join } from "node:path";
import type { FileSystem } from "@effect/platform";
import { Data, Effect, Schedule } from "effect";
import { z } from "zod";
import { APP_DATA_DIR } from "../constants.ts";
import { readJsonFile, writeJsonFile } from "../storage/jsonFile.ts";
import { CacheFileSchema, type CalendarEvent, CalendarEventSchema } from "./schemas.ts";
import { parisDayKeyFormat } from "./time.ts";

const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
/** Anciennement `ff-calendar.json` — renommé pour rester compréhensible hors du contexte de ce
 * fichier (ex: en listant `~/.aurum/`) sans savoir que "ff" = ForexFactory. Un ancien fichier sous
 * l'ancien nom devient simplement orphelin (jamais relu) : le prochain `fetchCalendar` retombe sur
 * un cache absent et refait un fetch réseau, sans conséquence au-delà de ce fetch supplémentaire. */
const CACHE_PATH = join(APP_DATA_DIR, "calendar-cache.json");
const MAX_RATE_LIMIT_RETRIES = 3;

/**
 * Échec de récupération du calendrier — réseau, HTTP non-200, ou payload inattendu. Une seule
 * classe plutôt qu'une hiérarchie taguée par cause : rien en dehors de `rateLimitRetry`
 * ci-dessous ne discrimine jamais par cause, seulement "est-ce un rate-limit retryable, et avec
 * quel délai" — porté par `retryAfterSeconds` (défini = oui). Même convention que
 * `CtraderMcpError` (ctrader/client.ts), pour la même raison.
 */
export class FetchCalendarError extends Data.TaggedError("FetchCalendarError")<{
  readonly message: string;
  readonly retryAfterSeconds?: number;
}> {}

/**
 * Policy de retry pour `fetchOnce` : ne réessaie que tant que l'échec porte un `retryAfterSeconds`
 * (429), en respectant le délai suggéré par le serveur, jusqu'à `MAX_RATE_LIMIT_RETRIES` fois.
 * Toute autre erreur (HTTP non-200, payload invalide, réseau down) sort du `recurWhile` dès la
 * première tentative — pas de valeur à réessayer une 404 ou un JSON cassé immédiatement.
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
      // Délai suggéré par le serveur si présent et exploitable, sinon 5s par défaut.
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
 * Le calendrier ("cette semaine") ne change quasiment pas d'un jour à l'autre — un fetch par jour
 * calendaire suffit largement et évite le rate limit du serveur. `force: true` (commande /refresh)
 * bypasse le cache same-day, mais retombe quand même sur les données en cache si le réseau échoue.
 * Cache disque en lecture "jamais en échec" / écriture "échoue tel quel, avalée ici en best-effort" —
 * cf. storage/jsonFile.ts pour cette discipline partagée avec les autres stores de l'app.
 */
export function fetchCalendar(
  force = false,
): Effect.Effect<CalendarEvent[], FetchCalendarError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const cached = yield* readJsonFile(CACHE_PATH, CacheFileSchema);
    if (
      !force &&
      cached &&
      parisDayKeyFormat.format(new Date(cached.fetchedAt)) === parisDayKeyFormat.format(new Date())
    ) {
      return cached.events;
    }

    const result = yield* Effect.either(fetchOnce().pipe(Effect.retry(rateLimitRetry)));
    if (result._tag === "Right") {
      // Écriture cache best-effort : un échec d'écriture ne doit pas faire échouer le refresh.
      const payload = { fetchedAt: new Date().toISOString(), events: result.right };
      yield* Effect.ignore(writeJsonFile(APP_DATA_DIR, CACHE_PATH, payload));
      return result.right;
    }

    // Réseau en échec (rate limit épuisé, offline…) : mieux vaut de la donnée périmée qu'une erreur.
    if (cached) return cached.events;
    return yield* Effect.fail(result.left);
  });
}
