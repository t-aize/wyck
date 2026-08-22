import { describe, expect, test } from "bun:test";
import { atrLevels, computeAtr, fetchAtr } from "./atr.ts";
import { fakeCtraderClient, runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

describe("computeAtr", () => {
  test("returns undefined when there are not enough bars", () => {
    const bars = Array.from({ length: 14 }, () => ({ high: 10, low: 9, close: 9.5 }));
    expect(computeAtr(bars, 14)).toBeUndefined(); // needs period+1 = 15
  });

  test("averages True Range over the period, using only the most recent bars", () => {
    // Each bar's high-low range is 2 (dwarfs the close-to-close gaps below), so TR == 2 for every
    // bar except the first (which has no previous close to compare against and is dropped).
    const bars = Array.from({ length: 20 }, (_, i) => ({
      high: 101 + i,
      low: 99 + i,
      close: 100 + i,
    }));
    expect(computeAtr(bars, 14)).toBe(2);
  });

  test("True Range picks the largest of the three candle-vs-previous-close gaps", () => {
    const bars = [
      { high: 100, low: 99, close: 99.5 },
      // gap up: high-low=1, but high-prevClose=100.5-99.5=1 too -> TR=1
      { high: 100.5, low: 99.6, close: 100 },
      // big gap down open: low-high range small, but prevClose-low = 100-97 = 3 -> TR=3
      { high: 97.5, low: 97, close: 97.2 },
    ];
    expect(computeAtr(bars, 2)).toBeCloseTo((1 + 3) / 2, 10);
  });
});

describe("atrLevels", () => {
  test("BUY: SL below entry, TP above, at the RR-scaled distance", () => {
    expect(atrLevels(2000, "BUY", 10, 1.2)).toEqual({ stopLoss: 1990, takeProfit: 2012 });
  });

  test("SELL: SL above entry, TP below, at the RR-scaled distance", () => {
    expect(atrLevels(2000, "SELL", 10, 1.2)).toEqual({ stopLoss: 2010, takeProfit: 1988 });
  });
});

describe("fetchAtr", () => {
  test("computes ATR from displayed-price trendbars (raw x10^5 converted)", () => {
    // 15 bars, high-low range = 2 displayed ($200,000 raw), flat closes -> ATR = 2.
    const trendbars = Array.from({ length: 15 }, (_, i) => ({
      timestamp: i,
      open: 200_000_000,
      high: 200_100_000,
      low: 199_900_000,
      close: 200_000_000,
      volume: 1,
    }));
    const client = fakeCtraderClient({ trendbars });
    expect(runOk(fetchAtr(client, 1))).toBe(2);
  });

  test("fails when the server returns too few candles", () => {
    const client = fakeCtraderClient({ trendbars: [] });
    const error = runFail(fetchAtr(client, 1));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Pas assez de bougies");
  });
});
