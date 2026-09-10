import { describe, expect, test } from "bun:test";
import { TradeSide } from "../protocol/TradeSide.ts";
import { mapPosition } from "./CtraderPosition.ts";

describe("mapPosition", () => {
  test("maps a full real-shaped payload", () => {
    const result = mapPosition({
      positionId: 7,
      symbolId: 1,
      tradeSide: "BUY",
      volume: 5000,
      entryPrice: 2000,
      stopLoss: 1990,
      takeProfit: 2020,
      commission: -1,
      swap: -0.5,
    });
    expect(result).toEqual({
      id: 7,
      symbolId: 1,
      side: TradeSide.BUY,
      volume: 5000,
      entry: 2000,
      stopLoss: 1990,
      takeProfit: 2020,
      swap: -0.5,
    });
  });

  test("a pending order's stub position (volume/entryPrice at 0) still maps", () => {
    const result = mapPosition({
      positionId: 8,
      symbolId: 1,
      tradeSide: "SELL",
      volume: 0,
      entryPrice: 0,
    });
    expect(result.id).toBe(8);
    expect(result.symbolId).toBe(1);
    expect(result.side).toBe(TradeSide.SELL);
    expect(result.volume).toBe(0);
    expect(result.entry).toBe(0);
    expect(result.stopLoss).toBeUndefined();
    expect(result.takeProfit).toBeUndefined();
  });

  test("missing/mistyped fields resolve to undefined instead of throwing", () => {
    const result = mapPosition({
      positionId: "not-a-number",
      tradeSide: "HOLD",
    });
    expect(result.id).toBeUndefined();
    expect(result.side).toBeUndefined();
    expect(result.volume).toBeUndefined();
    expect(result.entry).toBeUndefined();
  });

  test("an empty object / non-object maps to all-undefined", () => {
    const empty = {
      id: undefined,
      symbolId: undefined,
      side: undefined,
      volume: undefined,
      entry: undefined,
      stopLoss: undefined,
      takeProfit: undefined,
      swap: undefined,
    };
    expect(mapPosition({})).toEqual(empty);
    expect(mapPosition(null)).toEqual(empty);
  });
});
