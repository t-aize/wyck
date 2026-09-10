import type { HistoricalOrderType } from "../protocol/HistoricalOrderType.ts";
import type { TradeSide } from "../protocol/TradeSide.ts";

/**
 * Ordre tel que le book / l'historique le renvoie.
 *
 * Vérifié via `get_order_history` (10 ordres réels, XAUUSD). Les champs propres
 * aux ordres *en attente* (label, comment, timeInForce, statut) restent non
 * observés.
 *
 * `expirationTimestamp` est repris malgré tout (même nom que dans
 * {@link CreateOrderParams} / {@link AmendOrderParams}) — sans lui, un amend
 * manuel ne peut pas le renvoyer et cTrader **l'efface silencieusement**.
 */
export interface CtraderOrder {
  orderId: number;
  symbolId: number;
  /** Inclut `STOP_LOSS_TAKE_PROFIT` pour les protections auto d'une position. */
  orderType: HistoricalOrderType;
  tradeSide: TradeSide;
  /** 1/100 d'unité d'actif de base, comme {@link CreateOrderParams.volume}. */
  volume: number;
  /** Prix affiché, présent sur LIMIT / STOP_LIMIT. */
  limitPrice?: number;
  /** Prix affiché, présent sur STOP / STOP_LIMIT. */
  stopPrice?: number;
  stopLoss?: number;
  takeProfit?: number;
  /** Epoch ms. Omis = GTC côté serveur après un amend qui ne le renvoie pas. */
  expirationTimestamp?: number;
}
