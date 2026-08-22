import { describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { PRICE_SCALE } from "../constants.ts";
import type { CtraderClient } from "../ctrader/client.ts";
import { CtraderMcpError } from "../ctrader/client.ts";
import { fakeCtraderClient, runFail, runOk } from "../trading/testUtils.ts";
import { fetchStructure } from "./fetch.ts";

// Same 11-bar uptrend fixture hand-verified in bias.test.ts/swings.test.ts (HH at index 6, HL at
// index 8, close breaks above the first swing high at index 6 -> ends bullish, resistance=18,
// support=12) — scaled to the raw x10^5 price convention `fetchStructure` reads trendbars in.
const DISPLAYED = [
  { h: 10, l: 8, c: 9 },
  { h: 12, l: 10, c: 11 },
  { h: 15, l: 13, c: 14 },
  { h: 13, l: 11, c: 12 },
  { h: 11, l: 9, c: 10 },
  { h: 13, l: 11, c: 12 },
  { h: 18, l: 16, c: 17 },
  { h: 16, l: 14, c: 15 },
  { h: 14, l: 12, c: 13 },
  { h: 16, l: 14, c: 15 },
  { h: 20, l: 18, c: 19 },
];

const trendbars = DISPLAYED.map((bar, i) => ({
  timestamp: i,
  open: bar.c * PRICE_SCALE,
  high: bar.h * PRICE_SCALE,
  low: bar.l * PRICE_SCALE,
  close: bar.c * PRICE_SCALE,
  volume: 1,
}));

describe("fetchStructure", () => {
  test("converts raw x10^5 trendbars and computes the same reading independently on all three timeframes", () => {
    const client = fakeCtraderClient({ trendbars });
    const result = runOk(fetchStructure(client, 1));
    const expected = { bias: "bullish" as const, resistance: 18, support: 12 };
    expect(result).toEqual({ M_5: expected, M_15: expected, H_1: expected });
  });

  test("not enough candles on a timeframe resolves to neutral, not an error", () => {
    const client = fakeCtraderClient({ trendbars: trendbars.slice(0, 2) });
    const result = runOk(fetchStructure(client, 1));
    const expected = { bias: "neutral" as const, resistance: undefined, support: undefined };
    expect(result).toEqual({ M_5: expected, M_15: expected, H_1: expected });
  });

  test("propagates a transport failure from getTrendbars", () => {
    const failingClient = {
      getTrendbars: () => Effect.fail(new CtraderMcpError("boom")),
    } as unknown as CtraderClient;
    const error = runFail(fetchStructure(failingClient, 1));
    expect(error.message).toBe("boom");
  });
});
