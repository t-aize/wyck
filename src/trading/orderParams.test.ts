import { describe, expect, test } from "bun:test";
import { OrderType } from "../ctrader/protocol/OrderType.ts";
import { TradeSide } from "../ctrader/protocol/TradeSide.ts";
import { formatTradeSummary, toCreateOrderParams } from "./orderParams.ts";
import type { PreparedTrade } from "./types.ts";

function trade(overrides: Partial<PreparedTrade> = {}): PreparedTrade {
  return {
    orderType: OrderType.MARKET,
    tradeSide: TradeSide.BUY,
    entryPrice: 2000,
    stopLoss: 1990,
    takeProfit: 2020,
    volume: 1000,
    volumeLots: 0.1,
    riskAmount: 100,
    riskPercent: 1,
    rewardAmount: 200,
    ...overrides,
  };
}

describe("toCreateOrderParams", () => {
  test("MARKET order: SL/TP encoded as relative points from entry", () => {
    const params = toCreateOrderParams(1, trade());
    expect(params).toEqual({
      symbolId: 1,
      orderType: OrderType.MARKET,
      tradeSide: TradeSide.BUY,
      volume: 1000,
      label: "aurum",
      relativeStopLoss: 1_000_000, // |2000-1990| * PRICE_SCALE
      relativeTakeProfit: 2_000_000, // |2000-2020| * PRICE_SCALE
    });
  });

  test("LIMIT order: absolute SL/TP, limitPrice set, stopPrice absent", () => {
    const params = toCreateOrderParams(1, trade({ orderType: OrderType.LIMIT, entryPrice: 1990 }));
    expect(params).toMatchObject({
      orderType: OrderType.LIMIT,
      limitPrice: 1990,
      stopPrice: undefined,
      stopLoss: 1990,
      takeProfit: 2020,
    });
  });

  test("STOP order: absolute SL/TP, stopPrice set, limitPrice absent", () => {
    const params = toCreateOrderParams(1, trade({ orderType: OrderType.STOP, entryPrice: 2010 }));
    expect(params).toMatchObject({
      orderType: OrderType.STOP,
      stopPrice: 2010,
      limitPrice: undefined,
      stopLoss: 1990,
      takeProfit: 2020,
    });
  });
});

describe("formatTradeSummary", () => {
  test("formats every field into one line", () => {
    expect(formatTradeSummary(trade())).toBe(
      "BUY MARKET 2000.00 · SL 1990.00 · TP 2020.00 · 0.10 lots · risque 100.00 (1%)",
    );
  });
});
