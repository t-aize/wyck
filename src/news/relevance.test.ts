import { describe, expect, test } from "bun:test";
import { classifyImpact, isGoldRelevant } from "./relevance.ts";

describe("classifyImpact", () => {
  test("parses a valid impact regardless of case/whitespace", () => {
    expect(classifyImpact("High")).toBe("high");
    expect(classifyImpact("  MEDIUM  ")).toBe("medium");
    expect(classifyImpact("low")).toBe("low");
  });

  test("falls back to 'other' for anything not in the enum", () => {
    expect(classifyImpact("critical")).toBe("other");
    expect(classifyImpact("")).toBe("other");
  });
});

describe("isGoldRelevant", () => {
  test("any USD event is relevant, regardless of title", () => {
    expect(isGoldRelevant({ country: "USD", title: "Retail Sales m/m" })).toBe(true);
  });

  test("a non-USD event mentioning gold/XAU/precious metal is relevant", () => {
    expect(isGoldRelevant({ country: "CHN", title: "Gold Reserves" })).toBe(true);
    expect(isGoldRelevant({ country: "IND", title: "Precious Metal Imports" })).toBe(true);
    expect(isGoldRelevant({ country: "AUD", title: "XAU Demand Report" })).toBe(true);
  });

  test("a non-USD event with no gold keyword is not relevant", () => {
    expect(isGoldRelevant({ country: "EUR", title: "CPI m/m" })).toBe(false);
  });
});
