import { Effect } from "effect";
import { PRICE_SCALE } from "../constants.ts";
import type { CtraderClient, CtraderMcpError } from "../ctrader/client.ts";
import type { TradeSide } from "../ctrader/schemas.ts";
import { TradeValidationError } from "./types.ts";

export const ATR_PERIOD = 14;

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

const ATR_WINDOW_MS = 6 * 60 * 60_000; // 6h ≈ 72 bougies M5, largement plus que period+1 requis

/** ATR(14) sur M5, en prix affiché (÷ PRICE_SCALE, même convention que le reste de `trading/`). */
export function fetchAtr(
  client: CtraderClient,
  symbolId: number,
): Effect.Effect<number, TradeValidationError | CtraderMcpError> {
  return Effect.gen(function* () {
    const now = Date.now();
    // (fromTimestamp, toTimestamp) est la seule combinaison de get_trendbars fiable en pratique
    // (cf. ctrader/schemas.ts#GetTrendbarsParams) — `count` seul renvoie une erreur 400 côté serveur
    // malgré un schéma de requête valide. Fenêtre large pour absorber un marché calme/weekend tout
    // en gardant confortablement plus que les `ATR_PERIOD + 1` bougies nécessaires.
    const { trendbars } = yield* client.getTrendbars({
      symbolId,
      period: "M_5",
      fromTimestamp: String(now - ATR_WINDOW_MS),
      toTimestamp: String(now),
    });

    const bars = trendbars.map((bar) => ({
      high: bar.high / PRICE_SCALE,
      low: bar.low / PRICE_SCALE,
      close: bar.close / PRICE_SCALE,
    }));
    const atr = computeAtr(bars);
    if (atr === undefined) {
      return yield* Effect.fail(
        new TradeValidationError(
          `Pas assez de bougies M5 pour calculer l'ATR(${ATR_PERIOD}) ` +
            `(${bars.length} reçues, ${ATR_PERIOD + 1} requises)`,
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
): { stopLoss: number; takeProfit: number } {
  const stopLoss = side === "BUY" ? entryPrice - atr : entryPrice + atr;
  const takeProfit =
    side === "BUY" ? entryPrice + atr * rewardRiskRatio : entryPrice - atr * rewardRiskRatio;
  return { stopLoss, takeProfit };
}
