import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/client.ts";
import { detectLatestSweep } from "./sweep.ts";

/** Bougie synthétique. */
function bar(i: number, high: number, low: number, close = (high + low) / 2): CtraderTrendbar {
  return { timestamp: i * 60_000, open: (high + low) / 2, high, low, close, volume: 0 };
}

describe("detectLatestSweep", () => {
  const setup: CtraderTrendbar[] = [
    bar(0, 90, 85),
    bar(1, 92, 87),
    bar(2, 80, 75), // creux surveillé = 75
    bar(3, 92, 87),
    bar(4, 94, 89),
    bar(5, 90, 85),
    bar(6, 88, 83),
  ];

  test("mèche sous le creux surveillé, clôture repassée au-dessus ⇒ sweepLow", () => {
    const bars = [...setup, bar(7, 84, 70, 82)];
    expect(detectLatestSweep(bars, { left: 2, right: 2 })).toEqual({
      sweepHigh: false,
      sweepLow: true,
    });
  });

  test("clôture qui reste sous le niveau ⇒ vraie cassure, pas un sweep", () => {
    const bars = [...setup, bar(7, 84, 70, 72)];
    expect(detectLatestSweep(bars, { left: 2, right: 2 })).toEqual({
      sweepHigh: false,
      sweepLow: false,
    });
  });
});
