import { describe, expect, test } from "bun:test";
import { CtraderPositionSchema } from "./schemas.ts";

describe("CtraderPositionSchema", () => {
  test("maps a full real-shaped payload", () => {
    const result = CtraderPositionSchema.parse({
      positionId: 7,
      symbolId: 1,
      tradeSide: "BUY",
      volume: 5000, // 1/100 oz -> 0.5 lot
      entryPrice: 2000,
      stopLoss: 1990,
      takeProfit: 2020,
      commission: -1,
      swap: -0.5,
    });
    expect(result).toEqual({
      id: 7,
      side: "BUY",
      volumeLots: 0.5,
      entry: 2000,
      stopLoss: 1990,
      takeProfit: 2020,
      swap: -0.5,
    });
  });

  test("a pending order's stub position (volume/entryPrice at 0) still maps", () => {
    const result = CtraderPositionSchema.parse({
      positionId: 8,
      symbolId: 1,
      tradeSide: "SELL",
      volume: 0,
      entryPrice: 0,
    });
    expect(result.id).toBe(8);
    expect(result.side).toBe("SELL");
    expect(result.volumeLots).toBe(0);
    expect(result.entry).toBe(0);
    expect(result.stopLoss).toBeUndefined();
    expect(result.takeProfit).toBeUndefined();
  });

  test("missing/mistyped fields resolve to undefined instead of throwing", () => {
    const result = CtraderPositionSchema.parse({
      positionId: "not-a-number", // wrong type
      tradeSide: "HOLD", // not a valid TradeSide
      // volume/entryPrice entirely absent
    });
    expect(result.id).toBeUndefined();
    expect(result.side).toBeUndefined();
    expect(result.volumeLots).toBeUndefined();
    expect(result.entry).toBeUndefined();
  });

  test("an empty object is valid (permissive record) and maps to all-undefined", () => {
    const result = CtraderPositionSchema.parse({});
    expect(result).toEqual({
      id: undefined,
      side: undefined,
      volumeLots: undefined,
      entry: undefined,
      stopLoss: undefined,
      takeProfit: undefined,
      swap: undefined,
    });
  });
});
