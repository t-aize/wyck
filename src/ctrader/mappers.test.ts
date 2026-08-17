import { describe, expect, test } from "bun:test";
import { isUnmapped, readPosition } from "./mappers.ts";
import type { CtraderPosition } from "./schemas.ts";

describe("readPosition", () => {
  test("lit les champs confirmés (positionId/tradeSide/volume/entryPrice, cf. get_positions réel)", () => {
    const position: CtraderPosition = {
      positionId: 12345,
      tradeSide: "BUY",
      volume: 1000, // 0.10 lot
      entryPrice: 4100.2,
      stopLoss: 4090,
      takeProfit: 4110,
      swap: -1.5,
    };

    const read = readPosition(position);

    expect(read.id).toBe(12345);
    expect(read.side).toBe("BUY");
    expect(read.volumeLots).toBeCloseTo(0.1);
    expect(read.entry).toBe(4100.2);
    expect(read.stopLoss).toBe(4090);
    expect(read.takeProfit).toBe(4110);
    expect(read.swap).toBe(-1.5);
    expect(isUnmapped(read)).toBe(false);
  });

  test("signale un mapping cassé quand les champs à haute confiance sont absents", () => {
    const read = readPosition({ unexpectedField: "???" });
    expect(read.id).toBeUndefined();
    expect(read.side).toBeUndefined();
    expect(isUnmapped(read)).toBe(true);
  });

  test("ignore un tradeSide qui ne serait ni BUY ni SELL", () => {
    const read = readPosition({
      positionId: 1,
      tradeSide: "UNKNOWN",
      volume: 100,
      entryPrice: 4100,
    });
    expect(read.side).toBeUndefined();
    expect(isUnmapped(read)).toBe(true);
  });
});
