import { OrderType } from "../ctrader/protocol/OrderType.ts";
import { TradeSide } from "../ctrader/protocol/TradeSide.ts";
import type { PreparedTrade } from "./types.ts";

/** LIMIT/STOP déduit de la position de l'entrée par rapport au prix de référence (ask pour BUY, bid pour SELL). */
export function inferOrderType(
  side: TradeSide,
  entryPrice: number,
  referencePrice: number,
): OrderType {
  if (entryPrice === referencePrice) return OrderType.MARKET;
  if (side === TradeSide.BUY) return entryPrice > referencePrice ? OrderType.STOP : OrderType.LIMIT;
  return entryPrice < referencePrice ? OrderType.STOP : OrderType.LIMIT;
}

/** Résout le prix d'entrée effectif et le type d'ordre à partir de la direction (déduite du SL/TP,
 * cf. `prepare.ts#prepareTrade`). */
export function resolveEntry(
  side: TradeSide,
  entry: number | "market",
  reference: number,
): Pick<PreparedTrade, "entryPrice" | "orderType"> {
  const entryPrice = entry === "market" ? reference : entry;
  const orderType: OrderType =
    entry === "market" ? OrderType.MARKET : inferOrderType(side, entryPrice, reference);
  return { entryPrice, orderType };
}
