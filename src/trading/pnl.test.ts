import { describe, expect, test } from "bun:test";
import { TradeSide } from "@aurum/ctrader";
import { computeUnrealizedPnl, computeUnrealizedPnlOrUndefined } from "./pnl.ts";

const GOLD_1_LOT = 10_000; // 1.00 lot XAUUSD en volume API

describe("computeUnrealizedPnl", () => {
  test("BUY marks to bid", () => {
    expect(computeUnrealizedPnl(TradeSide.BUY, GOLD_1_LOT, 2000, 2010, 2011)).toBe(1000);
  });

  test("SELL marks to ask", () => {
    expect(computeUnrealizedPnl(TradeSide.SELL, GOLD_1_LOT, 2000, 1990, 1991)).toBe(900);
  });

  test("scales with volume", () => {
    expect(computeUnrealizedPnl(TradeSide.BUY, 25_000, 2000, 2010, 2011)).toBe(2500);
  });

  test("a loss comes back negative", () => {
    expect(computeUnrealizedPnl(TradeSide.BUY, GOLD_1_LOT, 2000, 1990, 1991)).toBe(-1000);
  });
});

describe("computeUnrealizedPnlOrUndefined", () => {
  test("returns undefined if any input is missing", () => {
    expect(
      computeUnrealizedPnlOrUndefined(undefined, GOLD_1_LOT, 2000, 2010, 2011),
    ).toBeUndefined();
    expect(
      computeUnrealizedPnlOrUndefined(TradeSide.BUY, undefined, 2000, 2010, 2011),
    ).toBeUndefined();
    expect(
      computeUnrealizedPnlOrUndefined(TradeSide.BUY, GOLD_1_LOT, undefined, 2010, 2011),
    ).toBeUndefined();
    expect(
      computeUnrealizedPnlOrUndefined(TradeSide.BUY, GOLD_1_LOT, 2000, undefined, 2011),
    ).toBeUndefined();
    expect(
      computeUnrealizedPnlOrUndefined(TradeSide.BUY, GOLD_1_LOT, 2000, 2010, undefined),
    ).toBeUndefined();
  });

  test("delegates to computeUnrealizedPnl once everything is defined", () => {
    expect(computeUnrealizedPnlOrUndefined(TradeSide.BUY, GOLD_1_LOT, 2000, 2010, 2011)).toBe(1000);
  });
});
