/**
 * Cache disque du calendrier : un fichier JSON par installation, dans le
 * `cacheDir` fourni par l'app (`~/.aurum/` côté Aurum).
 *
 * Lecture **jamais en échec** (fichier absent, JSON cassé, schéma d'une
 * version antérieure → `undefined`) ; écriture qui échoue « tel quel »
 * (`PlatformError`) — `fetchCalendar` l'avale en best-effort pour ne pas
 * faire rater un refresh réseau à cause d'un disque plein.
 */

import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect } from "effect";
import { type CacheFile, parseCacheFile } from "./schemas.ts";

/** Nom du fichier, volontairement stable — un rename orphelinise l'ancien cache
 * (un fetch réseau de plus, pas de perte fonctionnelle). */
export const CACHE_FILE = "calendar-cache.json";

/**
 * Lit et valide le cache. `undefined` = absent ou illisible : l'appelant
 * retombe sur un fetch, pas sur une erreur.
 */
export function readCache(
  path: string,
): Effect.Effect<CacheFile | undefined, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const raw = yield* fs.readFileString(path).pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined) return undefined;
    return yield* Effect.try(() => parseCacheFile(JSON.parse(raw))).pipe(
      Effect.orElseSucceed(() => undefined),
    );
  });
}

/**
 * Crée `dir` au besoin puis écrit le JSON indenté. Contrairement à `readCache`,
 * un échec d'écriture **remonte** — c'est à l'appelant d'ignorer ou non.
 */
export function writeCache(
  dir: string,
  path: string,
  data: CacheFile,
): Effect.Effect<void, PlatformError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    if (!(yield* fs.exists(dir))) {
      yield* fs.makeDirectory(dir, { recursive: true });
    }
    yield* fs.writeFileString(path, JSON.stringify(data, null, 2));
  });
}
