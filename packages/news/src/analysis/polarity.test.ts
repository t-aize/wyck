import { describe, expect, test } from "bun:test";
import { indicatorKind, macroImpulse } from "./polarity.ts";

describe("indicatorKind", () => {
  test("maps growth / inflation / labor titles", () => {
    expect(indicatorKind("Non-Farm Employment Change")).toBe("growth");
    expect(indicatorKind("Core PCE Price Index m/m")).toBe("inflation");
    expect(indicatorKind("Unemployment Claims")).toBe("labor_slack");
    expect(indicatorKind("Final GDP Price Index y/y")).toBeUndefined();
    expect(indicatorKind("Trade Balance")).toBeUndefined();
  });

  test("treats average hourly earnings as inflation, not growth", () => {
    expect(indicatorKind("Average Hourly Earnings m/m")).toBe("inflation");
  });

  test("maps rate decisions", () => {
    expect(indicatorKind("Main Refinancing Rate")).toBe("rates");
    expect(indicatorKind("Federal Funds Rate")).toBe("rates");
    expect(indicatorKind("Official Bank Rate")).toBe("rates");
    expect(indicatorKind("Interest Rate Decision")).toBe("rates");
  });
});

describe("macroImpulse", () => {
  test("growth/inflation/rates: higher forecast is hawkish", () => {
    expect(macroImpulse("growth", 255_000, 230_000)).toBe("hawkish");
    expect(macroImpulse("inflation", 0.3, 0.2)).toBe("hawkish");
    expect(macroImpulse("rates", 2.65, 2.4)).toBe("hawkish");
    expect(macroImpulse("rates", 2.4, 2.65)).toBe("dovish");
  });

  test("labor_slack inverts: higher unemployment is dovish", () => {
    expect(macroImpulse("labor_slack", 230_000, 215_000)).toBe("dovish");
  });

  test("equal readings are neutral", () => {
    expect(macroImpulse("growth", 200, 200)).toBe("neutral");
  });
});
