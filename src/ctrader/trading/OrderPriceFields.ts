/**
 * Champs de prix / protection partagés par `create_order` et `amend_order`
 * (même nom, même sens). Seule leur *contrainte* diffère selon le contexte.
 *
 * cTrader n'a **pas** d'amend partiel : tout champ non renvoyé sur `amend_order`
 * est effacé côté serveur (constaté sur limitPrice / stopPrice / SL / TP).
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
