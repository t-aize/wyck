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
  /** Historique total à charger, en ms — assez pour deux pivots confirmés avec marge. */
  historyMs: number;
  /** Ne change qu'une fois par jour (D1) : refetché une fois par jour calendaire plutôt qu'à
   * chaque poll de structure — cf. useStructure.ts. */
  dailyOnly?: boolean;
}

/**
 * 15M=7 / 1H=14 / 4H=20 sont les longueurs par défaut du script d'origine (Pine). 5M et D1 n'ont
 * pas d'équivalent documenté dans le script — recherché sans succès une source faisant autorité
 * (même LuxAlgo, la référence SMC la plus connue, utilise une longueur fixe indépendante du
 * timeframe : ça ne se scale pas dans l'industrie). Les valeurs ci-dessous suivent la régression
 * qui ressort des 3 valeurs d'origine elles-mêmes : `length ≈ -5.7 + 3.25 × log2(minutes_par_
 * bougie)`, un ajustement quasi parfait sur 15M/1H/4H (erreur < 4% sur le point du milieu).
 * Extrapolée sur D1 → 28. Pour 5M, la droite donne 2, bien trop bruyant en pratique sur XAUUSD
 * (quasi chaque mèche deviendrait un pivot) — écart assumé de la régression, length=8 choisi à
 * la place (cohérent avec le 7 du 15M). Des longueurs ajustables ici si l'usage réel montre
 * qu'elles rendent mal — pas un scope figé comme SYMBOL.
 *
 * Un 6H a été tenté puis retiré : cTrader n'a pas cette période nativement (cf. TRENDBAR_PERIODS
 * dans constants.ts), donc reconstruite depuis H1 — mais ça revenait vide en pratique contre le
 * vrai serveur MCP, très probablement parce que "1H" et "6H" redemandaient alors exactement les
 * mêmes bougies H1, en parallèle, dans le même batch (cf. useStructure.ts). Toutes les périodes
 * ci-dessous sont désormais distinctes — plus aucune requête dupliquée dans un même cycle.
 */
export const STRUCTURE_TIMEFRAMES: StructureTimeframe[] = [
  { label: "5M", period: "M_5", length: 8, periodMs: 5 * 60_000, historyMs: 1400 * 60 * 60_000 },
  { label: "15M", period: "M_15", length: 7, periodMs: 15 * 60_000, historyMs: 1400 * 60 * 60_000 },
  { label: "1H", period: "H_1", length: 14, periodMs: 60 * 60_000, historyMs: 1400 * 60 * 60_000 },
  {
    label: "4H",
    period: "H_4",
    length: 20,
    periodMs: 4 * 60 * 60_000,
    historyMs: 1400 * 60 * 60_000,
  },
  {
    label: "D1",
    period: "D_1",
    length: 28,
    periodMs: 24 * 60 * 60_000,
    historyMs: 7000 * 60 * 60_000, // ~292 jours — length=28 a besoin de bien plus que les 58 jours des autres TF
    dailyOnly: true,
  },
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
