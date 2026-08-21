/** Détection de fractale/swing — extrait de trend.ts, partagé par structuralTrend.ts,
 * structureEvents.ts et sweep.ts (les 3 méthodes qui en dérivent leur propre lecture). */

import type { CtraderTrendbar } from "../../ctrader/schemas.ts";

export interface FractalOptions {
  left: number;
  right: number;
}

export interface SwingSeries {
  highs: (number | undefined)[];
  lows: (number | undefined)[];
}

/**
 * Fractale symétrique à N bougies : un swing high à l'index i n'est marqué que si high[i] est le
 * maximum strict (première occurrence en cas d'égalité, comme `np.argmax`) de la fenêtre
 * [i-left, i+right]. Un swing n'est donc "connu" qu'une fois les `right` bougies suivantes closes
 * — jamais d'information du futur (pas de repaint).
 */
export function detectSwings(
  bars: CtraderTrendbar[],
  { left, right }: FractalOptions,
): SwingSeries {
  const n = bars.length;
  const highs: (number | undefined)[] = new Array(n).fill(undefined);
  const lows: (number | undefined)[] = new Array(n).fill(undefined);

  for (let i = left; i < n - right; i++) {
    let maxVal = -Infinity;
    let maxIdx = -1;
    let minVal = Infinity;
    let minIdx = -1;
    for (let k = i - left; k <= i + right; k++) {
      if (bars[k]!.high > maxVal) {
        maxVal = bars[k]!.high;
        maxIdx = k;
      }
      if (bars[k]!.low < minVal) {
        minVal = bars[k]!.low;
        minIdx = k;
      }
    }
    if (maxIdx === i) highs[i] = bars[i]!.high;
    if (minIdx === i) lows[i] = bars[i]!.low;
  }

  return { highs, lows };
}
