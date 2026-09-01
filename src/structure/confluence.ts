import type { StructureReading } from "./bias.ts";
import type { StructurePeriod } from "./fetch.ts";
import type { StructureBias } from "./types.ts";

export type ScalpBias = "bullish" | "bearish" | "mixed";
export type ScalpStrength = "strong" | "moderate" | "weak";

export interface ScalpDirection {
  bias: ScalpBias;
  /** `undefined` quand `bias === "mixed"` — la notion de force n'a pas de sens sans direction. */
  strength: ScalpStrength | undefined;
  bullishCount: number;
  bearishCount: number;
}

/** Timeframes subordonnés à H1, du plus lent au plus rapide — H1 lui-même n'y figure pas, c'est
 * l'autorité contre laquelle ils sont tous comparés (cf. `computeScalpDirection`). */
const LOWER_TIMEFRAMES = ["M_15", "M_5", "M_1"] as const satisfies readonly StructurePeriod[];

/**
 * Agrège les quatre lectures M1/M5/M15/H1 déjà calculées indépendamment (cf. fetch.ts) en une seule
 * direction de scalp — purement pour l'affichage, ne réintroduit aucune corrélation dans le calcul
 * du biais par timeframe (toujours indépendant, cf. bias.ts) : cette fonction ne fait que lire les
 * quatre résultats a posteriori.
 *
 * H1 fait autorité sur la direction (approche top-down standard du scalping : H1 = filtre de
 * tendance, M15 = structure/pullback, M5 = timing d'entrée, M1 = affinage de l'entrée dans le M5).
 * Plutôt qu'un veto binaire (une seule opposition annule tout), on note le degré d'accord — pratique
 * standard en MTF confluence scoring (cf. sources en commentaire de commit) : un score gradué reflète
 * mieux la réalité qu'un pass/fail, un M1 isolé contre la tendance ne devrait pas effacer une
 * confluence H1+M15+M5. Donc :
 * - H1 neutre -> "mixed" : pas de filtre de tendance, rien à noter.
 * - H1 non neutre -> bias = H1, force = nombre de M15/M5/M1 dont le bias vaut exactement H1 (0 à 3) :
 *   3 -> "strong", 2 -> "moderate", 1 -> "weak", 0 -> "mixed" (aucune confirmation, H1 isolé).
 */
export function computeScalpDirection(
  structure: Record<StructurePeriod, StructureReading>,
): ScalpDirection {
  const h1 = structure.H_1.bias;
  const lowerBiases = LOWER_TIMEFRAMES.map((period) => structure[period].bias);
  const allBiases = [h1, ...lowerBiases];
  const bullishCount = allBiases.filter((bias) => bias === "bullish").length;
  const bearishCount = allBiases.filter((bias) => bias === "bearish").length;
  const mixed: ScalpDirection = { bias: "mixed", strength: undefined, bullishCount, bearishCount };

  if (h1 === "neutral") return mixed;

  const agreement = lowerBiases.filter((bias) => bias === h1).length;
  if (agreement === 0) return mixed;

  const strength: ScalpStrength =
    agreement === 3 ? "strong" : agreement === 2 ? "moderate" : "weak";
  return { bias: h1, strength, bullishCount, bearishCount };
}
