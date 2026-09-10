/**
 * Type d'ordre que l'on **envoie** à `create_order`.
 *
 * Distinct de {@link HistoricalOrderType} : le book / l'historique y ajoutent
 * `STOP_LOSS_TAKE_PROFIT` (SL/TP auto d'une position), que l'on ne crée jamais
 * soi-même.
 */
export enum OrderType {
  MARKET = "MARKET",
  LIMIT = "LIMIT",
  STOP = "STOP",
  MARKET_RANGE = "MARKET_RANGE",
  STOP_LIMIT = "STOP_LIMIT",
}
