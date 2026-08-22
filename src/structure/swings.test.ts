import { describe, expect, test } from "bun:test";
import { detectSwings } from "./swings.ts";
import type { StructureBar } from "./types.ts";

function bar(timestamp: number, high: number, low: number, close: number): StructureBar {
  return { timestamp, high, low, close };
}

describe("detectSwings", () => {
  test("detects an uptrend's swing highs/lows and classifies HH/HL against the previous same-type swing", () => {
    const bars = [
      bar(0, 10, 8, 9),
      bar(1, 12, 10, 11),
      bar(2, 15, 13, 14), // swing high #1 (first of type, unclassified)
      bar(3, 13, 11, 12),
      bar(4, 11, 9, 10), // swing low #1 (first of type, unclassified)
      bar(5, 13, 11, 12),
      bar(6, 18, 16, 17), // swing high #2: 18 > 15 -> HH
      bar(7, 16, 14, 15),
      bar(8, 14, 12, 13), // swing low #2: 12 > 9 -> HL
      bar(9, 16, 14, 15),
      bar(10, 20, 18, 19),
    ];

    const swings = detectSwings(bars, 2);

    expect(swings).toEqual([
      { type: "high", index: 2, confirmedAtIndex: 4, timestamp: 2, price: 15, label: undefined },
      { type: "low", index: 4, confirmedAtIndex: 6, timestamp: 4, price: 9, label: undefined },
      { type: "high", index: 6, confirmedAtIndex: 8, timestamp: 6, price: 18, label: "HH" },
      { type: "low", index: 8, confirmedAtIndex: 10, timestamp: 8, price: 12, label: "HL" },
    ]);
  });

  test("a swing exactly equal to the previous same-type swing is labeled undefined, not a new HH/LH", () => {
    const bars = [
      bar(0, 10, 8, 9),
      bar(1, 12, 10, 11),
      bar(2, 15, 13, 14), // swing high #1, price 15 (first, unclassified)
      bar(3, 13, 11, 12),
      bar(4, 11, 9, 10), // swing low (first, unclassified)
      bar(5, 13, 11, 12),
      bar(6, 15, 13, 14), // swing high #2, price 15 again -> same level, not HH/LH
      bar(7, 13, 11, 12),
      bar(8, 11, 9, 10),
    ];

    const swings = detectSwings(bars, 2);
    const highs = swings.filter((s) => s.type === "high");

    expect(highs).toHaveLength(2);
    expect(highs[1]).toMatchObject({ price: 15, label: undefined });
  });

  test("on two adjacent bars with an identical extreme, only the earlier one is picked (left strict, right inclusive)", () => {
    const bars = [bar(0, 10, 0, 0), bar(1, 12, 0, 0), bar(2, 12, 0, 0), bar(3, 10, 0, 0)];

    const swings = detectSwings(bars, 1);

    expect(swings).toEqual([
      { type: "high", index: 1, confirmedAtIndex: 2, timestamp: 1, price: 12, label: undefined },
    ]);
  });

  test("returns no swings when there aren't enough bars for even one fractal", () => {
    const bars = [bar(0, 10, 8, 9), bar(1, 12, 10, 11), bar(2, 9, 7, 8)];
    expect(detectSwings(bars, 2)).toEqual([]);
  });

  test("a single wide bar between two narrow ones can be both a swing high and a swing low", () => {
    const bars = [bar(0, 10, 8, 9), bar(1, 15, 3, 9), bar(2, 10, 8, 9)];

    const swings = detectSwings(bars, 1);

    expect(swings).toHaveLength(2);
    expect(swings).toContainEqual(expect.objectContaining({ type: "high", index: 1, price: 15 }));
    expect(swings).toContainEqual(expect.objectContaining({ type: "low", index: 1, price: 3 }));
  });
});
