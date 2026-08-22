/** Bougie minimale nécessaire au calcul de structure — découplé de `CtraderTrendbar` (comme
 * `trading/atr.ts#computeAtr` prend `{high,low,close}[]` plutôt que le type cTrader directement),
 * pour rester pur et testable avec des littéraux. */
export interface StructureBar {
  timestamp: number;
  high: number;
  low: number;
  close: number;
}

export type SwingLabel = "HH" | "HL" | "LH" | "LL";

export interface SwingPoint {
  type: "high" | "low";
  /** Position dans le tableau de bougies où ce pivot a été trouvé. */
  index: number;
  /** Position à partir de laquelle ce pivot est confirmé/connaissable (index + largeur de la
   * fractale) — un swing n'est identifiable qu'une fois vu des deux côtés, cf. structure/swings.ts. */
  confirmedAtIndex: number;
  timestamp: number;
  price: number;
  /** `undefined` s'il n'y a pas de swing précédent du même type à comparer (le tout premier), ou si
   * le prix est exactement égal au précédent — ni plus haut ni plus bas, pas un nouveau niveau
   * distinct (cf. structure/swings.ts). */
  label: SwingLabel | undefined;
}

export type StructureBias = "bullish" | "bearish" | "neutral";
