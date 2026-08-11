/**
 * MÉTHODE 1 — structurelle (HH/HL vs LH/LL) : la plus fiable, directement issue de la théorie de
 * Dow. Séquence mixte (ex: HH mais LL) = range, pas de biais net — un signal binaire bull/bear
 * génère des faux positifs en range. Extrait de trend.ts.
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { detectSwings, type FractalOptions } from "./swings.ts";
import type { Trend } from "./types.ts";

export interface StructuralOptions extends FractalOptions {
  /** Filtre les swings trop petits (bruit) en % du prix — évite qu'un micro-pivot fausse la
   * lecture HH/HL sur un TF bas (M5). 0 = désactivé. */
  minSwingPct?: number;
}

export function classifyStructuralTrend(
  bars: CtraderTrendbar[],
  { left, right, minSwingPct = 0 }: StructuralOptions,
): Trend {
  const { highs, lows } = detectSwings(bars, { left, right });
  const highSeq: number[] = [];
  const lowSeq: number[] = [];
  let current: Trend = 0;

  for (let i = 0; i < bars.length; i++) {
    const h = highs[i];
    if (h !== undefined) {
      const prev = highSeq.at(-1);
      if (prev === undefined || (Math.abs(h - prev) / prev) * 100 >= minSwingPct) highSeq.push(h);
    }
    const l = lows[i];
    if (l !== undefined) {
      const prev = lowSeq.at(-1);
      if (prev === undefined || (Math.abs(l - prev) / prev) * 100 >= minSwingPct) lowSeq.push(l);
    }

    if (highSeq.length >= 2 && lowSeq.length >= 2) {
      const higherHigh = highSeq.at(-1)! > highSeq.at(-2)!;
      const higherLow = lowSeq.at(-1)! > lowSeq.at(-2)!;
      const lowerHigh = highSeq.at(-1)! < highSeq.at(-2)!;
      const lowerLow = lowSeq.at(-1)! < lowSeq.at(-2)!;
      current = higherHigh && higherLow ? 1 : lowerHigh && lowerLow ? -1 : 0;
    }
  }

  return current;
}
