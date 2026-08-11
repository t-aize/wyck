import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { computeAdx, computeEmaStack } from "./filters.ts";

/** Bougie synthétique. */
function bar(i: number, high: number, low: number, close = (high + low) / 2): CtraderTrendbar {
  return { timestamp: i * 60_000, open: (high + low) / 2, high, low, close, volume: 0 };
}

describe("computeEmaStack", () => {
  function closeBar(i: number, close: number): CtraderTrendbar {
    return { timestamp: i, open: close, high: close + 1, low: close - 1, close, volume: 0 };
  }

  test("tendance haussière régulière ⇒ EMA rapide > EMA lente", () => {
    const bars = Array.from({ length: 60 }, (_, i) => closeBar(i, 100 + i * 0.5));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBe(true);
  });

  test("tendance baissière régulière ⇒ EMA rapide < EMA lente", () => {
    const bars = Array.from({ length: 60 }, (_, i) => closeBar(i, 200 - i * 0.5));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBe(false);
  });

  test("pas assez de bougies pour l'EMA lente ⇒ undefined", () => {
    const bars = Array.from({ length: 10 }, (_, i) => closeBar(i, 100 + i));
    expect(computeEmaStack(bars, { fast: 10, slow: 30 })).toBeUndefined();
  });
});

describe("computeAdx", () => {
  test("pas assez de bougies ⇒ undefined", () => {
    const bars = Array.from({ length: 10 }, (_, i) => bar(i, 101, 99, 100));
    expect(computeAdx(bars, 14)).toBeUndefined();
  });

  test('tendance nette et régulière ⇒ ADX élevé (> 25, seuil usuel "en tendance")', () => {
    const bars = Array.from({ length: 40 }, (_, i) => {
      const base = 100 + i * 2;
      return bar(i, base + 1, base - 1, base + 0.5);
    });
    expect(computeAdx(bars, 14)!).toBeGreaterThan(25);
  });

  test("marché plat et bruité ⇒ ADX bas (< 25)", () => {
    let seed = 42;
    function rand() {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    }
    const bars = Array.from({ length: 40 }, (_, i) => {
      const base = 100 + (rand() - 0.5) * 2;
      return bar(i, base + 1, base - 1, base);
    });
    expect(computeAdx(bars, 14)!).toBeLessThan(25);
  });
});
