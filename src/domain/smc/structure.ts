/**
 * Adapté de "ALV - SMC MTF Structure" (Pine Script v6, TradingView) : structure de
 * marché multi-timeframe (swing highs/lows, biais, BOS/CHoCH) à partir de pivots
 * confirmés.
 *
 * Contrairement au script Pine, pas de mode live vs confirmé à trancher : on ne
 * calcule jamais que sur des bougies déjà closes (cf. dropFormingBar dans ./bars.ts)
 * et un pivot n'est retenu qu'une fois `length` bougies passées après lui, donc pas
 * de repaint à corriger — l'input "Signaux confirmés uniquement" du script d'origine
 * disparaît. Pas non plus d'inputs de dashboard (position/thème/colonnes) : ce panel
 * réutilise le thème et la mise en page fixes du reste du terminal.
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { isPivotHigh, isPivotLow } from "./pivots.ts";

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

/** Un snapshot de structure étiqueté par timeframe (ex: "1H") — vit ici plutôt que dans
 * ui/hooks/useStructure.ts pour que domain/smc/bias.ts (qui en a besoin en entrée) n'ait pas à
 * importer un type depuis ui/, ce qui inverserait le sens de dépendance du projet. */
export interface StructureRow {
  label: string;
  snapshot: StructureSnapshot;
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
