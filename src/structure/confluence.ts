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

/**
 * Agrège les trois lectures M5/M15/H1 déjà calculées indépendamment (cf. fetch.ts) en une seule
 * direction de scalp — purement pour l'affichage, ne réintroduit aucune corrélation dans le calcul
 * du biais par timeframe (toujours indépendant, cf. bias.ts) : cette fonction ne fait que lire les
 * trois résultats a posteriori.
 *
 * H1 fait autorité sur la direction (approche top-down standard du scalping : H1 = filtre de
 * tendance, M15 = structure/pullback, M5 = timing d'entrée — la HTF fixe le sens, les LTF ne font
 * que chronométrer l'entrée dedans, jamais contre — cf. sources en commentaire de commit). Donc :
 * - H1 neutre, ou contredit par M15 ou M5 (bias strictement opposé, pas juste neutre) -> "mixed" :
 *   pas de filtre de tendance fiable, pas de conviction suffisante pour scalper.
 * - H1 confirmé par les deux autres (aucune opposition) -> bias = H1, "strong" si M15 ET M5
 *   s'alignent activement dessus, "moderate" si un seul le fait (l'autre neutre, ni pour ni contre).
 */
export function computeScalpDirection(
  structure: Record<StructurePeriod, StructureReading>,
): ScalpDirection {
  const h1 = structure.H_1.bias;
  const m15 = structure.M_15.bias;
  const m5 = structure.M_5.bias;
  const bullishCount = [h1, m15, m5].filter((bias) => bias === "bullish").length;
  const bearishCount = [h1, m15, m5].filter((bias) => bias === "bearish").length;
  const mixed: ScalpDirection = { bias: "mixed", strength: undefined, bullishCount, bearishCount };

  if (h1 === "neutral" || opposes(m15, h1) || opposes(m5, h1)) return mixed;

  const strength: ScalpStrength = m15 === h1 && m5 === h1 ? "strong" : "moderate";
  return { bias: h1, strength, bullishCount, bearishCount };
}
