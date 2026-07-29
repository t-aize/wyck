import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeStructure } from "./structure.ts";

/** Bougie synthétique — seuls high/low/close comptent pour computeStructure. */
function bar(i: number, high: number, low: number, close = high): CtraderTrendbar {
  return { timestamp: i, open: low, high, low, close, volume: 0 };
}

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
    });
  });

  test("détecte deux pivots ascendants (BOS haussier puis CHoCH baissier, biais haussier)", () => {
    const length = 2;
    const bars: CtraderTrendbar[] = [
      bar(0, 100, 80),
      bar(1, 101, 81),
      bar(2, 110, 90), // pivot HIGH #1
      bar(3, 103, 83),
      bar(4, 104, 84),
      bar(5, 102, 82),
      bar(6, 103, 60), // pivot LOW #1
      bar(7, 105, 75),
      bar(8, 108, 78),
      bar(9, 112, 79, 112), // BOS haussier : clôture > 110 (pivot HIGH #1)
      bar(10, 111, 80),
      bar(11, 113, 82), // pivot HIGH #2 (plus haut que #1)
      bar(12, 110, 85),
      bar(13, 109, 86),
      bar(14, 108, 87),
      bar(15, 107, 65), // pivot LOW #2 (plus haut que #1 ⇒ higher low)
      bar(16, 106, 90),
      bar(17, 105, 95),
      bar(18, 104, 94),
      bar(19, 60, 58, 58), // CHoCH baissier : clôture < 65 (pivot LOW #2)
    ];

    const snapshot = computeStructure(bars, length);

    expect(snapshot.swingHigh).toBe(113);
    expect(snapshot.swingLow).toBe(65);
    expect(snapshot.bias).toBe(1); // higher high (113>110) + higher low (65>60)
    expect(snapshot.signalType).toBe("CHoCH"); // la tendance haussière (BOS à l'indice 9) est cassée
    expect(snapshot.signalDir).toBe(-1);
    expect(snapshot.sweepLow).toBe(false); // vraie cassure : la clôture reste sous le swing, pas de retour à l'intérieur
    expect(snapshot.sweepHigh).toBe(false);
  });

  test("mèche sous le swing low puis clôture repassée au-dessus ⇒ sweepLow", () => {
    const length = 2;
    const bars: CtraderTrendbar[] = [
      bar(0, 100, 90),
      bar(1, 101, 91),
      bar(2, 102, 80), // pivot LOW
      bar(3, 103, 92),
      bar(4, 104, 93),
      bar(5, 105, 94),
      bar(6, 106, 70, 95), // mèche sous 80, clôture (95) repassée au-dessus
    ];

    const snapshot = computeStructure(bars, length);

    expect(snapshot.swingLow).toBe(80);
    expect(snapshot.swingHigh).toBeUndefined(); // aucun pivot high ne se forme dans cette série
    expect(snapshot.bias).toBe(0);
    expect(snapshot.sweepLow).toBe(true);
    expect(snapshot.sweepHigh).toBe(false);
  });
});
