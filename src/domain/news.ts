/** Calendrier économique ForexFactory (semaine en cours), avec cache disque journalier. */

import { join } from "node:path";
import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Data, Effect } from "effect";
import { z } from "zod";
import { APP_DATA_DIR } from "../constants.ts";

const CALENDAR_URL = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
const CACHE_PATH = join(APP_DATA_DIR, "ff-calendar.json");

// Le calendrier est toujours raisonné en heure de Paris, indépendamment du fuseau système —
// autant pour l'affichage (NewsPanel) que pour la limite "un jour" du cache ci-dessous. Utiliser
// le fuseau système ici serait incohérent avec l'affichage si l'app tourne ailleurs qu'à Paris.
export const PARIS_TZ = "Europe/Paris";
export const parisDayKeyFormat = new Intl.DateTimeFormat("en-CA", {
  timeZone: PARIS_TZ,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

const CalendarEventSchema = z.object({
  title: z.string(),
  country: z.string(),
  date: z.string(),
  impact: z.string(),
  forecast: z.string(),
  previous: z.string(),
});

/** Schéma décoré du `timestamp` dérivé (epoch ms, cf. `fetchCalendar`) — sert à la fois à valider
 * le cache disque (qui stocke les events déjà décorés) et à dériver `CalendarEvent`, pour ne pas
 * retaper les 6 champs de `CalendarEventSchema` une deuxième fois dans une interface à part. */
const DecoratedCalendarEventSchema = CalendarEventSchema.extend({ timestamp: z.number() });
export type CalendarEvent = z.infer<typeof DecoratedCalendarEventSchema>;

const NewsImpactSchema = z.enum(["high", "medium", "low"]);
export type NewsImpact = z.infer<typeof NewsImpactSchema> | "other";

export function classifyImpact(raw: string): NewsImpact {
  const parsed = NewsImpactSchema.safeParse(raw.trim().toLowerCase());
  return parsed.success ? parsed.data : "other";
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
  events: z.array(DecoratedCalendarEventSchema),
});

/**
 * Échec de récupération du calendrier — réseau, HTTP non-200, ou payload inattendu. Une seule
 * classe plutôt qu'une hiérarchie taguée par cause : rien en dehors de la boucle de retry
 * (`fetchCalendar` ci-dessous) ne discrimine jamais par cause, seulement "est-ce un rate-limit
 * retryable, et avec quel délai" — porté par `retryAfterSeconds` (défini = oui). Même convention
 * que `CtraderMcpError` (ctrader/client.ts), pour la même raison.
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

/**
 * Le calendrier ("cette semaine") ne change quasiment pas d'un jour à l'autre — un fetch par jour
 * calendaire suffit largement et évite le rate limit du serveur. `force: true` (commande /refresh)
 * bypasse le cache same-day, mais retombe quand même sur les données en cache si le réseau échoue.
 *
 * Retry : sur 429, réessaie en respectant le `retry-after` renvoyé par le serveur (jusqu'à
 * `MAX_RATE_LIMIT_RETRIES` fois de plus) ; toute autre erreur (HTTP non-200, payload invalide,
 * réseau down) n'est pas retentée — pas de valeur à réessayer une 404 ou un JSON cassé
 * immédiatement.
 */
export function fetchCalendar(
  force = false,
): Effect.Effect<CalendarEvent[], FetchCalendarError, FileSystem.FileSystem> {
  // Description d'une tentative unique, réexécutée telle quelle par la boucle de retry ci-dessous
  // (un Effect est une description pure, rejouable — pas une Promise déjà résolue une fois pour
  // toutes) : pas besoin d'un helper séparé pour "refaire la même chose une fois de plus".
  const attempt: Effect.Effect<CalendarEvent[], FetchCalendarError> = Effect.gen(function* () {
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

  return Effect.gen(function* () {
    const cached = yield* readCache();
    if (
      !force &&
      cached &&
      parisDayKeyFormat.format(new Date(cached.fetchedAt)) === parisDayKeyFormat.format(new Date())
    ) {
      return cached.events;
    }

    let result = yield* Effect.either(attempt);
    for (
      let retries = 0;
      result._tag === "Left" &&
      result.left.retryAfterSeconds !== undefined &&
      retries < MAX_RATE_LIMIT_RETRIES;
      retries++
    ) {
      yield* Effect.sleep(`${result.left.retryAfterSeconds} seconds`);
      result = yield* Effect.either(attempt);
    }

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
