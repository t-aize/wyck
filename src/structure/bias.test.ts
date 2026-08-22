import { describe, expect, test } from "bun:test";
import { computeStructure } from "./bias.ts";
import type { StructureBar, SwingPoint } from "./types.ts";

function bar(close: number): StructureBar {
  return { timestamp: 0, high: close, low: close, close };
}

function swing(
  type: "high" | "low",
  index: number,
  price: number,
  label: SwingPoint["label"] = undefined,
): SwingPoint {
  return { type, index, confirmedAtIndex: index, timestamp: index, price, label };
}

describe("computeStructure", () => {
  test("no swings at all -> neutral bias, no resistance/support", () => {
    const bars = [bar(100), bar(200), bar(50)];
    expect(computeStructure(bars, [])).toEqual({
      bias: "neutral",
      resistance: undefined,
      support: undefined,
    });
  });

  test("a close breaking above the first confirmed swing high sets an initial bullish bias", () => {
    const bars = [bar(18), bar(21), bar(22)]; // index 1 breaks the swing high at 20
    const swings = [swing("high", 0, 20)];
    expect(computeStructure(bars, swings)).toEqual({
      bias: "bullish",
      resistance: 20,
      support: undefined,
    });
  });

  test("a close breaking below the first confirmed swing low sets an initial bearish bias", () => {
    const bars = [bar(12), bar(9), bar(8)]; // index 1 breaks the swing low at 10
    const swings = [swing("low", 0, 10)];
    expect(computeStructure(bars, swings)).toEqual({
      bias: "bearish",
      resistance: undefined,
      support: 10,
    });
  });

  test("resistance/support always reflect the LAST confirmed swing of each type, not the first", () => {
    const bars = [bar(18), bar(21), bar(22), bar(17), bar(19)];
    const swings = [
      swing("high", 0, 20),
      swing("low", 1, 10),
      swing("high", 2, 25, "HH"),
      swing("low", 3, 15, "HL"),
    ];
    const result = computeStructure(bars, swings);
    expect(result.resistance).toBe(25);
    expect(result.support).toBe(15);
  });

  test("an isolated LH label does NOT flip an established bullish bias without a close breaking the last confirmed low", () => {
    // A(high,20) -> B(low,10) -> break above 20 sets bullish -> C(high,25,"HH") -> D(low,15,"HL")
    // -> E(high,22,"LH"): lower than C, but no close ever drops below D(15) -> bias must stay bullish.
    const bars = [
      bar(18), // i0: below A(20), no break yet
      bar(21), // i1: A(20) revealed at i0, breaks above it -> bullish
      bar(22), // i2: C(25) revealed here, no break either way
      bar(17), // i3: D(15) revealed here, stays above it
      bar(19), // i4: E(22,"LH") revealed here — must NOT flip bias despite the LH label
    ];
    const swings = [
      swing("high", 0, 20),
      swing("low", 1, 10),
      swing("high", 2, 25, "HH"),
      swing("low", 3, 15, "HL"),
      swing("high", 4, 22, "LH"),
    ];

    expect(computeStructure(bars, swings).bias).toBe("bullish");
  });

  test("companion case: an actual close below the last confirmed low DOES flip an established bullish bias to bearish", () => {
    const bars = [
      bar(18), // i0
      bar(21), // i1: breaks above A(20) -> bullish
      bar(22), // i2
      bar(17), // i3: D(15) revealed, stays above it
      bar(14), // i4: closes below D(15) -> CHoCH, flips to bearish
    ];
    const swings = [
      swing("high", 0, 20),
      swing("low", 1, 10),
      swing("high", 2, 25, "HH"),
      swing("low", 3, 15, "HL"),
    ];

    expect(computeStructure(bars, swings).bias).toBe("bearish");
  });

  test("a swing not yet confirmed (confirmedAtIndex beyond the current bar) is not usable for a break check", () => {
    // The swing high at price 20 only confirms at index 5 — a close above 20 at index 1 must NOT
    // trigger a bullish flip yet, since the level isn't knowable at that point without look-ahead.
    const bars = [bar(18), bar(25), bar(25), bar(25), bar(25), bar(25)];
    const confirmedLate = [{ ...swing("high", 0, 20), confirmedAtIndex: 5 }];
    expect(computeStructure(bars, confirmedLate).bias).toBe("bullish");

    // Confirmed beyond the end of the series: never gets to trigger a break at all.
    const neverConfirmed = [{ ...swing("high", 0, 20), confirmedAtIndex: 99 }];
    const result = computeStructure(bars, neverConfirmed);
    expect(result.bias).toBe("neutral");
    expect(result.resistance).toBeUndefined();
  });
});
