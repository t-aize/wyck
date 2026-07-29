/**
 * Calcul d'un trade à partir de trois entrées (entrée, SL, TP) + risque : direction
 * déduite du SL/TP, type d'ordre déduit du prix d'entrée vs marché, taille de
 * position dérivée du risque en % d'équity.
 *
 * Convention prix : l'API cTrader renvoie des prix bruts à l'échelle x10^5 sur les
 * endpoints de lecture (spot, trendbars) mais attend des prix "affichés" (divisés
 * par 10^5) sur les endpoints d'écriture (create_order). Tout dans ce fichier
 * travaille en prix affiché — la conversion x10^5 se fait une seule fois, à la
 * lecture des données brutes.
 */

import { LOT_VOLUME, PRICE_SCALE } from "../constants.ts";
import type { CreateOrderParams, CtraderClient, OrderType, TradeSide } from "../ctrader/client.ts";

export interface TradeInput {
  /** "market" = prix courant (ordre MARKET) ; un nombre = prix affiché (LIMIT/STOP déduit) */
  entry: number | "market";
  /** % de l'équity du compte */
  riskPercent: number;
  stopLoss: number;
  takeProfit: number;
}

export interface PreparedTrade {
  orderType: OrderType;
  tradeSide: TradeSide;
  entryPrice: number;
  stopLoss: number;
  takeProfit: number;
  /** Volume au format API (1/100 d'once pour XAUUSD) */
  volume: number;
  /** Volume en lots (1 lot = 100 onces), pour l'affichage */
  volumeLots: number;
  riskAmount: number;
  riskPercent: number;
  /** Gain potentiel si le TP est atteint (même formule que riskAmount, distance TP) */
  rewardAmount: number;
}

// Pas/minimum de volume imposés par ce compte sur XAUUSD : 0.01 lot (confirmé via la
// plateforme du broker — pas de dropdown 0.01→1.00 lot par incréments de 0.01).
const VOLUME_STEP = 100; // 0.01 lot

function computeVolume(riskAmount: number, stopDistance: number): number {
  if (stopDistance <= 0) throw new Error("Distance de stop invalide (SL identique à l'entrée ?)");
  const ounces = riskAmount / stopDistance;
  const volume = Math.round((ounces * 100) / VOLUME_STEP) * VOLUME_STEP;
  if (volume < VOLUME_STEP) {
    throw new Error(
      `Volume calculé (${(volume / LOT_VOLUME).toFixed(4)} lot) sous le minimum de ce compte ` +
        "(0.01 lot) — augmente le risque% ou resserre le stop",
    );
  }
  return volume;
}

function toPoints(priceDistance: number): number {
  return Math.round(priceDistance * PRICE_SCALE);
}

/** LIMIT/STOP déduit de la position de l'entrée par rapport au prix de référence (ask pour BUY, bid pour SELL). */
function inferOrderType(side: TradeSide, entryPrice: number, referencePrice: number): OrderType {
  if (entryPrice === referencePrice) return "MARKET";
  if (side === "BUY") return entryPrice > referencePrice ? "STOP" : "LIMIT";
  return entryPrice < referencePrice ? "STOP" : "LIMIT";
}

export async function prepareTrade(
  client: CtraderClient,
  symbolId: number,
  input: TradeInput,
): Promise<PreparedTrade> {
  if (!Number.isFinite(input.riskPercent) || input.riskPercent <= 0 || input.riskPercent > 100) {
    throw new Error("Risque invalide : doit être un pourcentage entre 0 et 100");
  }

  const [{ prices }, { equity, moneyDigits }] = await Promise.all([
    client.getSpotPrices({ symbolId: [symbolId] }),
    client.getBalance(),
  ]);
  const spot = prices[0];
  if (!spot) throw new Error("Prix indisponible pour ce symbole");
  const bid = spot.bid / PRICE_SCALE;
  const ask = spot.ask / PRICE_SCALE;

  const { stopLoss, takeProfit } = input;
  if (stopLoss === takeProfit) throw new Error("SL et TP ne peuvent pas être identiques");
  // direction déduite du SL/TP : BUY si le SL est sous le TP, SELL sinon.
  const side: TradeSide = stopLoss < takeProfit ? "BUY" : "SELL";
  const reference = side === "BUY" ? ask : bid;

  const entryPrice = input.entry === "market" ? reference : input.entry;
  const orderType: OrderType =
    input.entry === "market" ? "MARKET" : inferOrderType(side, entryPrice, reference);

  if (side === "BUY" && !(stopLoss < entryPrice && takeProfit > entryPrice)) {
    throw new Error("Incohérent pour un achat : le SL doit être sous l'entrée et le TP au-dessus");
  }
  if (side === "SELL" && !(stopLoss > entryPrice && takeProfit < entryPrice)) {
    throw new Error(
      "Incohérent pour une vente : le SL doit être au-dessus de l'entrée et le TP en dessous",
    );
  }

  const stopDistance = Math.abs(entryPrice - stopLoss);
  const targetDistance = Math.abs(entryPrice - takeProfit);
  const riskAmount = (equity / 10 ** moneyDigits) * (input.riskPercent / 100);
  const volume = computeVolume(riskAmount, stopDistance);

  return {
    orderType,
    tradeSide: side,
    entryPrice,
    stopLoss,
    takeProfit,
    volume,
    volumeLots: volume / LOT_VOLUME,
    riskAmount,
    riskPercent: input.riskPercent,
    rewardAmount: (volume / 100) * targetDistance,
  };
}

/**
 * P&L latent d'une position ouverte, calculé plutôt que lu : l'API cTrader (Open API
 * `ProtoOAPosition`, que ce MCP reflète — cf. commentaire en tête de ctrader/mappers.ts)
 * n'expose aucun champ de profit latent, seulement des données réalisées (swap,
 * commission). Mark-to-market au bid pour un long (prix de sortie si on clôturait
 * maintenant), à l'ask pour un short — convention standard, cohérente avec le reste du
 * fichier qui déduit toujours le prix de référence du côté de la position.
 */
export function computeUnrealizedPnl(
  side: TradeSide,
  volumeLots: number,
  entryPrice: number,
  bid: number,
  ask: number,
): number {
  const markPrice = side === "BUY" ? bid : ask;
  const priceDiff = side === "BUY" ? markPrice - entryPrice : entryPrice - markPrice;
  return priceDiff * volumeLots * 100;
}

export function toCreateOrderParams(symbolId: number, trade: PreparedTrade): CreateOrderParams {
  const base = {
    symbolId,
    orderType: trade.orderType,
    tradeSide: trade.tradeSide,
    volume: trade.volume,
    label: "aurum",
  };

  if (trade.orderType === "MARKET") {
    return {
      ...base,
      relativeStopLoss: toPoints(Math.abs(trade.entryPrice - trade.stopLoss)),
      relativeTakeProfit: toPoints(Math.abs(trade.entryPrice - trade.takeProfit)),
    };
  }

  return {
    ...base,
    limitPrice: trade.orderType === "LIMIT" ? trade.entryPrice : undefined,
    stopPrice: trade.orderType === "STOP" ? trade.entryPrice : undefined,
    stopLoss: trade.stopLoss,
    takeProfit: trade.takeProfit,
  };
}
