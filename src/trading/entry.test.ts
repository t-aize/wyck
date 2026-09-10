import { describe, expect, test } from "bun:test";
import { OrderType, TradeSide } from "@aurum/ctrader";
import { inferOrderType, resolveEntry } from "./entry.ts";

describe("inferOrderType", () => {
  test("returns MARKET when entry equals the reference price", () => {
    expect(inferOrderType(TradeSide.BUY, 2000, 2000)).toBe(OrderType.MARKET);
    expect(inferOrderType(TradeSide.SELL, 2000, 2000)).toBe(OrderType.MARKET);
  });

  test("BUY: entry above reference is a STOP, below is a LIMIT", () => {
    expect(inferOrderType(TradeSide.BUY, 2010, 2000)).toBe(OrderType.STOP);
    expect(inferOrderType(TradeSide.BUY, 1990, 2000)).toBe(OrderType.LIMIT);
  });

  test("SELL: entry below reference is a STOP, above is a LIMIT", () => {
    expect(inferOrderType(TradeSide.SELL, 1990, 2000)).toBe(OrderType.STOP);
    expect(inferOrderType(TradeSide.SELL, 2010, 2000)).toBe(OrderType.LIMIT);
  });
});

describe("resolveEntry", () => {
  test("'market' entry resolves to the reference price and a MARKET order", () => {
    expect(resolveEntry(TradeSide.BUY, "market", 2001)).toEqual({
      entryPrice: 2001,
      orderType: OrderType.MARKET,
    });
  });

  test("a numeric entry keeps its price and infers the order type", () => {
    expect(resolveEntry(TradeSide.BUY, 2010, 2000)).toEqual({
      entryPrice: 2010,
      orderType: OrderType.STOP,
    });
    expect(resolveEntry(TradeSide.SELL, 2010, 2000)).toEqual({
      entryPrice: 2010,
      orderType: OrderType.LIMIT,
    });
  });
});
