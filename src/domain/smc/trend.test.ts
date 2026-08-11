import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeTrendState } from "./trend.ts";

/** Bougie synthétique. */
function bar(i: number, high: number, low: number, close = (high + low) / 2): CtraderTrendbar {
  return { timestamp: i * 60_000, open: (high + low) / 2, high, low, close, volume: 0 };
}

// Zigzag net, fractale left=right=2 (fenêtre de 5 bougies) : creux #1=75(idx2), sommet #1=120(idx7),
// creux #2=85(idx12, > 75 ⇒ HL), sommet #2=130(idx17, > 120 ⇒ HH). Séries tracées via l'algorithme
// réel (cf. commentaire équivalent dans les anciens tests), pas calculées à la main.
const ZIGZAG_UP: CtraderTrendbar[] = [
  bar(0, 90, 85),
  bar(1, 92, 87),
  bar(2, 80, 75),
  bar(3, 92, 87),
  bar(4, 94, 89),
  bar(5, 100, 95),
  bar(6, 110, 105),
  bar(7, 120, 115),
  bar(8, 108, 103),
  bar(9, 106, 101),
  bar(10, 100, 95),
  bar(11, 96, 91),
  bar(12, 90, 85),
  bar(13, 94, 89),
  bar(14, 98, 93),
  bar(15, 106, 101),
  bar(16, 116, 111),
  bar(17, 130, 125),
  bar(18, 120, 115),
  bar(19, 118, 113),
];

// Tests d'intégration de l'agrégateur uniquement — les tests unitaires de chaque méthode/filtre
// vivent désormais dans leur propre fichier (structuralTrend.test.ts, structureEvents.test.ts,
// sweep.test.ts, filters.test.ts, atr.test.ts), cf. docs/ARCHITECTURE.md.
describe("computeTrendState", () => {
  test("historique trop court ⇒ état vide", () => {
    const bars = ZIGZAG_UP.slice(0, 3);
    expect(computeTrendState(bars, { left: 2, right: 2 })).toEqual({
      structural: 0,
      rawEvent: 0,
      confirmedEvent: 0,
      lastEventDisplacementOk: undefined,
      sweepHigh: false,
      sweepLow: false,
      emaStackBullish: undefined,
      adx: undefined,
      pending: { resistance: undefined, support: undefined },
    });
  });

  test("combine structural + événementiel sur un même historique haussier", () => {
    const state = computeTrendState(ZIGZAG_UP, {
      left: 2,
      right: 2,
      displacementMult: 0,
      emaFast: 3,
      emaSlow: 5,
      adxPeriod: 3,
    });
    expect(state.structural).toBe(1);
    expect(state.emaStackBullish).toBe(true);
    expect(state.adx).toBeGreaterThan(0);
  });

  test("expose le prochain niveau de résistance encore surveillé", () => {
    // Sommet #2 (130 @ idx17) jamais cassé dans ZIGZAG_UP ⇒ toujours "en résistance". Le trend
    // établi par le dernier événement (BOS haussier) fait qu'une cassure continuerait ⇒ BOS.
    const state = computeTrendState(ZIGZAG_UP, { left: 2, right: 2, displacementMult: 0 });
    expect(state.pending.resistance).toEqual({ level: 130, kind: "BOS" });
  });
});
