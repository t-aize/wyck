import { describe, expect, test } from "bun:test";
import { fetchTradeContext } from "./context.ts";
import { fakeCtraderClient, runFail, runOk } from "./testUtils.ts";
import { TradeValidationError } from "./types.ts";

describe("fetchTradeContext", () => {
  test("converts raw x10^5 prices to displayed prices", () => {
    const client = fakeCtraderClient({ prices: [{ bid: 200_000_000, ask: 200_100_000 }] });
    const result = runOk(fetchTradeContext(client, 1));
    expect(result.bid).toBe(2000);
    expect(result.ask).toBe(2001);
    expect(result.equity).toBe(1_000_000);
    expect(result.moneyDigits).toBe(2);
  });

  test("fails when the server returns no price for the symbol", () => {
    const client = fakeCtraderClient({ prices: [] });
    const error = runFail(fetchTradeContext(client, 1));
    expect(error).toBeInstanceOf(TradeValidationError);
    expect(error.message).toContain("Prix indisponible");
  });
});
