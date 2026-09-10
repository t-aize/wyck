import { describe, expect, test } from "bun:test";
import { newsProfile } from "../profile/profile.ts";
import { instrumentBias } from "./bias.ts";

const gold = newsProfile({ symbolName: "XAUUSD" });
const eurusd = newsProfile({ symbolName: "EURUSD" });
const us100 = newsProfile({ symbolName: "US100" });
const btc = newsProfile({ symbolName: "BTCUSD" });

describe("instrumentBias — metal (XAUUSD)", () => {
  test("direct polarity (NFP): forecast above previous reads Fed-hawkish -> bearish for gold", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "255K", previous: "230K" },
        gold,
      ),
    ).toBe("bearish");
  });

  test("direct polarity: forecast below previous -> bullish for gold", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "180K", previous: "230K" },
        gold,
      ),
    ).toBe("bullish");
  });

  test("inverse polarity (unemployment claims): forecast above previous -> bullish for gold", () => {
    expect(
      instrumentBias(
        { title: "Unemployment Claims", country: "USD", forecast: "230K", previous: "215K" },
        gold,
      ),
    ).toBe("bullish");
  });

  test("inverse polarity: forecast below previous -> bearish for gold", () => {
    expect(
      instrumentBias(
        { title: "Unemployment Claims", country: "USD", forecast: "200K", previous: "215K" },
        gold,
      ),
    ).toBe("bearish");
  });

  test("equal forecast/previous is neutral", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "200K", previous: "200K" },
        gold,
      ),
    ).toBe("neutral");
  });

  test("a title with no polarity rule returns undefined (no bet, not a guess)", () => {
    expect(
      instrumentBias(
        { title: "Trade Balance", country: "USD", forecast: "-65B", previous: "-63B" },
        gold,
      ),
    ).toBeUndefined();
  });

  test("inflation: Core PCE forecast above previous -> bearish for gold", () => {
    expect(
      instrumentBias(
        { title: "Core PCE Price Index m/m", country: "USD", forecast: "0.3%", previous: "0.2%" },
        gold,
      ),
    ).toBe("bearish");
  });

  test("unparsable forecast/previous returns undefined", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "", previous: "230K" },
        gold,
      ),
    ).toBeUndefined();
  });
});

describe("instrumentBias — forex (EURUSD)", () => {
  test("hawkish USD (quote) is bearish for EURUSD", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "255K", previous: "230K" },
        eurusd,
      ),
    ).toBe("bearish");
  });

  test("hawkish EUR (base) is bullish for EURUSD", () => {
    expect(
      instrumentBias(
        { title: "CPI y/y", country: "EUR", forecast: "2.6%", previous: "2.4%" },
        eurusd,
      ),
    ).toBe("bullish");
  });

  test("dovish USD (unemployment up) is bullish for EURUSD", () => {
    expect(
      instrumentBias(
        { title: "Unemployment Claims", country: "USD", forecast: "230K", previous: "215K" },
        eurusd,
      ),
    ).toBe("bullish");
  });

  test("a JPY print does not bias EURUSD", () => {
    expect(
      instrumentBias(
        { title: "CPI y/y", country: "JPY", forecast: "2.6%", previous: "2.4%" },
        eurusd,
      ),
    ).toBeUndefined();
  });
});

describe("instrumentBias — index (US100)", () => {
  test("strong NFP is bullish for US equities (risk-on), unlike gold", () => {
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "255K", previous: "230K" },
        us100,
      ),
    ).toBe("bullish");
  });

  test("higher CPI is bearish for US equities (discount rates)", () => {
    expect(
      instrumentBias(
        { title: "CPI y/y", country: "USD", forecast: "3.2%", previous: "3.0%" },
        us100,
      ),
    ).toBe("bearish");
  });

  test("higher unemployment claims are bearish for US equities", () => {
    expect(
      instrumentBias(
        { title: "Unemployment Claims", country: "USD", forecast: "230K", previous: "215K" },
        us100,
      ),
    ).toBe("bearish");
  });
});

describe("instrumentBias — crypto (BTCUSD)", () => {
  test("treats BTC like a risk asset: hawkish inflation is bearish, strong growth is bullish", () => {
    expect(
      instrumentBias(
        { title: "Core PCE Price Index m/m", country: "USD", forecast: "0.3%", previous: "0.2%" },
        btc,
      ),
    ).toBe("bearish");
    expect(
      instrumentBias(
        { title: "Non-Farm Employment Change", country: "USD", forecast: "255K", previous: "230K" },
        btc,
      ),
    ).toBe("bullish");
  });
});
