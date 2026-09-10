import type { CtraderClient, CtraderMcpError, TradeSide } from "@aurum/ctrader";
import { Effect } from "effect";
import { toLots } from "../utils/priceMath.ts";
import { fetchTradeContext } from "./context.ts";
import { resolveEntry } from "./entry.ts";
import { computeVolume, validateRiskPercent } from "./risk.ts";
import type { PreparedTrade, TradeInput } from "./types.ts";
import { TradeValidationError } from "./types.ts";

/**
 * `client` reçu en paramètre explicite, comme partout ailleurs dans l'app (cf. commentaire de tête
 * de `CtraderClient`) — pas de DI Effect ici. Chaque règle de validation échoue via
 * `Effect.fail(new TradeValidationError(...))` plutôt qu'un `throw` générique, pour rester dans le
 * canal d'erreur typé d'Effect.
 */
export function prepareTrade(
  client: CtraderClient,
  symbolId: number,
  input: TradeInput,
  lotSize = 100,
): Effect.Effect<PreparedTrade, TradeValidationError | CtraderMcpError> {
  return Effect.gen(function* () {
    yield* validateRiskPercent(input.riskPercent);
    const { bid, ask, equity, moneyDigits } = yield* fetchTradeContext(client, symbolId);

    const { stopLoss, takeProfit } = input;
    if (stopLoss === takeProfit) {
      return yield* Effect.fail(
        new TradeValidationError("SL et TP ne peuvent pas être identiques"),
      );
    }
    // direction déduite du SL/TP : BUY si le SL est sous le TP, SELL sinon.
    const side: TradeSide = stopLoss < takeProfit ? "BUY" : "SELL";
    const reference = side === "BUY" ? ask : bid;
    const { entryPrice, orderType } = resolveEntry(side, input.entry, reference);

    if (side === "BUY" && !(stopLoss < entryPrice && takeProfit > entryPrice)) {
      return yield* Effect.fail(
        new TradeValidationError(
          "Incohérent pour un achat : le SL doit être sous l'entrée et le TP au-dessus",
        ),
      );
    }
    if (side === "SELL" && !(stopLoss > entryPrice && takeProfit < entryPrice)) {
      return yield* Effect.fail(
        new TradeValidationError(
          "Incohérent pour une vente : le SL doit être au-dessus de l'entrée et le TP en dessous",
        ),
      );
    }

    const stopDistance = Math.abs(entryPrice - stopLoss);
    const targetDistance = Math.abs(entryPrice - takeProfit);
    const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
    const volume = yield* computeVolume(riskAmount, stopDistance, lotSize);

    const trade: PreparedTrade = {
      orderType,
      tradeSide: side,
      entryPrice,
      stopLoss,
      takeProfit,
      volume,
      volumeLots: toLots(volume, lotSize),
      riskAmount,
      riskPercent: input.riskPercent,
      rewardAmount: (volume / 100) * targetDistance,
    };
    return trade;
  });
}
