import type { StructureReading } from "./bias.ts";
import type { StructurePeriod } from "./fetch.ts";
import type { StructureBias } from "./types.ts";

export type ScalpBias = "bullish" | "bearish" | "mixed";
export type ScalpStrength = "strong" | "moderate";

export interface ScalpDirection {
  bias: ScalpBias;
  /** `undefined` quand `bias === "mixed"` — la notion de force n'a pas de sens sans direction. */
  strength: ScalpStrength | undefined;
  bullishCount: number;
  bearishCount: number;
}

function opposes(bias: StructureBias, h1: StructureBias): boolean {
  return bias !== "neutral" && bias !== h1;
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
 * tendance, M15 = structure/pullback, M5 = timing d'entrée, M1 = affinage de l'entrée dans le M5 —
 * la HTF fixe le sens, les LTF ne font que chronométrer l'entrée dedans, jamais contre — cf. sources
 * en commentaire de commit). Donc :
 * - H1 neutre, ou contredit par M15, M5 ou M1 (bias strictement opposé, pas juste neutre) ->
 *   "mixed" : pas de filtre de tendance fiable, pas de conviction suffisante pour scalper.
 * - H1 confirmé par les trois autres (aucune opposition) -> bias = H1, "strong" si M15, M5 ET M1
 *   s'alignent tous activement dessus, "moderate" sinon (au moins un neutre, ni pour ni contre).
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

  if (h1 === "neutral" || lowerBiases.some((bias) => opposes(bias, h1))) return mixed;

  const strength: ScalpStrength = lowerBiases.every((bias) => bias === h1) ? "strong" : "moderate";
  return { bias: h1, strength, bullishCount, bearishCount };
}
