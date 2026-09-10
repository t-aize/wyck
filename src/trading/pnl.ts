import type { TradeSide } from "@aurum/ctrader";

/**
 * P&L latent d'une position ouverte, calculé plutôt que lu : l'API cTrader n'expose
 * aucun champ de profit latent. Mark-to-market au bid pour un long, à l'ask pour un
 * short. `volume` est le volume API (1/100 d'unité de base) — P&L = Δprix × volume/100,
 * indépendant de la classe d'actif tant que le compte est dans la devise de cotation.
 */
export function computeUnrealizedPnl(
  side: TradeSide,
  volume: number,
  entryPrice: number,
  bid: number,
  ask: number,
): number {
  const markPrice = side === "BUY" ? bid : ask;
  const priceDiff = side === "BUY" ? markPrice - entryPrice : entryPrice - markPrice;
  return priceDiff * (volume / 100);
}

export function computeUnrealizedPnlOrUndefined(
  side: TradeSide | undefined,
  volume: number | undefined,
  entryPrice: number | undefined,
  bid: number | undefined,
  ask: number | undefined,
): number | undefined {
  if (
    side === undefined ||
    volume === undefined ||
    entryPrice === undefined ||
    bid === undefined ||
    ask === undefined
  ) {
    return undefined;
  }
  return computeUnrealizedPnl(side, volume, entryPrice, bid, ask);
}
