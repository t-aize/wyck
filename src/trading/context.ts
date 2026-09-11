import { Effect } from "effect";
import { PRICE_SCALE } from "../constants.ts";
import type { CtraderClient } from "../ctrader/client/CtraderClient.ts";
import type { CtraderMcpError } from "../ctrader/client/CtraderMcpError.ts";
import { TradeValidationError } from "./types.ts";

/** Fetch spot+balance concurrent — prix déjà convertis en prix affiché (÷ PRICE_SCALE), comme le
 * reste du domaine trading. */
export function fetchTradeContext(
  client: CtraderClient,
  symbolId: number,
): Effect.Effect<
  { bid: number; ask: number; equity: number; moneyDigits: number },
  TradeValidationError | CtraderMcpError
> {
  return Effect.gen(function* () {
    const [{ prices }, { equity, moneyDigits }] = yield* Effect.all(
      [client.getSpotPrices({ symbolId: [symbolId] }), client.getBalance()],
      { concurrency: "unbounded" },
    );

    const spot = prices[0];
    if (!spot) {
      return yield* Effect.fail(new TradeValidationError("Prix indisponible pour ce symbole"));
    }
    return { bid: spot.bid / PRICE_SCALE, ask: spot.ask / PRICE_SCALE, equity, moneyDigits };
  });
}
