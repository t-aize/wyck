import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeStructure } from "./structure.ts";

/** Bougie synthétique — seuls high/low/close comptent pour computeStructure. */
function bar(i: number, high: number, low: number, close = high): CtraderTrendbar {
  return { timestamp: i, open: low, high, low, close, volume: 0 };
}

// Série commune aux tests bias/BOS/CHoCH : deux jambes haussières (creux #1→85, sommet #1→115,
// creux #2→103 plus haut, sommet #2→122 plus haut) suivies d'une cassure haussière (BOS) sur la
// clôture de la bougie 9 (120, au-dessus du sommet #1 à 115) — valeurs retracées via l'algorithme
// swings() réel (cf. commentaire en tête de structure.ts), pas à la main.
const UPTREND_BARS: CtraderTrendbar[] = [
  bar(0, 100, 95),
  bar(1, 90, 85),
  bar(2, 95, 90),
  bar(3, 105, 100),
  bar(4, 115, 110),
  bar(5, 108, 103),
  bar(6, 112, 107),
  bar(7, 122, 117),
  bar(8, 116, 111),
  bar(9, 120, 115),
];

describe("computeStructure", () => {
  test("renvoie un snapshot vide si l'historique est trop court", () => {
    const bars = [bar(0, 10, 5), bar(1, 11, 6), bar(2, 12, 7)];
    const snapshot = computeStructure(bars, 5);
    expect(snapshot).toEqual({
      swingHigh: undefined,
      swingLow: undefined,
      bias: 0,
      signalType: undefined,
      signalDir: undefined,
      sweepLow: false,
      sweepHigh: false,
      nextBullish: undefined,
      nextBearish: undefined,
    });
  });

  test("higher high + higher low ⇒ biais haussier, BOS sur la cassure, niveaux à surveiller exposés", () => {
    const snapshot = computeStructure(UPTREND_BARS, 2);

    expect(snapshot.swingHigh).toBe(122); // sommet #2 > sommet #1 (115)
    expect(snapshot.swingLow).toBe(103); // creux #2 > creux #1 (85)
    expect(snapshot.bias).toBe(1);
    expect(snapshot.signalType).toBe("BOS"); // cassure dans le sens du trend en cours (haussier)
    expect(snapshot.signalDir).toBe(1);
    expect(snapshot.sweepLow).toBe(false);
    expect(snapshot.sweepHigh).toBe(false);
    // 122 n'a jamais cassé, casser continuerait la tendance haussière ⇒ BOS.
    expect(snapshot.nextBullish).toEqual({ level: 122, type: "BOS" });
    // 103 n'a jamais cassé, casser inverserait la tendance haussière ⇒ CHoCH.
    expect(snapshot.nextBearish).toEqual({ level: 103, type: "CHoCH" });
  });

  test("une cassure qui inverse la tendance devient un CHoCH", () => {
    // Même série, plus une bougie qui clôture sous le niveau bas encore surveillé (103).
    const bars = [...UPTREND_BARS, bar(10, 105, 95, 95)];
    const snapshot = computeStructure(bars, 2);

    expect(snapshot.signalType).toBe("CHoCH"); // inverse le trend haussier établi par le BOS précédent
    expect(snapshot.signalDir).toBe(-1);
    expect(snapshot.bias).toBe(1); // le biais (dernier pivots swing) ne bouge pas, indépendant du signal
    // 103 vient d'être consommé par le CHoCH, aucun nouveau pivot bas ne s'est reformé depuis.
    expect(snapshot.nextBearish).toBeUndefined();
    // 122 n'a toujours pas cassé ; le trend est repassé baissier ⇒ le casser inverserait de nouveau : CHoCH.
    expect(snapshot.nextBullish).toEqual({ level: 122, type: "CHoCH" });
  });

  test("mèche sous le swing low puis clôture repassée au-dessus ⇒ sweepLow", () => {
    const length = 2;
    const bars: CtraderTrendbar[] = [
      bar(0, 100, 95),
      bar(1, 90, 85), // devient le creux surveillé (85) une fois confirmé
      bar(2, 95, 90),
      bar(3, 105, 100),
      bar(4, 104, 99),
      bar(5, 106, 80, 107), // mèche sous 85, clôture (107) repassée au-dessus
    ];

    const snapshot = computeStructure(bars, length);

    expect(snapshot.swingLow).toBe(85);
    expect(snapshot.swingHigh).toBeUndefined(); // aucun pivot high ne se forme dans cette série
    expect(snapshot.bias).toBe(0);
    expect(snapshot.sweepLow).toBe(true);
    expect(snapshot.sweepHigh).toBe(false);
    // Aucun trend établi (0) : le premier break, quel que soit son sens, est toujours classé BOS.
    expect(snapshot.nextBearish).toEqual({ level: 85, type: "BOS" });
    expect(snapshot.nextBullish).toBeUndefined();
  });
});
