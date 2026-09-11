/**
 * Type d'ordre tel que le **book / l'historique** le renvoie.
 *
 * Reprend {@link OrderType} et y ajoute {@link HistoricalOrderType.STOP_LOSS_TAKE_PROFIT} :
 * les protections SL/TP que le serveur attache tout seul à une position. On ne
 * l'envoie jamais dans {@link CreateOrderParams}.
 */
export enum HistoricalOrderType {
  MARKET = "MARKET",
  LIMIT = "LIMIT",
  STOP = "STOP",
  MARKET_RANGE = "MARKET_RANGE",
  STOP_LIMIT = "STOP_LIMIT",
  STOP_LOSS_TAKE_PROFIT = "STOP_LOSS_TAKE_PROFIT",
}
