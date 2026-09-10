import { describe, expect, test } from "bun:test";
import { newsProfile } from "../profile/profile.ts";
import { classifyImpact, isDefaultVisible, isRelevant } from "./relevance.ts";

const gold = newsProfile({ symbolName: "XAUUSD" });
const eurusd = newsProfile({ symbolName: "EURUSD" });
const us100 = newsProfile({ symbolName: "US100" });
const btc = newsProfile({ symbolName: "BTCUSD" });

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

describe("isRelevant", () => {
  test("XAUUSD: any USD event is relevant", () => {
    expect(isRelevant({ country: "USD", title: "Retail Sales m/m" }, gold)).toBe(true);
  });

  test("XAUUSD: a non-USD event mentioning gold is relevant", () => {
    expect(isRelevant({ country: "CHN", title: "Gold Reserves" }, gold)).toBe(true);
    expect(isRelevant({ country: "IND", title: "Precious Metal Imports" }, gold)).toBe(true);
  });

  test("XAUUSD: a non-USD event with no gold keyword is not relevant", () => {
    expect(isRelevant({ country: "EUR", title: "CPI m/m" }, gold)).toBe(false);
  });

  test("EURUSD: both EUR and USD events are relevant, JPY is not", () => {
    expect(isRelevant({ country: "EUR", title: "CPI m/m" }, eurusd)).toBe(true);
    expect(isRelevant({ country: "USD", title: "NFP" }, eurusd)).toBe(true);
    expect(isRelevant({ country: "JPY", title: "Tankan" }, eurusd)).toBe(false);
  });

  test("US100: USD events are relevant, EUR are not (unless the title names the index)", () => {
    expect(isRelevant({ country: "USD", title: "CPI m/m" }, us100)).toBe(true);
    expect(isRelevant({ country: "EUR", title: "German CPI" }, us100)).toBe(false);
    expect(isRelevant({ country: "EUR", title: "Nasdaq futures" }, us100)).toBe(true);
  });

  test("BTCUSD: USD events and bitcoin titles are relevant", () => {
    expect(isRelevant({ country: "USD", title: "FOMC Statement" }, btc)).toBe(true);
    expect(isRelevant({ country: "EUR", title: "Bitcoin regulation" }, btc)).toBe(true);
    expect(isRelevant({ country: "JPY", title: "Tankan" }, btc)).toBe(false);
  });
});

describe("isDefaultVisible", () => {
  test("requires both relevance and high impact", () => {
    expect(
      isDefaultVisible(
        {
          title: "NFP",
          country: "USD",
          date: "",
          impact: "High",
          forecast: "",
          previous: "",
          timestamp: 0,
        },
        gold,
      ),
    ).toBe(true);
    expect(
      isDefaultVisible(
        {
          title: "NFP",
          country: "USD",
          date: "",
          impact: "Low",
          forecast: "",
          previous: "",
          timestamp: 0,
        },
        gold,
      ),
    ).toBe(false);
  });
});
