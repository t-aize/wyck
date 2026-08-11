/**
 * Agrégateur de tendance (Bullish / Bearish / Range) — combine les méthodes/filtres extraits dans
 * les fichiers voisins de ce dossier (chacun une seule responsabilité, cf. docs/ARCHITECTURE.md) :
 *
 * - `structuralTrend.ts` — MÉTHODE 1, structurelle (HH/HL vs LH/LL, théorie de Dow).
 * - `structureEvents.ts` — MÉTHODE 2, événementielle (BOS/CHoCH + règle de confirmation).
 * - `sweep.ts` — liquidity sweep (mèche au-delà d'un niveau, distinct d'une cassure de structure).
 * - `filters.ts` — EMA stack + ADX, deux filtres classiques explicitement PAS du SMC.
 * - `atr.ts` / `series.ts` / `swings.ts` / `types.ts` — briques bas niveau partagées par les fichiers
 *   ci-dessus (ATR, fractale/swing, true range, le type `Trend` lui-même).
 *
 * Ce fichier reste le point d'entrée unique pour le reste de l'app (`useTrend.ts`, `TrendPanel.tsx`,
 * `domain/trading.ts`) — d'où les ré-exports ci-dessous, qui préservent exactement la même surface
 * publique qu'avant l'éclatement (rien en dehors de `domain/smc/` n'a besoin de changer un import).
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeAdx, computeEmaStack } from "./filters.ts";
import { classifyStructuralTrend } from "./structuralTrend.ts";
import {
  classifyEventTrend,
  detectStructureEvents,
  type PendingLevels,
  type StructureDetectionOptions,
} from "./structureEvents.ts";
import { detectLatestSweep } from "./sweep.ts";
import type { Trend } from "./types.ts";

export { computeAtr } from "./atr.ts";
export { ADX_TRENDING_THRESHOLD, computeAdx, computeEmaStack } from "./filters.ts";
export type { StructuralOptions } from "./structuralTrend.ts";
export { classifyStructuralTrend } from "./structuralTrend.ts";
export type {
  EventTrendResult,
  PendingLevel,
  PendingLevels,
  StructureDetectionOptions,
  StructureDetectionResult,
  StructureEvent,
  StructureEventKind,
} from "./structureEvents.ts";
export { classifyEventTrend, detectStructureEvents } from "./structureEvents.ts";
export type { SweepState } from "./sweep.ts";
export { detectLatestSweep } from "./sweep.ts";
export type { FractalOptions } from "./swings.ts";
export type { Trend } from "./types.ts";

// ─── Combiné ────────────────────────────────────────────────────────────

export interface TrendComputeOptions extends StructureDetectionOptions {
  minSwingPct?: number;
  emaFast?: number;
  emaSlow?: number;
  adxPeriod?: number;
}

export interface TrendState {
  structural: Trend;
  rawEvent: Trend;
  confirmedEvent: Trend;
  lastEventDisplacementOk: boolean | undefined;
  sweepHigh: boolean;
  sweepLow: boolean;
  emaStackBullish: boolean | undefined;
  adx: number | undefined;
  /** Prochains niveaux de structure (résistance/support) encore surveillés, avec ce que leur
   * cassure produirait — BOS ou CHoCH selon la tendance actuelle. */
  pending: PendingLevels;
}

const EMPTY_TREND_STATE: TrendState = {
  structural: 0,
  rawEvent: 0,
  confirmedEvent: 0,
  lastEventDisplacementOk: undefined,
  sweepHigh: false,
  sweepLow: false,
  emaStackBullish: undefined,
  adx: undefined,
  pending: { resistance: undefined, support: undefined },
};

export function computeTrendState(
  bars: CtraderTrendbar[],
  options: TrendComputeOptions,
): TrendState {
  const {
    left,
    right,
    minSwingPct = 0,
    atrPeriod = 14,
    displacementMult = 1.2,
    emaFast = 20,
    emaSlow = 50,
    adxPeriod = 14,
  } = options;

  if (bars.length < left + right + 1) return EMPTY_TREND_STATE;

  const structural = classifyStructuralTrend(bars, { left, right, minSwingPct });
  const { events, pending } = detectStructureEvents(bars, {
    left,
    right,
    atrPeriod,
    displacementMult,
  });
  const { raw, confirmed, lastDisplacementOk } = classifyEventTrend(events);
  const { sweepHigh, sweepLow } = detectLatestSweep(bars, { left, right });

  return {
    structural,
    rawEvent: raw,
    confirmedEvent: confirmed,
    lastEventDisplacementOk: lastDisplacementOk,
    sweepHigh,
    sweepLow,
    emaStackBullish: computeEmaStack(bars, { fast: emaFast, slow: emaSlow }),
    adx: computeAdx(bars, adxPeriod),
    pending,
  };
}
