import type { StructureBar, StructureBias, SwingPoint } from "./types.ts";

export interface StructureReading {
  bias: StructureBias;
  /** Prix du dernier swing haut confirmé — niveau de résistance à surveiller (définition simple :
   * pas "le niveau au-dessus du prix actuel", juste le dernier haut confirmé, qu'il ait déjà été
   * cassé ou non). `undefined` si aucun swing haut n'est encore confirmé. */
  resistance: number | undefined;
  /** Symétrique de `resistance`, pour le dernier swing bas confirmé. */
  support: number | undefined;
}

/**
 * Biais dérivé par cassure confirmée (CHoCH), pas par simple lecture du dernier label HH/HL/LH/LL —
 * c'est le point méthodologique central (cf. audit) : un pivot peut se relabelliser sans que le prix
 * ait réellement cassé le niveau opposé, et une lecture naïve du dernier label se ferait piéger par
 * ça. Le biais ne change que lorsqu'une bougie CLÔTURE au-delà du dernier swing opposé confirmé —
 * une mèche qui touche sans clôturer au-delà ne compte pas. Bascule immédiate sur cassure (pas
 * d'étape transitoire "neutre" en attendant une confirmation BOS — décision utilisateur).
 *
 * Recalculé entièrement à partir de l'historique à chaque appel (pas d'état caché entre polls) :
 * parcourt les bougies dans l'ordre, révèle chaque swing dès qu'il devient confirmé
 * (`confirmedAtIndex`), et applique la règle de cassure. `label` des swings n'intervient jamais ici
 * — seuls `type`/`price`/`confirmedAtIndex` comptent, exactement pour éviter le piège du label seul.
 * Retourne aussi `resistance`/`support` : les mêmes derniers swings confirmés que le calcul de
 * cassure suit déjà en interne, exposés tels quels plutôt que recalculés séparément.
 */
export function computeStructure(bars: StructureBar[], swings: SwingPoint[]): StructureReading {
  let bias: StructureBias = "neutral";
  let lastHigh: SwingPoint | undefined;
  let lastLow: SwingPoint | undefined;
  let nextSwingIdx = 0;

  for (let i = 0; i < bars.length; i++) {
    while (nextSwingIdx < swings.length && swings[nextSwingIdx]!.confirmedAtIndex <= i) {
      const swing = swings[nextSwingIdx]!;
      if (swing.type === "high") lastHigh = swing;
      else lastLow = swing;
      nextSwingIdx++;
    }

    const close = bars[i]!.close;
    if (bias !== "bearish" && lastLow !== undefined && close < lastLow.price) {
      bias = "bearish";
    }
    if (bias !== "bullish" && lastHigh !== undefined && close > lastHigh.price) {
      bias = "bullish";
    }
  }

  return { bias, resistance: lastHigh?.price, support: lastLow?.price };
}
