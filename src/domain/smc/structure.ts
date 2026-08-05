/**
 * Structure de marché multi-timeframe (swing highs/lows, biais, BOS/CHoCH) à partir de pivots
 * confirmés. Détection de pivot : machine à état `swings()` du script "Smart Money Concepts
 * [LuxAlgo]" (le plus copié sur TradingView) — verrouille l'extrême courant jusqu'à ce qu'un
 * nouvel extrême opposé dépasse la fenêtre glissante des `length` dernières bougies (zigzag
 * adaptatif), plutôt qu'une fractale symétrique classique (`ta.pivothigh(length,length)`) qui
 * sous-détecte dès que deux sommets proches existent dans la même fenêtre. Choisie après
 * comparaison avec deux implémentations SMC de référence indépendantes (LuxAlgo et le package
 * Python `smart-money-concepts`), qui convergent toutes les deux vers une détection de type
 * zigzag plutôt qu'une fractale à fenêtre fixe stricte.
 *
 * Pas de mode live vs confirmé à trancher comme en Pine : on ne calcule jamais que sur des
 * bougies déjà closes (cf. dropFormingBar dans ./bars.ts), et un pivot n'est confirmé qu'une fois
 * `length` bougies passées, donc pas de repaint à corriger.
 */

import type { CtraderTrendbar } from "../../ctrader/client.ts";

export type StructureBias = -1 | 0 | 1;
export type StructureSignalType = "BOS" | "CHoCH";

export interface PendingBreak {
  level: number;
  /** Classification si ce niveau casse maintenant, avec la tendance actuelle — même règle que le
   * signal déjà réalisé (continuation du trend = BOS, inversion = CHoCH). */
  type: StructureSignalType;
}

export interface StructureSnapshot {
  swingHigh: number | undefined;
  swingLow: number | undefined;
  bias: StructureBias;
  signalType: StructureSignalType | undefined;
  signalDir: -1 | 1 | undefined;
  /** Mèche au-delà du dernier swing puis clôture repassée à l'intérieur (balayage de liquidité). */
  sweepLow: boolean;
  sweepHigh: boolean;
  /** Niveau haussier encore surveillé pour une cassure, et ce que cette cassure produirait.
   * `undefined` si déjà cassé et qu'aucun nouveau pivot haut n'a encore reformé de niveau à
   * surveiller. */
  nextBullish: PendingBreak | undefined;
  /** Symétrique de `nextBullish` côté baissier. */
  nextBearish: PendingBreak | undefined;
}

/** Un snapshot de structure étiqueté par timeframe (ex: "1H") — vit ici plutôt que dans
 * ui/hooks/useStructure.ts pour que domain/smc/bias.ts (qui en a besoin en entrée) n'ait pas à
 * importer un type depuis ui/, ce qui inverserait le sens de dépendance du projet. */
export interface StructureRow {
  label: string;
  /** Structure "swing"/externe (fenêtre de pivot `SWING_LENGTH`, cf. timeframes.ts) — pilote le
   * tableau SMC MTF STRUCTURE et le biais global (bias.ts). */
  snapshot: StructureSnapshot;
  /** Structure interne (fenêtre de pivot `INTERNAL_LENGTH`, plus courte) — mêmes deux échelles que
   * le script SMC de LuxAlgo. Uniquement consommée par le panneau "prochain BOS/CHoCH"
   * (NextStructurePanel.tsx) pour ne pas manquer les retournements à plus petite échelle que la
   * structure swing ; bias.ts et StructurePanel.tsx n'en ont pas besoin et ne la lisent pas. */
  internal: StructureSnapshot;
}

const EMPTY_SNAPSHOT: StructureSnapshot = {
  swingHigh: undefined,
  swingLow: undefined,
  bias: 0,
  signalType: undefined,
  signalDir: undefined,
  sweepLow: false,
  sweepHigh: false,
  nextBullish: undefined,
  nextBearish: undefined,
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
  // État de la machine swings() — 0 verrouillé sur un sommet, 1 sur un creux (`var os = 0` en Pine).
  let os: 0 | 1 = 0;

  for (let i = length; i < bars.length; i++) {
    const bar = bars[i]!;

    // swings(length) : fenêtre glissante des `length` dernières bougies [i-length+1, i], comparée
    // au candidat `length` bougies en arrière (bars[i-length]) — verrouille cet extrême tant que
    // rien depuis ne l'a dépassé, bascule dès qu'un extrême opposé le fait.
    let upper = -Infinity;
    let lower = Infinity;
    for (let k = i - length + 1; k <= i; k++) {
      upper = Math.max(upper, bars[k]!.high);
      lower = Math.min(lower, bars[k]!.low);
    }
    const refHigh = bars[i - length]!.high;
    const refLow = bars[i - length]!.low;
    const prevOs = os;
    os = refHigh > upper ? 0 : refLow < lower ? 1 : os;

    if (os === 0 && prevOs !== 0) {
      prevHigh = currHigh;
      currHigh = refHigh;
      highBroken = false;
      breakHighLevel = refHigh;
    }
    if (os === 1 && prevOs !== 1) {
      prevLow = currLow;
      currLow = refLow;
      lowBroken = false;
      breakLowLevel = refLow;
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

  // Même règle de classification que dans la boucle de détection ci-dessus : une cassure de
  // breakHighLevel/breakLowLevel continue le trend courant (BOS) ou l'inverse (CHoCH).
  const nextBullish: PendingBreak | undefined =
    breakHighLevel !== undefined
      ? { level: breakHighLevel, type: trend === -1 ? "CHoCH" : "BOS" }
      : undefined;
  const nextBearish: PendingBreak | undefined =
    breakLowLevel !== undefined
      ? { level: breakLowLevel, type: trend === 1 ? "CHoCH" : "BOS" }
      : undefined;

  return {
    swingHigh: currHigh,
    swingLow: currLow,
    bias,
    signalType,
    signalDir,
    sweepLow,
    sweepHigh,
    nextBullish,
    nextBearish,
  };
}
