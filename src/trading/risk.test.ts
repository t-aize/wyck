import { describe, expect, test } from "bun:test";
import { computeVolume, validateRiskPercent } from "./risk.ts";
import { runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

const GOLD_LOT = 100; // XAUUSD : 0.01 lot = 100 volume API

describe("computeVolume", () => {
  test("snaps to 0.01-lot step and rounds half up", () => {
    // riskAmount=105, stopDistance=10 -> units=10.5 -> round(10.5)=11 -> 1100
    expect(runOk(computeVolume(105, 10, GOLD_LOT))).toBe(1100);
  });

  test("computes a plain case with no rounding needed", () => {
    // riskAmount=100, stopDistance=11 -> units=9.0909... -> round=9 -> 900
    expect(runOk(computeVolume(100, 11, GOLD_LOT))).toBe(900);
  });

  test("fails on a non-positive stop distance", () => {
    const error = runFail(computeVolume(100, 0, GOLD_LOT));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Distance de stop invalide");

    const negative = runFail(computeVolume(100, -5, GOLD_LOT));
    expect(negative.message).toContain("Distance de stop invalide");
  });

  test("fails when the computed volume is under the account minimum", () => {
    const error = runFail(computeVolume(0.01, 11, GOLD_LOT));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("sous le minimum");
  });

  test("succeeds exactly at the 0.01-lot step", () => {
    // riskAmount=11, stopDistance=11 -> units=1 -> 100 volume
    expect(runOk(computeVolume(11, 11, GOLD_LOT))).toBe(100);
  });
});

describe("validateRiskPercent", () => {
  test.each([0, -5, 101, Number.NaN, Number.POSITIVE_INFINITY])(
    "rejects %p as invalid",
    (riskPercent) => {
      const error = runFail(validateRiskPercent(riskPercent));
      expect(error).toBeInstanceOf(TradeValidationError);
      expect(error.message).toContain("Risque invalide");
    },
  );

  test.each([0.01, 1, 50, 100])("accepts %p as valid", (riskPercent) => {
    expect(runOk(validateRiskPercent(riskPercent))).toBeUndefined();
  });
});
