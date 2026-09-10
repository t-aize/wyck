import type { CtraderClient, CtraderMcpError, TradeSide, TrendbarPeriod } from "@aurum/ctrader";
import { Effect } from "effect";
import { toLots } from "../utils/priceMath.ts";
import { atrLevels, fetchAtr } from "./atr.ts";
import { fetchTradeContext } from "./context.ts";
import { resolveEntry } from "./entry.ts";
import { computeVolume, validateRiskPercent } from "./risk.ts";
import type { PreparedTrade } from "./types.ts";
import { TradeValidationError } from "./types.ts";

export interface AtrTradeInput {
  /** Explicite plutôt que déduit du SL/TP (cf. `prepare.ts#prepareTrade`) : ici le SL/TP sont eux-
   * mêmes dérivés de l'ATR une fois la direction connue, pas l'inverse — rien à déduire depuis. */
  tradeSide: TradeSide;
  entry: number | "market";
  riskPercent: number;
  /** Reward:Risk — TP = distance ATR × ce ratio. */
  rewardRiskRatio: number;
  /** Réglages ATR courants (cf. settings.ts#AppConfig, `settings atrperiod`/`settings atrtimeframe`)
   * — absents = défauts historiques de `fetchAtr` (ATR_PERIOD/ATR_TIMEFRAME), pour ne pas forcer
   * chaque appelant/test à les fournir explicitement. */
  atrPeriod?: number;
  atrTimeframe?: TrendbarPeriod;
}

/**
 * Variante de `prepare.ts#prepareTrade` pour le mode ATR : le prix d'entrée n'est résolu qu'une
 * seule fois ici (via `fetchTradeContext` + `resolveEntry`), et le SL/TP sont calculés à partir de
 * CE prix — pas d'un second fetch de prix indépendant. Un `entry: "market"` résolu deux fois avec
 * des prix légèrement différents casserait silencieusement `inferOrderType` (l'égalité stricte
 * `entryPrice === referencePrice` qui décide MARKET vs LIMIT/STOP) dès que le marché tique entre les
 * deux lectures — c'est pour éviter précisément ce piège que ce fichier existe plutôt que de faire
 * un fetch ATR à part puis rappeler `prepareTrade` avec un prix déjà figé.
 */
export function prepareAtrTrade(
  client: CtraderClient,
  symbolId: number,
  input: AtrTradeInput,
  specs: { lotSize: number; digits: number } = { lotSize: 100, digits: 2 },
): Effect.Effect<PreparedTrade, TradeValidationError | CtraderMcpError> {
  return Effect.gen(function* () {
    yield* validateRiskPercent(input.riskPercent);
    if (!Number.isFinite(input.rewardRiskRatio) || input.rewardRiskRatio <= 0) {
      return yield* Effect.fail(
        new TradeValidationError("Ratio reward:risk invalide : doit être un nombre positif"),
      );
    }

    const [{ bid, ask, equity, moneyDigits }, atr] = yield* Effect.all(
      [
        fetchTradeContext(client, symbolId),
        fetchAtr(client, symbolId, {
          period: input.atrPeriod,
          timeframe: input.atrTimeframe,
        }),
      ],
      { concurrency: "unbounded" },
    );

    const { tradeSide: side } = input;
    const reference = side === "BUY" ? ask : bid;
    const { entryPrice, orderType } = resolveEntry(side, input.entry, reference);

    const { stopLoss, takeProfit } = atrLevels(
      entryPrice,
      side,
      atr,
      input.rewardRiskRatio,
      specs.digits,
    );

    const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
    const volume = yield* computeVolume(riskAmount, atr, specs.lotSize);

    const trade: PreparedTrade = {
      orderType,
      tradeSide: side,
      entryPrice,
      stopLoss,
      takeProfit,
      volume,
      volumeLots: toLots(volume, specs.lotSize),
      riskAmount,
      riskPercent: input.riskPercent,
      rewardAmount: (volume / 100) * (atr * input.rewardRiskRatio),
      atrRewardRiskRatio: input.rewardRiskRatio,
    };
    return trade;
  });
}
