import { describe, expect, test } from "bun:test";
import type { AmendablePosition, ClosablePosition, CtraderOrder } from "../ctrader/schemas.ts";
import { toAmendOrderParams, toAmendPositionParams, toClosePositionParams } from "./amendParams.ts";

const order: CtraderOrder = {
  orderId: 42,
  symbolId: 1,
  orderType: "LIMIT",
  tradeSide: "BUY",
  volume: 1000,
  limitPrice: 1990,
  stopPrice: undefined,
  stopLoss: 1980,
  takeProfit: 2020,
  expirationTimestamp: undefined,
};

const position: AmendablePosition = {
  id: 7,
  side: "SELL",
  volumeLots: 0.5,
  entry: 2000,
  stopLoss: 2010,
  takeProfit: 1980,
  swap: -1.2,
};

describe("toAmendOrderParams", () => {
  test("with no changes, resends every field verbatim from the order", () => {
    expect(toAmendOrderParams(order)).toEqual({
      orderId: 42,
      volume: 1000,
      limitPrice: 1990,
      stopPrice: undefined,
      stopLoss: 1980,
      takeProfit: 2020,
      expirationTimestamp: undefined,
    });
  });

  test("an explicit change overrides the existing value", () => {
    const params = toAmendOrderParams(order, { stopLoss: 1975 });
    expect(params.stopLoss).toBe(1975);
    expect(params.takeProfit).toBe(2020); // untouched
  });

  test("an explicitly-undefined change does not erase the existing value", () => {
    const params = toAmendOrderParams(order, { stopLoss: undefined, takeProfit: 2025 });
    expect(params.stopLoss).toBe(1980); // kept from `order`, not erased
    expect(params.takeProfit).toBe(2025);
  });
});

describe("toAmendPositionParams", () => {
  test("with no changes, resends the position's current SL/TP", () => {
    expect(toAmendPositionParams(position)).toEqual({
      positionId: 7,
      stopLoss: 2010,
      takeProfit: 1980,
    });
  });

  test("an explicit change overrides, an explicit undefined does not erase", () => {
    const params = toAmendPositionParams(position, { stopLoss: 2005, takeProfit: undefined });
    expect(params).toEqual({ positionId: 7, stopLoss: 2005, takeProfit: 1980 });
  });
});

describe("toClosePositionParams", () => {
  test("converts volumeLots back to API volume", () => {
    const closable: ClosablePosition = { ...position, volumeLots: 1.5 };
    expect(toClosePositionParams(closable)).toEqual({ positionId: 7, volume: 15_000 });
  });
});
