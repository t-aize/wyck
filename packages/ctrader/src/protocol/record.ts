/**
 * Record JSON permissif — positions ({@link CtraderPositionSchema}) et résultats
 * d'écriture ({@link CreateOrderResultSchema}…).
 *
 * Un `z.object` strict qui se trompe (champ optionnel d'une classe d'actif jamais
 * testée) ferait *planter* l'affichage (`safeParse` → {@link CtraderMcpError}),
 * pire pour un panel de trading que des valeurs possiblement fausses mais
 * visibles. Un `.transform()` post-parse (jamais un `.refine()`, qui pourrait
 * faire échouer le parse) projette ensuite le record sur la forme utile, champ
 * par champ, chacun `undefined` plutôt qu'une erreur.
 */

import { z } from "zod";
import type { TradeSide } from "./enums.ts";

/** Objet JSON quelconque : accepte tout, ne rejette jamais. */
export const PermissiveRecordSchema = z.record(z.string(), z.unknown());

/**
 * Lit `keys[0]`, sinon `keys[1]`… dans un record non typé.
 *
 * @returns la première valeur numérique trouvée, ou `undefined` si aucune clé
 * n'est présente / n'a le bon type — plutôt qu'une exception.
 */
export function readNumber(record: Record<string, unknown>, keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number") return value;
  }
  return undefined;
}

/**
 * `tradeSide` du record → {@link TradeSide}, ou `undefined` si absent / ni BUY ni SELL.
 */
export function readTradeSide(record: Record<string, unknown>): TradeSide | undefined {
  const value = record.tradeSide;
  return value === "BUY" || value === "SELL" ? value : undefined;
}
