import { Effect } from "effect";
import { PRICE_SCALE, TRENDBAR_PERIOD_MS, type TrendbarPeriod } from "../constants.ts";
import type { CtraderClient, CtraderMcpError } from "../ctrader/client.ts";
import type { TradeSide } from "../ctrader/schemas.ts";
import { roundPrice } from "../utils/priceMath.ts";
import { TradeValidationError } from "./types.ts";

/** Défauts appliqués tant que l'utilisateur n'a rien réglé via `settings atrperiod`/`settings
 * atrtimeframe` (cf. commands/settings.ts, settings.ts#EMPTY_APP_CONFIG) — inchangés par rapport au
 * comportement d'avant ces réglages. */
export const ATR_PERIOD = 14;
export const ATR_TIMEFRAME: TrendbarPeriod = "M_5";

function trueRange(high: number, low: number, prevClose: number): number {
  return Math.max(high - low, Math.abs(high - prevClose), Math.abs(low - prevClose));
}

/**
 * Moyenne simple des `period` dernières True Range — pas le lissage exponentiel de Wilder que la
 * plupart des plateformes utilisent par défaut pour leur indicateur ATR. Avec seulement
 * `period + 1` bougies récupérées (cf. `fetchAtr`, pas des centaines pour laisser un lissage
 * converger), une moyenne de Wilder donnerait de toute façon exactement le même résultat que cette
 * moyenne simple (le "seed" de Wilder EST le calcul entier faute de bougies supplémentaires à
 * lisser derrière) — autant assumer la moyenne simple plutôt que présenter une formule qui ne lisse
 * rien en pratique ici.
 */
export function computeAtr(
  bars: { high: number; low: number; close: number }[],
  period: number = ATR_PERIOD,
): number | undefined {
  if (bars.length < period + 1) return undefined;
  const relevant = bars.slice(-(period + 1));
  let sum = 0;
  for (let i = 1; i < relevant.length; i++) {
    sum += trueRange(relevant[i]!.high, relevant[i]!.low, relevant[i - 1]!.close);
  }
  return sum / period;
}

// Ratio conservé de l'ancien 6h/M5 = 72 bougies pour period+1 = 15 requises (~×4.8) — appliqué en
// proportion de `period` plutôt qu'en durée fixe, pour rester généreux sur tout timeframe/period au
// lieu de sur-fetcher massivement sur W_1/MN_1 ou sous-fetcher sur un `period` élevé en M_1.
const ATR_WINDOW_CANDLES_FACTOR = 5;

/** ATR(`period`) sur `timeframe`, en prix affiché (÷ PRICE_SCALE, même convention que le reste de
 * `trading/`). Défauts = comportement historique (M5, période 14) tant qu'aucun réglage n'est
 * fourni — cf. `settings atrperiod`/`settings atrtimeframe` (commands/settings.ts), qui alimentent
 * ces options depuis `prepareAtr.ts`/`useAtrAutoRefresh.ts`. */
export function fetchAtr(
  client: CtraderClient,
  symbolId: number,
  options?: { timeframe?: TrendbarPeriod; period?: number },
): Effect.Effect<number, TradeValidationError | CtraderMcpError> {
  const timeframe = options?.timeframe ?? ATR_TIMEFRAME;
  const period = options?.period ?? ATR_PERIOD;
  return Effect.gen(function* () {
    const now = Date.now();
    const windowMs = TRENDBAR_PERIOD_MS[timeframe] * (period + 1) * ATR_WINDOW_CANDLES_FACTOR;
    // (fromTimestamp, toTimestamp) est la seule combinaison de get_trendbars fiable en pratique
    // (cf. ctrader/schemas.ts#GetTrendbarsParams) — `count` seul renvoie une erreur 400 côté serveur
    // malgré un schéma de requête valide. Fenêtre large pour absorber un marché calme/weekend tout
    // en gardant confortablement plus que les `period + 1` bougies nécessaires.
    const { trendbars } = yield* client.getTrendbars({
      symbolId,
      period: timeframe,
      fromTimestamp: String(now - windowMs),
      toTimestamp: String(now),
    });

    const bars = trendbars.map((bar) => ({
      high: bar.high / PRICE_SCALE,
      low: bar.low / PRICE_SCALE,
      close: bar.close / PRICE_SCALE,
    }));
    const atr = computeAtr(bars, period);
    if (atr === undefined) {
      return yield* Effect.fail(
        new TradeValidationError(
          `Pas assez de bougies ${timeframe} pour calculer l'ATR(${period}) ` +
            `(${bars.length} reçues, ${period + 1} requises)`,
        ),
      );
    }
    return atr;
  });
}

/** SL/TP dérivés une seule fois, à partir du prix d'entrée déjà résolu — pas de re-tracking après
 * coup (contrairement à l'ancien atrTracking.ts, supprimé en 76452d9) : une fois la proposition
 * acceptée, le SL/TP sont figés comme n'importe quel trade préparé manuellement. */
export function atrLevels(
  entryPrice: number,
  side: TradeSide,
  atr: number,
  rewardRiskRatio: number,
  digits = 2,
): { stopLoss: number; takeProfit: number } {
  const stopLoss = side === "BUY" ? entryPrice - atr : entryPrice + atr;
  const takeProfit =
    side === "BUY" ? entryPrice + atr * rewardRiskRatio : entryPrice - atr * rewardRiskRatio;
  return { stopLoss: roundPrice(stopLoss, digits), takeProfit: roundPrice(takeProfit, digits) };
}
