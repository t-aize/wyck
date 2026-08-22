import { describe, expect, test } from "bun:test";
import { prepareTrade } from "./prepare.ts";
import { fakeCtraderClient, runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

// equity=1_000_000, moneyDigits=2 -> riskAmount = (1_000_000/100) * (riskPercent/100)
// riskPercent=1 -> riskAmount = 100.
const client = fakeCtraderClient({
  prices: [{ bid: 200_000_000, ask: 200_100_000 }], // displayed bid=2000, ask=2001
  equity: 1_000_000,
  moneyDigits: 2,
});

describe("prepareTrade", () => {
  test("BUY at market: direction/entry/volume/reward all derived correctly", () => {
    const trade = runOk(
      prepareTrade(client, 1, {
        entry: "market",
        riskPercent: 1,
        stopLoss: 1990,
        takeProfit: 2020,
      }),
    );
    expect(trade).toEqual({
      orderType: "MARKET",
      tradeSide: "BUY",
      entryPrice: 2001, // ask, side BUY at market
      stopLoss: 1990,
      takeProfit: 2020,
      volume: 900, // computeVolume(100, stopDistance=11)
      volumeLots: 0.09,
      riskAmount: 100,
      riskPercent: 1,
      rewardAmount: 171, // (900/100) * targetDistance(19)
    });
  });

  test("SELL with an explicit LIMIT entry (deduced from SL above / TP below entry)", () => {
    const trade = runOk(
      prepareTrade(client, 1, { entry: 2010, riskPercent: 1, stopLoss: 2020, takeProfit: 1990 }),
    );
    expect(trade).toEqual({
      orderType: "LIMIT",
      tradeSide: "SELL",
      entryPrice: 2010,
      stopLoss: 2020,
      takeProfit: 1990,
      volume: 1000, // computeVolume(100, stopDistance=10)
      volumeLots: 0.1,
      riskAmount: 100,
      riskPercent: 1,
      rewardAmount: 200, // (1000/100) * targetDistance(20)
    });
  });

  test("rejects an invalid risk% before touching the network", () => {
    const error = runFail(
      prepareTrade(client, 1, {
        entry: "market",
        riskPercent: 0,
        stopLoss: 1990,
        takeProfit: 2020,
      }),
    );
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Risque invalide");
  });

  test("rejects identical SL/TP", () => {
    const error = runFail(
      prepareTrade(client, 1, {
        entry: "market",
        riskPercent: 1,
        stopLoss: 2000,
        takeProfit: 2000,
      }),
    );
    expect(error.message).toContain("identiques");
  });

  test("rejects a BUY whose SL/TP are on the wrong side of entry", () => {
    // stopLoss < takeProfit -> inferred BUY, but stopLoss(2005) is not below entry(2001)
    const error = runFail(
      prepareTrade(client, 1, {
        entry: "market",
        riskPercent: 1,
        stopLoss: 2005,
        takeProfit: 2020,
      }),
    );
    expect(error.message).toContain("Incohérent pour un achat");
  });

  test("rejects a SELL whose SL/TP are on the wrong side of entry", () => {
    // stopLoss(1990) > takeProfit(1980) -> inferred SELL, but stopLoss must be ABOVE entry(2000)
    const error = runFail(
      prepareTrade(client, 1, { entry: 2000, riskPercent: 1, stopLoss: 1990, takeProfit: 1980 }),
    );
    expect(error.message).toContain("Incohérent pour une vente");
  });

  test("propagates a computeVolume failure (risk% too small for the stop distance)", () => {
    const error = runFail(
      prepareTrade(client, 1, {
        entry: "market",
        riskPercent: 0.0001,
        stopLoss: 1990,
        takeProfit: 2020,
      }),
    );
    expect(error.message).toContain("sous le minimum");
  });
});
