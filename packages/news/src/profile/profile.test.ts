import { describe, expect, test } from "bun:test";
import { classifyAssetClass } from "./classify.ts";
import { newsProfile } from "./profile.ts";
import { inferBaseQuote, normalizeSymbolName } from "./symbol.ts";

describe("normalizeSymbolName", () => {
  test("strips broker suffixes", () => {
    expect(normalizeSymbolName("XAUUSD.r")).toBe("XAUUSD");
    expect(normalizeSymbolName("EURUSD.a")).toBe("EURUSD");
    expect(normalizeSymbolName("US100_SB")).toBe("US100");
    expect(normalizeSymbolName("btcusd")).toBe("BTCUSD");
  });
});

describe("inferBaseQuote", () => {
  test("prefers explicit assets", () => {
    expect(inferBaseQuote("whatever", "XAU", "USD")).toEqual({ base: "XAU", quote: "USD" });
  });

  test("maps USDT/USDC quotes to USD", () => {
    expect(inferBaseQuote("BTCUSDT")).toEqual({ base: "BTC", quote: "USD" });
    expect(inferBaseQuote("ETH", "ETH", "USDT")).toEqual({ base: "ETH", quote: "USD" });
  });

  test("splits standard 6-letter names", () => {
    expect(inferBaseQuote("EURUSD")).toEqual({ base: "EUR", quote: "USD" });
    expect(inferBaseQuote("USDJPY")).toEqual({ base: "USD", quote: "JPY" });
    expect(inferBaseQuote("XAUUSD")).toEqual({ base: "XAU", quote: "USD" });
  });

  test("keeps index tickers as base", () => {
    expect(inferBaseQuote("US100")).toEqual({ base: "US100", quote: "USD" });
    expect(inferBaseQuote("GER40")).toEqual({ base: "GER40", quote: "USD" });
  });
});

describe("newsProfile", () => {
  test("XAUUSD is a metal driven by USD + gold keywords", () => {
    const profile = newsProfile({ symbolName: "XAUUSD" });
    expect(profile.assetClass).toBe("metal");
    expect(profile.countries).toEqual(["USD"]);
    expect(profile.keywords.test("Gold Reserves")).toBe(true);
  });

  test("EURUSD is forex covering both currencies", () => {
    const profile = newsProfile({ symbolName: "EURUSD" });
    expect(profile.assetClass).toBe("forex");
    expect(profile.countries).toEqual(["EUR", "USD"]);
  });

  test("US100 / USTEC / NAS100 all map to a USD index", () => {
    for (const name of ["US100", "USTEC", "NAS100", "US100.r"]) {
      const profile = newsProfile({ symbolName: name });
      expect(profile.assetClass).toBe("index");
      expect(profile.countries).toEqual(["USD"]);
    }
  });

  test("GER40 maps to EUR", () => {
    const profile = newsProfile({ symbolName: "GER40" });
    expect(profile.assetClass).toBe("index");
    expect(profile.countries).toEqual(["EUR"]);
  });

  test("UK100 maps to GBP", () => {
    expect(newsProfile({ symbolName: "UK100" }).countries).toEqual(["GBP"]);
  });

  test("BTCUSD is crypto driven by USD + crypto keywords", () => {
    const profile = newsProfile({ symbolName: "BTCUSD" });
    expect(profile.assetClass).toBe("crypto");
    expect(profile.countries).toEqual(["USD"]);
    expect(profile.keywords.test("Bitcoin ETF inflows")).toBe(true);
  });

  test("USOIL is energy driven by USD", () => {
    const profile = newsProfile({ symbolName: "USOIL" });
    expect(profile.assetClass).toBe("energy");
    expect(profile.countries).toEqual(["USD"]);
    expect(profile.keywords.test("Crude Oil Inventories")).toBe(true);
  });

  test("XAUEUR is a metal quoted in EUR", () => {
    const profile = newsProfile({ symbolName: "XAUEUR" });
    expect(profile.assetClass).toBe("metal");
    expect(profile.quote).toBe("EUR");
    expect(profile.countries).toEqual(["EUR"]);
  });
});

describe("classifyAssetClass", () => {
  test("does not tag EURUSD as metal just because USD is the quote", () => {
    expect(classifyAssetClass("EUR", "USD", "EURUSD")).toBe("forex");
  });
});
