import { describe, expect, test } from "bun:test";
import type { CtraderTrendbar } from "../../ctrader/schemas.ts";
import { dropFormingBar } from "./bars.ts";

/** Bougie synthétique — seuls high/low/close comptent pour dropFormingBar. */
function bar(i: number, high: number, low: number, close = high): CtraderTrendbar {
  return { timestamp: i, open: low, high, low, close, volume: 0 };
}

describe("dropFormingBar", () => {
  const periodMs = 60_000;

  test("écarte la dernière bougie si elle n'est pas encore close", () => {
    const bars = [bar(0, 10, 5), bar(1, 11, 6)];
    bars[1]!.timestamp = 100_000;
    const now = 130_000; // 100_000 + 60_000 > now ⇒ encore en formation
    expect(dropFormingBar(bars, periodMs, now)).toEqual([bars[0]!]);
  });

  test("garde toutes les bougies si la dernière est déjà close", () => {
    const bars = [bar(0, 10, 5), bar(1, 11, 6)];
    bars[1]!.timestamp = 100_000;
    const now = 170_000; // 100_000 + 60_000 <= now ⇒ close
    expect(dropFormingBar(bars, periodMs, now)).toEqual(bars);
  });
});
