import { describe, expect, test } from "bun:test";
import type { CtraderPosition } from "../../ctrader/schemas.ts";
import { isUnmapped } from "./PositionsPanel.tsx";

function position(overrides: Partial<CtraderPosition> = {}): CtraderPosition {
  return {
    id: 1,
    side: "BUY",
    volumeLots: 0.1,
    entry: 4100,
    stopLoss: 4090,
    takeProfit: 4110,
    swap: -1.5,
    ...overrides,
  };
}

describe("isUnmapped", () => {
  test("tous les champs à haute confiance résolus ⇒ false", () => {
    expect(isUnmapped(position())).toBe(false);
  });

  test("id absent ⇒ true", () => {
    expect(isUnmapped(position({ id: undefined }))).toBe(true);
  });

  test("side absent ⇒ true", () => {
    expect(isUnmapped(position({ side: undefined }))).toBe(true);
  });

  test("volumeLots absent ⇒ true", () => {
    expect(isUnmapped(position({ volumeLots: undefined }))).toBe(true);
  });

  test("entry absent ⇒ true", () => {
    expect(isUnmapped(position({ entry: undefined }))).toBe(true);
  });

  test("stopLoss/takeProfit/swap absents n'affectent pas le résultat (confiance basse)", () => {
    expect(
      isUnmapped(position({ stopLoss: undefined, takeProfit: undefined, swap: undefined })),
    ).toBe(false);
  });
});
