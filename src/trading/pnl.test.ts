import { describe, expect, test } from "bun:test";
import { computeUnrealizedPnl, computeUnrealizedPnlOrUndefined } from "./pnl.ts";

describe("computeUnrealizedPnl", () => {
  test("BUY marks to bid", () => {
    expect(computeUnrealizedPnl("BUY", 1, 2000, 2010, 2011)).toBe(1000);
  });

  test("SELL marks to ask", () => {
    expect(computeUnrealizedPnl("SELL", 1, 2000, 1990, 1991)).toBe(900);
  });

  test("scales with volumeLots", () => {
    expect(computeUnrealizedPnl("BUY", 2.5, 2000, 2010, 2011)).toBe(2500);
  });

  test("a loss comes back negative", () => {
    expect(computeUnrealizedPnl("BUY", 1, 2000, 1990, 1991)).toBe(-1000);
  });
});

describe("computeUnrealizedPnlOrUndefined", () => {
  test("returns undefined if any input is missing", () => {
    expect(computeUnrealizedPnlOrUndefined(undefined, 1, 2000, 2010, 2011)).toBeUndefined();
    expect(computeUnrealizedPnlOrUndefined("BUY", undefined, 2000, 2010, 2011)).toBeUndefined();
    expect(computeUnrealizedPnlOrUndefined("BUY", 1, undefined, 2010, 2011)).toBeUndefined();
    expect(computeUnrealizedPnlOrUndefined("BUY", 1, 2000, undefined, 2011)).toBeUndefined();
    expect(computeUnrealizedPnlOrUndefined("BUY", 1, 2000, 2010, undefined)).toBeUndefined();
  });

  test("delegates to computeUnrealizedPnl once everything is defined", () => {
    expect(computeUnrealizedPnlOrUndefined("BUY", 1, 2000, 2010, 2011)).toBe(1000);
  });
});
