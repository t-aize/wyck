import { describe, expect, test } from "bun:test";
import { CtraderPositionSchema } from "./schemas.ts";

describe("CtraderPositionSchema", () => {
  test("mappe les champs confirmés (positionId/tradeSide/volume/entryPrice, cf. get_positions réel)", () => {
    const position = CtraderPositionSchema.parse({
      positionId: 12345,
      tradeSide: "BUY",
      volume: 1000, // 0.10 lot
      entryPrice: 4100.2,
      stopLoss: 4090,
      takeProfit: 4110,
      swap: -1.5,
    });

    expect(position.id).toBe(12345);
    expect(position.side).toBe("BUY");
    expect(position.volumeLots).toBeCloseTo(0.1);
    expect(position.entry).toBe(4100.2);
    expect(position.stopLoss).toBe(4090);
    expect(position.takeProfit).toBe(4110);
    expect(position.swap).toBe(-1.5);
  });

  test("champs à haute confiance absents ⇒ undefined plutôt qu'un échec de parse", () => {
    const position = CtraderPositionSchema.parse({ unexpectedField: "???" });
    expect(position.id).toBeUndefined();
    expect(position.side).toBeUndefined();
    expect(position.volumeLots).toBeUndefined();
    expect(position.entry).toBeUndefined();
  });

  test("ignore un tradeSide qui ne serait ni BUY ni SELL", () => {
    const position = CtraderPositionSchema.parse({
      positionId: 1,
      tradeSide: "UNKNOWN",
      volume: 100,
      entryPrice: 4100,
    });
    expect(position.side).toBeUndefined();
  });
});
