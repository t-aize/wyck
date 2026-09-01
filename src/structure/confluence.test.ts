import { describe, expect, test } from "bun:test";
import type { StructureReading } from "./bias.ts";
import { computeScalpDirection } from "./confluence.ts";
import type { StructurePeriod } from "./fetch.ts";
import type { StructureBias } from "./types.ts";

function structure(
  m1: StructureBias,
  m5: StructureBias,
  m15: StructureBias,
  h1: StructureBias,
): Record<StructurePeriod, StructureReading> {
  const reading = (bias: StructureBias): StructureReading => ({
    bias,
    resistance: undefined,
    support: undefined,
  });
  return { M_1: reading(m1), M_5: reading(m5), M_15: reading(m15), H_1: reading(h1) };
}

describe("computeScalpDirection", () => {
  test("all four bullish -> strong bullish", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "strong",
      bullishCount: 4,
      bearishCount: 0,
    });
  });

  test("all four bearish -> strong bearish", () => {
    expect(computeScalpDirection(structure("bearish", "bearish", "bearish", "bearish"))).toEqual({
      bias: "bearish",
      strength: "strong",
      bullishCount: 0,
      bearishCount: 4,
    });
  });

  test("H1 bullish confirmed by M15 only, M5/M1 neutral -> weak bullish (1/3 agreement)", () => {
    expect(computeScalpDirection(structure("neutral", "neutral", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "weak",
      bullishCount: 2,
      bearishCount: 0,
    });
  });

  test("H1 bearish confirmed by M5 only, M15/M1 neutral -> weak bearish (1/3 agreement)", () => {
    expect(computeScalpDirection(structure("neutral", "bearish", "neutral", "bearish"))).toEqual({
      bias: "bearish",
      strength: "weak",
      bullishCount: 0,
      bearishCount: 2,
    });
  });

  test("H1 bullish confirmed by M1 only, M5/M15 neutral -> weak bullish (1/3 agreement)", () => {
    expect(computeScalpDirection(structure("bullish", "neutral", "neutral", "bullish"))).toEqual({
      bias: "bullish",
      strength: "weak",
      bullishCount: 2,
      bearishCount: 0,
    });
  });

  test("H1 neutral even with M15/M5/M1 all bullish -> mixed (no trend filter to confirm)", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "bullish", "neutral"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 3,
      bearishCount: 0,
    });
  });

  test("M15 opposes H1 but M5/M1 agree -> weak bullish (2/3 agreement, opposition no longer vetoes)", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "bearish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "moderate",
      bullishCount: 3,
      bearishCount: 1,
    });
  });

  test("M5 opposes H1 but M15/M1 agree -> moderate bullish (2/3 agreement)", () => {
    expect(computeScalpDirection(structure("bullish", "bearish", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "moderate",
      bullishCount: 3,
      bearishCount: 1,
    });
  });

  test("M1 opposes H1 but M15/M5 agree -> moderate bullish (2/3 agreement)", () => {
    expect(computeScalpDirection(structure("bearish", "bullish", "bullish", "bullish"))).toEqual({
      bias: "bullish",
      strength: "moderate",
      bullishCount: 3,
      bearishCount: 1,
    });
  });

  test("only M1 agrees with H1, M5/M15 opposed -> weak bearish (1/3 agreement)", () => {
    expect(computeScalpDirection(structure("bearish", "bullish", "bullish", "bearish"))).toEqual({
      bias: "bearish",
      strength: "weak",
      bullishCount: 2,
      bearishCount: 2,
    });
  });

  test("H1 bearish, all three lower opposed (bullish) -> mixed (0/3 agreement, H1 isolated)", () => {
    expect(computeScalpDirection(structure("bullish", "bullish", "bullish", "bearish"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 3,
      bearishCount: 1,
    });
  });

  test("all neutral -> mixed", () => {
    expect(computeScalpDirection(structure("neutral", "neutral", "neutral", "neutral"))).toEqual({
      bias: "mixed",
      strength: undefined,
      bullishCount: 0,
      bearishCount: 0,
    });
  });
});
