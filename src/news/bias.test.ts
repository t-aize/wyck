import { describe, expect, test } from "bun:test";
import { goldBias } from "./bias.ts";

describe("goldBias", () => {
  test("direct polarity (NFP): forecast above previous reads Fed-hawkish -> bearish for gold", () => {
    expect(
      goldBias({ title: "Non-Farm Employment Change", forecast: "255K", previous: "230K" }),
    ).toBe("bearish");
  });

  test("direct polarity: forecast below previous -> bullish for gold", () => {
    expect(
      goldBias({ title: "Non-Farm Employment Change", forecast: "180K", previous: "230K" }),
    ).toBe("bullish");
  });

  test("inverse polarity (unemployment claims): forecast above previous -> bullish for gold", () => {
    expect(goldBias({ title: "Unemployment Claims", forecast: "230K", previous: "215K" })).toBe(
      "bullish",
    );
  });

  test("inverse polarity: forecast below previous -> bearish for gold", () => {
    expect(goldBias({ title: "Unemployment Claims", forecast: "200K", previous: "215K" })).toBe(
      "bearish",
    );
  });

  test("equal forecast/previous is neutral", () => {
    expect(
      goldBias({ title: "Non-Farm Employment Change", forecast: "200K", previous: "200K" }),
    ).toBe("neutral");
  });

  test("a title with no polarity rule returns undefined (no bet, not a guess)", () => {
    expect(
      goldBias({ title: "Trade Balance", forecast: "-65B", previous: "-63B" }),
    ).toBeUndefined();
  });

  test("direct polarity (inflation): Core PCE forecast above previous -> bearish for gold", () => {
    expect(
      goldBias({ title: "Core PCE Price Index m/m", forecast: "0.3%", previous: "0.2%" }),
    ).toBe("bearish");
  });

  test("direct polarity (inflation): CPI forecast below previous -> bullish for gold", () => {
    expect(goldBias({ title: "CPI y/y", forecast: "3.0%", previous: "3.1%" })).toBe("bullish");
  });

  test("direct polarity (inflation): PPI forecast equal previous -> neutral", () => {
    expect(goldBias({ title: "PPI m/m", forecast: "0.2%", previous: "0.2%" })).toBe("neutral");
  });

  test("unparsable forecast/previous returns undefined", () => {
    expect(
      goldBias({ title: "Non-Farm Employment Change", forecast: "", previous: "230K" }),
    ).toBeUndefined();
  });
});
