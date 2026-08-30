/**
 * I/O JSON partagé par les stores disque de l'app (cf. `settings.ts`, `trading/atrTradeStore.ts`,
 * `news/calendar.ts`) — tous dans `APP_DATA_DIR` (cf. constants.ts), tous avec la même discipline
 * qu'avant ce regroupement : lecture tolérante à tout (fichier absent, JSON invalide, format d'une
 * version antérieure -> `undefined`, jamais un échec — à l'appelant de fournir la valeur de repli),
 * écriture qui échoue "tel quel" (`PlatformError`) en laissant l'appelant décider s'il doit remonter
 * (settings : l'utilisateur doit savoir) ou être avalée (`Effect.ignore`, stores best-effort).
 *
 * Fichiers volontairement séparés malgré ce code d'I/O commun : centraliser le *code* ne veut pas
 * dire centraliser le *fichier physique*. Un fichier par domaine évite qu'une écriture en
 * arrière-plan (ex: le refresh ATR toutes les 60s, cf. useAtrAutoRefresh.ts) ne puisse jamais entrer
 * en collision avec une autre (settings, calendrier) et en écraser une des deux — écrire dans un
 * seul fichier partagé réintroduirait exactement le risque que cette séparation évite.
 */

import { FileSystem } from "@effect/platform";
import type { PlatformError } from "@effect/platform/Error";
import { Effect } from "effect";
import type { z } from "zod";

/** `undefined` si le fichier est absent, illisible, JSON invalide, ou ne respecte pas `schema` — un
 * store disque corrompu ou d'un format antérieur n'est pas plus fatal qu'un fichier qui n'existe pas
 * encore (premier lancement). `schema` valide la forme *avant* que l'appelant n'en fasse quoi que ce
 * soit (ex: déchiffrer un champ) — un champ manquant ou du mauvais type est détecté ici, pas plus
 * loin sous une forme moins lisible. */
export function readJsonFile<T>(
  path: string,
  schema: z.ZodType<T>,
): Effect.Effect<T | undefined, never, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    const raw = yield* fs.readFileString(path).pipe(Effect.orElseSucceed(() => undefined));
    if (raw === undefined) return undefined;

    return yield* Effect.try(() => schema.parse(JSON.parse(raw))).pipe(
      Effect.orElseSucceed(() => undefined),
    );
  });
}

/** Crée `dir` s'il n'existe pas encore puis écrit `data` en JSON indenté dans `path`. `mode`
 * optionnel pour un fichier sensible (cf. settings.ts, 0o600 — les autres stores n'en ont pas
 * besoin, leur contenu n'est pas secret). */
export function writeJsonFile(
  dir: string,
  path: string,
  data: unknown,
  options?: { mode?: number },
): Effect.Effect<void, PlatformError, FileSystem.FileSystem> {
  return Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem;
    if (!(yield* fs.exists(dir))) {
      yield* fs.makeDirectory(dir, { recursive: true });
    }
    yield* fs.writeFileString(path, JSON.stringify(data, null, 2), options);
  });
}
