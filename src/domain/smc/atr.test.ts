import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/schemas.ts";
import { computeAtr } from "./atr.ts";

/** Bougie synthétique. */
function bar(i: number, high: number, low: number, close = (high + low) / 2): CtraderTrendbar {
  return { timestamp: i * 60_000, open: (high + low) / 2, high, low, close, volume: 0 };
}

describe("computeAtr", () => {
  // True range par bougie (formule standard, cf. series.ts#trueRangeSeries) : bar0 → 0 (pas de
  // clôture précédente, convention du fichier) ; bar1 : max(104-94=10, |104-95|=9, |94-95|=1) = 10 ;
  // bar2 : max(108-98=10, |108-99|=9, |98-99|=1) = 10 ; bar3 : max(120-100=20, |120-103|=17,
  // |100-103|=3) = 20. Moyenne mobile(3) du dernier point : (10+10+20)/3 = 13.33.
  const BARS: CtraderTrendbar[] = [
    bar(0, 100, 90, 95),
    bar(1, 104, 94, 99),
    bar(2, 108, 98, 103),
    bar(3, 120, 100, 110),
  ];

  test("ATR(3) le plus récent, calculé à la main", () => {
    expect(computeAtr(BARS, 3)).toBeCloseTo(40 / 3);
  });

  test("historique trop court pour la période ⇒ undefined", () => {
    expect(computeAtr(BARS.slice(0, 2), 3)).toBeUndefined();
  });

  test("période par défaut = 14", () => {
    expect(computeAtr(BARS.slice(0, 2))).toBeUndefined();
    expect(computeAtr(BARS, 14)).toBeUndefined(); // seulement 4 bougies, il en faut 14
  });
});
