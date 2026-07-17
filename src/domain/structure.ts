/**
 * Adapté de "ALV - SMC MTF Structure" (Pine Script v6, TradingView) : structure de
 * marché multi-timeframe (swing highs/lows, biais, BOS/CHoCH) à partir de pivots
 * confirmés.
 *
 * Contrairement au script Pine, pas de mode live vs confirmé à trancher : on ne
 * calcule jamais que sur des bougies déjà closes (cf. dropFormingBar) et un pivot
 * n'est retenu qu'une fois `length` bougies passées après lui, donc pas de repaint
 * à corriger — l'input "Signaux confirmés uniquement" du script d'origine disparaît.
 * Pas non plus d'inputs de dashboard (position/thème/colonnes) : ce panel réutilise
 * le thème et la mise en page fixes du reste du terminal.
 */

import type { CtraderTrendbar, GetTrendbarsParams } from "../ctrader/client.ts";

export interface StructureTimeframe {
  label: string;
  period: GetTrendbarsParams["period"];
  length: number;
  periodMs: number;
}

/** TFs & longueurs par défaut du script d'origine — un choix de scope figé, pas un réglage (cf. SYMBOL dans constants.ts). */
export const STRUCTURE_TIMEFRAMES: StructureTimeframe[] = [
  { label: "15M", period: "M_15", length: 7, periodMs: 15 * 60_000 },
  { label: "1H", period: "H_1", length: 14, periodMs: 60 * 60_000 },
  { label: "4H", period: "H_4", length: 20, periodMs: 4 * 60 * 60_000 },
];

export type StructureBias = -1 | 0 | 1;
export type StructureSignalType = "BOS" | "CHoCH";

export interface StructureSnapshot {
  swingHigh: number | undefined;
  swingLow: number | undefined;
  bias: StructureBias;
  signalType: StructureSignalType | undefined;
  signalDir: -1 | 1 | undefined;
  /** Mèche au-delà du dernier swing puis clôture repassée à l'intérieur (balayage de liquidité). */
  sweepLow: boolean;
  sweepHigh: boolean;
}

const EMPTY_SNAPSHOT: StructureSnapshot = {
  swingHigh: undefined,
  swingLow: undefined,
  bias: 0,
  signalType: undefined,
  signalDir: undefined,
  sweepLow: false,
  sweepHigh: false,
};

/** get_trendbars renvoie la bougie en formation en dernière position ; on l'écarte, sinon un pivot pourrait "apparaître" puis disparaître d'un poll à l'autre. */
export function dropFormingBar(
  bars: CtraderTrendbar[],
  periodMs: number,
  now: number,
): CtraderTrendbar[] {
  const last = bars[bars.length - 1];
  return last && last.timestamp + periodMs > now ? bars.slice(0, -1) : bars;
}

// Bornes garanties par les appelants (i toujours dans [length, bars.length - 1 - length]) : `!` plutôt qu'une garde inutile.
function isPivotHigh(bars: CtraderTrendbar[], i: number, length: number): boolean {
  const high = bars[i]!.high;
  for (let k = i - length; k <= i + length; k++) {
    if (k !== i && bars[k]!.high >= high) return false;
  }
  return true;
}

function isPivotLow(bars: CtraderTrendbar[], i: number, length: number): boolean {
  const low = bars[i]!.low;
  for (let k = i - length; k <= i + length; k++) {
    if (k !== i && bars[k]!.low <= low) return false;
  }
  return true;
}

/**
 * Rejoue l'historique pour ne garder que l'état final : swing H/L courants, biais
 * (déduit des deux derniers pivots), et le dernier signal de structure (BOS =
 * cassure dans le sens de la tendance en cours, CHoCH = cassure qui l'inverse).
 */
export function computeStructure(bars: CtraderTrendbar[], length: number): StructureSnapshot {
  if (bars.length < length * 2 + 1) return EMPTY_SNAPSHOT;

  let prevHigh: number | undefined;
  let currHigh: number | undefined;
  let prevLow: number | undefined;
  let currLow: number | undefined;
  let highBroken = false;
  let lowBroken = false;
  let breakHighLevel: number | undefined;
  let breakLowLevel: number | undefined;
  let trend: StructureBias = 0;
  let signalType: StructureSignalType | undefined;
  let signalDir: -1 | 1 | undefined;

  for (let i = length; i < bars.length; i++) {
    const bar = bars[i]!;

    if (i + length < bars.length) {
      if (isPivotHigh(bars, i, length)) {
        prevHigh = currHigh;
        currHigh = bar.high;
        highBroken = false;
        breakHighLevel = bar.high;
      }
      if (isPivotLow(bars, i, length)) {
        prevLow = currLow;
        currLow = bar.low;
        lowBroken = false;
        breakLowLevel = bar.low;
      }
    }

    if (currHigh !== undefined && bar.high > currHigh) highBroken = true;
    if (currLow !== undefined && bar.low < currLow) lowBroken = true;

    const prevClose = bars[i - 1]?.close;
    if (prevClose !== undefined) {
      if (
        breakHighLevel !== undefined &&
        bar.close > breakHighLevel &&
        prevClose <= breakHighLevel
      ) {
        signalType = trend === -1 ? "CHoCH" : "BOS";
        signalDir = 1;
        trend = 1;
        breakHighLevel = undefined;
      }
      if (breakLowLevel !== undefined && bar.close < breakLowLevel && prevClose >= breakLowLevel) {
        signalType = trend === 1 ? "CHoCH" : "BOS";
        signalDir = -1;
        trend = -1;
        breakLowLevel = undefined;
      }
    }
  }

  const lastClose = bars[bars.length - 1]!.close;
  const sweepLow = lowBroken && currLow !== undefined && lastClose > currLow;
  const sweepHigh = highBroken && currHigh !== undefined && lastClose < currHigh;

  let bias: StructureBias = 0;
  if (
    prevHigh !== undefined &&
    prevLow !== undefined &&
    currHigh !== undefined &&
    currLow !== undefined
  ) {
    const higherHigh = currHigh > prevHigh;
    const higherLow = currLow > prevLow;
    bias = higherHigh && higherLow ? 1 : !higherHigh && !higherLow ? -1 : 0;
  }

  return {
    swingHigh: currHigh,
    swingLow: currLow,
    bias,
    signalType,
    signalDir,
    sweepLow,
    sweepHigh,
  };
}
