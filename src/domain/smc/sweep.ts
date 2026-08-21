/** Liquidity sweep — mèche au-delà d'un niveau, PAS de clôture au-delà (distinct d'une vraie
 * cassure de structure, cf. structureEvents.ts). Extrait de trend.ts. */

import type { CtraderTrendbar } from "../../ctrader/schemas.ts";
import { detectSwings, type FractalOptions } from "./swings.ts";

export interface SweepState {
  /** Mèche au-delà du dernier swing high puis clôture repassée en dessous — piège vendeur. */
  sweepHigh: boolean;
  /** Symétrique, piège acheteur. */
  sweepLow: boolean;
}

/** État du sweep sur la dernière bougie seulement (le panneau affiche "maintenant", pas un
 * historique) — équivalent à lire `.iloc[-1]` des deux séries pandas de la référence. */
export function detectLatestSweep(
  bars: CtraderTrendbar[],
  { left, right }: FractalOptions,
): SweepState {
  const { highs, lows } = detectSwings(bars, { left, right });
  let lastHigh: number | undefined;
  let lastLow: number | undefined;
  let sweepHigh = false;
  let sweepLow = false;

  for (let i = 0; i < bars.length; i++) {
    if (highs[i] !== undefined) lastHigh = highs[i];
    if (lows[i] !== undefined) lastLow = lows[i];

    sweepHigh = lastHigh !== undefined && bars[i]!.high > lastHigh && bars[i]!.close < lastHigh;
    sweepLow = lastLow !== undefined && bars[i]!.low < lastLow && bars[i]!.close > lastLow;
  }

  return { sweepHigh, sweepLow };
}
