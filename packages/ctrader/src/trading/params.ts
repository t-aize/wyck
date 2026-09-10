/**
 * Params **sortants** des 5 outils d'écriture.
 *
 * Interfaces TS simples, jamais validées à l'exécution — construites par l'app
 * (ou le client) à partir de valeurs déjà typées par `tsc`, jamais reçues du
 * réseau. Rien à valider à cette frontière.
 *
 * cTrader n'a **pas** d'amend partiel : tout champ non renvoyé sur `amend_order`
 * est effacé côté serveur (constaté sur limitPrice / stopPrice / SL / TP).
 * L'app reconstruit donc toujours le payload complet depuis l'ordre existant.
 */

import type { OrderType, TimeInForce, TradeSide } from "../protocol/enums.ts";

/**
 * Champs de prix / protection partagés par `create_order` et `amend_order`
 * (même nom, même sens). Seule leur *contrainte* diffère selon le contexte.
 */
export interface OrderPriceFields {
  /** Requis pour LIMIT, STOP_LIMIT à la création. */
  limitPrice?: number;
  /** Requis pour STOP, STOP_LIMIT à la création. */
  stopPrice?: number;
  /**
   * Prix absolu. Supporté sur LIMIT / STOP / STOP_LIMIT, **pas** sur
   * MARKET / MARKET_RANGE en création. Exclusif avec {@link OrderPriceFields.relativeStopLoss}.
   */
  stopLoss?: number;
  /** Comme `stopLoss`, pour le take-profit. Exclusif avec {@link OrderPriceFields.relativeTakeProfit}. */
  takeProfit?: number;
  /**
   * Distance en points depuis le prix d'exécution ; requis pour MARKET /
   * MARKET_RANGE en création. Exclusif avec {@link OrderPriceFields.stopLoss}.
   */
  relativeStopLoss?: number;
  /** Comme `relativeStopLoss`, pour le take-profit. Exclusif avec {@link OrderPriceFields.takeProfit}. */
  relativeTakeProfit?: number;
  /** Epoch ms (entier uniquement, pas d'ISO-8601 ici). */
  expirationTimestamp?: number;
}

/** Params de `create_order`. */
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

/** Params de `amend_order` — payload **complet**, pas un patch. */
export interface AmendOrderParams extends OrderPriceFields {
  orderId: number;
  volume?: number;
}

/** Params de `cancel_order`. */
export interface CancelOrderParams {
  orderId: number;
}

/** Params de `amend_position` (SL / TP d'une position déjà ouverte). */
export interface AmendPositionParams {
  positionId: number;
  /** Nouveau SL en prix affiché (pas en pipettes) ; omis = inchangé. */
  stopLoss?: number;
  /** Nouveau TP en prix affiché ; omis = inchangé. */
  takeProfit?: number;
  trailingStopLoss?: boolean;
}

/** Params de `close_position`. */
export interface ClosePositionParams {
  positionId: number;
  /**
   * Volume à clôturer en 1/100 d'unité d'actif de base
   * (`volume = lots × lotSize × 100`).
   */
  volume: number;
}
