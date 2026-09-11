import { describe, expect, test } from "bun:test";
import { OrderType } from "../ctrader/protocol/OrderType.ts";
import { TradeSide } from "../ctrader/protocol/TradeSide.ts";
import { prepareAtrTrade } from "./prepareAtr.ts";
import { fakeCtraderClient, runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

// 15 M5 bars, flat high-low range of 2 (displayed) -> ATR(14) = 2.
const flatTrendbars = Array.from({ length: 15 }, (_, i) => ({
  timestamp: i,
  open: 200_000_000,
  high: 200_100_000,
  low: 199_900_000,
  close: 200_000_000,
  volume: 1,
}));

// equity=1_000_000, moneyDigits=2 -> riskAmount = (1_000_000/100) * (riskPercent/100)
// riskPercent=1 -> riskAmount = 100. stopDistance = ATR = 2 -> volume = computeVolume(100, 2).
const client = fakeCtraderClient({
  prices: [{ bid: 200_000_000, ask: 200_100_000 }], // displayed bid=2000, ask=2001
  equity: 1_000_000,
  moneyDigits: 2,
  trendbars: flatTrendbars,
});

describe("prepareAtrTrade", () => {
  test("BUY at market: entry resolves to ask, SL/TP derived from ATR and RR", () => {
    const trade = runOk(
      prepareAtrTrade(client, 1, {
        tradeSide: TradeSide.BUY,
        entry: "market",
        riskPercent: 1,
        rewardRiskRatio: 1.2,
      }),
    );
    expect(trade.orderType).toBe(OrderType.MARKET);
    expect(trade.tradeSide).toBe(TradeSide.BUY);
    expect(trade.entryPrice).toBe(2001); // ask
    expect(trade.stopLoss).toBe(1999); // entry - ATR(2)
    expect(trade.takeProfit).toBeCloseTo(2003.4, 10); // entry + ATR*1.2
    expect(trade.riskAmount).toBe(100);
    // computeVolume(100, stopDistance=2): ounces=50 -> volume=round(50*100/100)*100=5000
    expect(trade.volume).toBe(5000);
    expect(trade.volumeLots).toBe(0.5);
  });

  test("SELL with an explicit entry price: SL above, TP below", () => {
    const trade = runOk(
      prepareAtrTrade(client, 1, {
        tradeSide: TradeSide.SELL,
        entry: 1995,
        riskPercent: 1,
        rewardRiskRatio: 2,
      }),
    );
    expect(trade.entryPrice).toBe(1995);
    expect(trade.stopLoss).toBe(1997); // entry + ATR(2)
    expect(trade.takeProfit).toBe(1991); // entry - ATR*2
    // computeVolume(100, stopDistance=2) same as above -> 5000
    expect(trade.volume).toBe(5000);
    expect(trade.rewardAmount).toBe(200); // (5000/100) * (ATR(2)*RR(2))
  });

  test("rejects an invalid risk% before touching the network", () => {
    const error = runFail(
      prepareAtrTrade(client, 1, {
        tradeSide: TradeSide.BUY,
        entry: "market",
        riskPercent: 0,
        rewardRiskRatio: 1.2,
      }),
    );
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Risque invalide");
  });

  test("rejects a non-positive reward:risk ratio", () => {
    const error = runFail(
      prepareAtrTrade(client, 1, {
        tradeSide: TradeSide.BUY,
        entry: "market",
        riskPercent: 1,
        rewardRiskRatio: 0,
      }),
    );
    expect(error.message).toContain("Ratio reward:risk invalide");
  });

  test("propagates an ATR fetch failure (not enough candles)", () => {
    const thinClient = fakeCtraderClient({ trendbars: [] });
    const error = runFail(
      prepareAtrTrade(thinClient, 1, {
        tradeSide: TradeSide.BUY,
        entry: "market",
        riskPercent: 1,
        rewardRiskRatio: 1.2,
      }),
    );
    expect(error.message).toContain("Pas assez de bougies");
  });

  test("propagates a computeVolume failure (risk% too small for the ATR distance)", () => {
    const error = runFail(
      prepareAtrTrade(client, 1, {
        tradeSide: TradeSide.BUY,
        entry: "market",
        riskPercent: 0.0001,
        rewardRiskRatio: 1.2,
      }),
    );
    expect(error.message).toContain("sous le minimum");
  });
});
