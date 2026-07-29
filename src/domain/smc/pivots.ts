import type { CtraderTrendbar } from "../../ctrader/client.ts";

/**
 * Détection de pivot par fenêtre symétrique [i-length, i+length] : confirmé seulement une fois
 * `length` bougies passées des deux côtés, donc jamais de repaint. C'est la même convention que
 * `ta.pivothigh(length,length)`/`ta.pivotlow(length,length)` en Pine — utilisée par le script
 * "Trendlines with Breaks" de LuxAlgo, et par le script ALV dont `smc/structure.ts` est adapté.
 * Le script "Smart Money Concepts" de LuxAlgo utilise une méthode différente ("leg", asymétrique/
 * streaming) pour sa propre structure interne/swing — cf. docs/smc-plan.md §1. Partagé ici plutôt
 * que dupliqué : Trendlines & Break réutilisera exactement cette même fonction.
 */

// Bornes garanties par les appelants (i toujours dans [length, bars.length - 1 - length]) : `!` plutôt qu'une garde inutile.
export function isPivotHigh(bars: CtraderTrendbar[], i: number, length: number): boolean {
  const high = bars[i]!.high;
  for (let k = i - length; k <= i + length; k++) {
    if (k !== i && bars[k]!.high >= high) return false;
  }
  return true;
}

export function isPivotLow(bars: CtraderTrendbar[], i: number, length: number): boolean {
  const low = bars[i]!.low;
  for (let k = i - length; k <= i + length; k++) {
    if (k !== i && bars[k]!.low <= low) return false;
  }
  return true;
}
