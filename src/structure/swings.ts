import type { StructureBar, SwingLabel, SwingPoint } from "./types.ts";

/** Fractale de Williams (5 bougies) — définition standard pour ce genre d'usage : un pivot plus
 * réactif (largeur 1) est beaucoup plus bruyant sur du M5, un pivot plus large isole des retournements
 * plus significatifs au prix de plus de délai de confirmation. */
export const DEFAULT_FRACTAL_WIDTH = 2;

/** Bord gauche strict, bord droit inclusif — sur deux bougies adjacentes de même extrême, ça retient
 * la première plutôt que de compter deux fois le même niveau (cf. audit méthodologique : convention
 * standard pour départager les égalités). */
function isSwingHigh(bars: StructureBar[], i: number, width: number): boolean {
  for (let j = i - width; j < i; j++) {
    if (!(bars[i]!.high > bars[j]!.high)) return false;
  }
  for (let j = i + 1; j <= i + width; j++) {
    if (!(bars[i]!.high >= bars[j]!.high)) return false;
  }
  return true;
}

function isSwingLow(bars: StructureBar[], i: number, width: number): boolean {
  for (let j = i - width; j < i; j++) {
    if (!(bars[i]!.low < bars[j]!.low)) return false;
  }
  for (let j = i + 1; j <= i + width; j++) {
    if (!(bars[i]!.low <= bars[j]!.low)) return false;
  }
  return true;
}

function classify(
  type: "high" | "low",
  price: number,
  previous: number | undefined,
): SwingLabel | undefined {
  if (previous === undefined || price === previous) return undefined;
  if (type === "high") return price > previous ? "HH" : "LH";
  return price > previous ? "HL" : "LL";
}

/**
 * Détecte les pivots (hauts/bas locaux) confirmés sur une série de bougies déjà clôturées — n'évalue
 * jamais la bougie en cours de formation : les `width` dernières bougies de la série ne peuvent de
 * toute façon pas encore être confirmées (pas assez de bougies à droite), donc naturellement exclues.
 *
 * Chaque pivot est classé par rapport au précédent du même type (HH/LH pour les hauts, HL/LL pour
 * les bas) — `label` reste `undefined` pour le tout premier pivot de chaque type ou une égalité
 * exacte, plutôt que de forcer une classification qui n'a pas de sens. Retourne TOUS les pivots
 * bruts, classés ou non : `structure/bias.ts` a besoin de connaître l'extremum confirmé le plus
 * récent de chaque type même quand il n'a pas de label distinct, pour sa détection de cassure.
 */
export function detectSwings(
  bars: StructureBar[],
  width: number = DEFAULT_FRACTAL_WIDTH,
): SwingPoint[] {
  const swings: SwingPoint[] = [];
  let previousHigh: number | undefined;
  let previousLow: number | undefined;

  // Deux `if` indépendants plutôt qu'un `else if` : une bougie étroite encadrée de bougies plus
  // larges des deux côtés peut être à la fois un haut ET un bas local (rare, mais rien n'empêche
  // structurellement les deux — pas de raison d'en laisser tomber un arbitrairement).
  for (let i = width; i < bars.length - width; i++) {
    if (isSwingHigh(bars, i, width)) {
      const price = bars[i]!.high;
      swings.push({
        type: "high",
        index: i,
        confirmedAtIndex: i + width,
        timestamp: bars[i]!.timestamp,
        price,
        label: classify("high", price, previousHigh),
      });
      previousHigh = price;
    }
    if (isSwingLow(bars, i, width)) {
      const price = bars[i]!.low;
      swings.push({
        type: "low",
        index: i,
        confirmedAtIndex: i + width,
        timestamp: bars[i]!.timestamp,
        price,
        label: classify("low", price, previousLow),
      });
      previousLow = price;
    }
  }

  return swings;
}
