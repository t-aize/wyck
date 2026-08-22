import { describe, expect, test } from "bun:test";
import { computeVolume, VOLUME_STEP, validateRiskPercent } from "./risk.ts";
import { runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

describe("computeVolume", () => {
  test("snaps to VOLUME_STEP and rounds half up", () => {
    // riskAmount=105, stopDistance=10 -> ounces=10.5 -> round(10.5)=11 -> 1100
    expect(runOk(computeVolume(105, 10))).toBe(1100);
  });

  test("computes a plain case with no rounding needed", () => {
    // riskAmount=100, stopDistance=11 -> ounces=9.0909... -> round=9 -> 900
    expect(runOk(computeVolume(100, 11))).toBe(900);
  });

  test("fails on a non-positive stop distance", () => {
    const error = runFail(computeVolume(100, 0));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Distance de stop invalide");

    const negative = runFail(computeVolume(100, -5));
    expect(negative.message).toContain("Distance de stop invalide");
  });

  test("fails when the computed volume is under the account minimum", () => {
    const error = runFail(computeVolume(0.01, 11));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("sous le minimum");
  });

  test("succeeds exactly at VOLUME_STEP", () => {
    // riskAmount=11, stopDistance=11 -> ounces=1 -> round((1*100)/100)=1 -> 100 (== VOLUME_STEP)
    expect(runOk(computeVolume(11, 11))).toBe(VOLUME_STEP);
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
