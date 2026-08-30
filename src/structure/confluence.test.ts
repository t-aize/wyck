import { describe, expect, test } from "bun:test";
import type { StructureReading } from "./bias.ts";
import { computeScalpDirection } from "./confluence.ts";
import type { StructurePeriod } from "./fetch.ts";
import type { StructureBias } from "./types.ts";

function structure(
  m5: StructureBias,
  m15: StructureBias,
  h1: StructureBias,
): Record<StructurePeriod, StructureReading> {
  const reading = (bias: StructureBias): StructureReading => ({
    bias,
    resistance: undefined,
    support: undefined,
  });
  return { M_5: reading(m5), M_15: reading(m15), H_1: reading(h1) };
}

describe("computeScalpDirection", () => {
  test("all three bullish -> strong bullish", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "strong",
      bullishCount: 3,
      bearishCount: 0,
    });
  });

  test("all three bearish -> strong bearish", () => {
    expect(computeScalpDirection(structure("bearish", "bearish", "bearish"))).toEqual({
      bias: "bearish",
      strength: "strong",
      bullishCount: 0,
      bearishCount: 3,
    });
  });

  test("H1 bullish confirmed by M15 only, M5 neutral -> moderate bullish", () => {
    expect(computeScalpDirection(structure("neutral", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "moderate",
      bullishCount: 2,
      bearishCount: 0,
    });
  });

  test("H1 bearish confirmed by M5 only, M15 neutral -> moderate bearish", () => {
    expect(computeScalpDirection(structure("bearish", "neutral", "bearish"))).toEqual({
      bias: "bearish",
      strength: "moderate",
      bullishCount: 0,
      bearishCount: 2,
    });
  });

  test("H1 neutral even with M5/M15 both bullish -> mixed (no trend filter to confirm)", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "neutral"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 2,
      bearishCount: 0,
    });
  });

  test("M15 opposes H1 -> mixed even though M5 agrees with H1", () => {
    expect(computeScalpDirection(structure("bullish", "bearish", "bullish"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 2,
      bearishCount: 1,
    });
  });

  test("M5 opposes H1 -> mixed even though M15 agrees with H1", () => {
    expect(computeScalpDirection(structure("bearish", "bullish", "bullish"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 2,
      bearishCount: 1,
    });
  });

  test("all neutral -> mixed", () => {
    expect(computeScalpDirection(structure("neutral", "neutral", "neutral"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 0,
      bearishCount: 0,
    });
  });
});
