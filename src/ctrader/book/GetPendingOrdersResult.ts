import type { CtraderOrder } from "./CtraderOrder.ts";

/**
 * Enveloppe de `get_pending_orders` : **tous** les ordres en attente du compte
 * (tous symboles), distinct de `get_positions.orders`.
 */
export interface GetPendingOrdersResult {
  orders: CtraderOrder[];
  /** Pagination serveur : `true` s'il reste une page. */
  hasMore: boolean;
}
