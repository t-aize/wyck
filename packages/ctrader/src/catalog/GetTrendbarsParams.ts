import type { TrendbarPeriod } from "../protocol/TrendbarPeriod.ts";

/**
 * Params sortants de `get_trendbars`.
 *
 * Combinaisons valides côté serveur :
 *
 * - `(count)` → N dernières bougies ;
 * - `(toTimestamp, count)` → N bougies se terminant à `toTimestamp` ;
 * - `(fromTimestamp, toTimestamp)` → toutes les bougies sur la plage (≤ 720 h).
 *
 * En pratique **seule la 3e** s'est montrée fiable : les deux autres ont
 * renvoyé une 400 malgré une requête conforme au schéma annoncé.
 */
export interface GetTrendbarsParams {
  symbolId: number;
  period: TrendbarPeriod;
  /** ISO-8601, borne basse de la plage (avec `toTimestamp`). */
  fromTimestamp?: string;
  /** ISO-8601, borne haute de la plage. */
  toTimestamp?: string;
  /** Nombre de bougies — peu fiable seul, cf. commentaire d'interface. */
  count?: number;
}
