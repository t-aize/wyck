/** ATR/true range et filtre de displacement — extrait de trend.ts. */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { rollingMean, trueRangeSeries } from "./series.ts";

/** Série ATR(period) complète, une valeur par bougie — `undefined` tant que l'historique est plus
 * court que `period`. Exportée séparément de `computeAtr` (qui ne garde que la dernière valeur) car
 * structureEvents.ts a besoin de la série entière pour son filtre de displacement bougie par bougie. */
export function computeAtrSeries(bars: CtraderTrendbar[], period = 14): (number | undefined)[] {
  return rollingMean(trueRangeSeries(bars), period);
}

/** ATR(period) le plus récent, échelle brute x10^5 comme le reste de `domain/smc` (cf. commentaire
 * de tête de trading.ts pour la conversion vers un prix affiché). */
export function computeAtr(bars: CtraderTrendbar[], period = 14): number | undefined {
  return computeAtrSeries(bars, period).at(-1);
}

/** Bougie "molle" : range < displacementMult × ATR courant — pas assez d'order flow réel derrière
 * la cassure. Permissif (true) tant que l'ATR n'a pas assez de bougies pour être calculé, comme la
 * référence. */
export function hasDisplacement(
  bars: CtraderTrendbar[],
  i: number,
  atr: (number | undefined)[],
  displacementMult: number,
): boolean {
  const a = atr[i];
  if (a === undefined) return true;
  const candleRange = bars[i]!.high - bars[i]!.low;
  return candleRange >= displacementMult * a;
}
