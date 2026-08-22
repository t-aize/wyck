import { describe, expect, test } from "bun:test";
import { inferOrderType, resolveEntry } from "./entry.ts";

describe("inferOrderType", () => {
  test("returns MARKET when entry equals the reference price", () => {
    expect(inferOrderType("BUY", 2000, 2000)).toBe("MARKET");
    expect(inferOrderType("SELL", 2000, 2000)).toBe("MARKET");
  });

  test("BUY: entry above reference is a STOP, below is a LIMIT", () => {
    expect(inferOrderType("BUY", 2010, 2000)).toBe("STOP");
    expect(inferOrderType("BUY", 1990, 2000)).toBe("LIMIT");
  });

  test("SELL: entry below reference is a STOP, above is a LIMIT", () => {
    expect(inferOrderType("SELL", 1990, 2000)).toBe("STOP");
    expect(inferOrderType("SELL", 2010, 2000)).toBe("LIMIT");
  });
});

describe("resolveEntry", () => {
  test("'market' entry resolves to the reference price and a MARKET order", () => {
    expect(resolveEntry("BUY", "market", 2001)).toEqual({
      entryPrice: 2001,
      orderType: "MARKET",
    });
  });

  test("a numeric entry keeps its price and infers the order type", () => {
    expect(resolveEntry("BUY", 2010, 2000)).toEqual({ entryPrice: 2010, orderType: "STOP" });
    expect(resolveEntry("SELL", 2010, 2000)).toEqual({ entryPrice: 2010, orderType: "LIMIT" });
  });
});
