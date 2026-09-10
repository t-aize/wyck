/** Helpers de parsing partagés par les commandes de `src/commands/`. */

import { roundPrice } from "../utils/priceMath.ts";

/**
 * Coercion `string -> number fini`, `undefined` si vide/absent/non numérique — remplace l'ancien
 * détour par `effect/Schema` (`commands.ts` historique, `Schema.NumberFromString` + `Schema.filter`)
 * pour ce qui n'est qu'une conversion, pas une validation de données assez riche pour mériter un
 * schéma déclaratif. Seule façon de parser un nombre dans `src/commands/` — contrairement à l'ancien
 * fichier, qui en avait deux (ce détour Schema ici, `Number()` brut dans `resolveCancelTargets`).
 */
export function parseFiniteNumber(raw: string | undefined): number | undefined {
  if (raw === undefined || raw.trim() === "") return undefined;
  const value = Number(raw);
  return Number.isFinite(value) ? value : undefined;
}

/** Comme `parseFiniteNumber`, avec arrondi à la précision API du symbole et rejet des prix ≤ 0. */
export function parsePrice(raw: string | undefined, digits = 2): number | undefined {
  const value = parseFiniteNumber(raw);
  if (value === undefined || value <= 0) return undefined;
  return roundPrice(value, digits);
}

/** Parse un flag prix optionnel (`--sl`/`--tp`) : absent → `{}`, invalide → `{ error }`. */
export function parseOptionalPrice(
  raw: string | undefined,
  label: string,
  digits = 2,
): { value?: number; error?: string } {
  if (raw === undefined) return {};
  const value = parsePrice(raw, digits);
  if (value === undefined) return { error: `${label} invalide : "${raw}"` };
  return { value };
}

/**
 * Parseur `--flag valeur` / `-f valeur` (style CLI, ordre libre) — utilisé par `amend` (`trade` est
 * passé à des arguments positionnels stricts, plus de flags). `aliases` mappe une clé logique vers
 * ses formes acceptées.
 *
 * Une valeur qui est elle-même un flag connu (ex. `amend 1 --sl --tp 10`) est refusée comme "valeur
 * manquante" plutôt que silencieusement absorbée comme prix — trou de validation de l'ancienne
 * version.
 */
export function parseFlags(
  args: string[],
  aliases: Record<string, string[]>,
): Record<string, string> | string {
  const flagToKey = new Map<string, string>();
  const knownFlags = new Set<string>();
  for (const [key, flags] of Object.entries(aliases)) {
    for (const flag of flags) {
      flagToKey.set(flag, key);
      knownFlags.add(flag);
    }
  }

  const result: Record<string, string> = {};
  for (let i = 0; i < args.length; i += 2) {
    const flag = args[i];
    const key = flag && flagToKey.get(flag);
    if (!key) return `option inconnue : "${flag ?? ""}"`;
    const value = args[i + 1];
    if (value === undefined || knownFlags.has(value)) return `valeur manquante pour ${flag}`;
    result[key] = value;
  }
  return result;
}
