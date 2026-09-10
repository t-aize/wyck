import type { OrderType } from "../protocol/OrderType.ts";
import type { TimeInForce } from "../protocol/TimeInForce.ts";
import type { TradeSide } from "../protocol/TradeSide.ts";
import type { OrderPriceFields } from "./OrderPriceFields.ts";

/** Params sortants de `create_order`. */
export interface CreateOrderParams extends OrderPriceFields {
  symbolId: number;
  orderType: OrderType;
  tradeSide: TradeSide;
  /**
   * Volume en 1/100 d'unité d'actif de base (`volume = lots × lotSize × 100`).
   * `lotSize` dépend de la classe : forex = 100 000, métaux = 100 (XAU : 1 lot =
   * 10 000), indices / crypto = 1. Ne pas réutiliser la valeur forex ailleurs.
   */
  volume: number;
  comment?: string;
  label?: string;
  timeInForce?: TimeInForce;
  /** Prix de référence du slippage (MARKET_RANGE). */
  baseSlippagePrice?: number;
  slippageInPoints?: number;
}
