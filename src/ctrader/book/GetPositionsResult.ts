import type { CtraderOrder } from "./CtraderOrder.ts";
import { type CtraderPosition, mapPosition } from "./CtraderPosition.ts";

/**
 * Enveloppe de `get_positions`.
 *
 * `orders` n'est souvent **que** les SL/TP attachés aux positions ouvertes —
 * pas l'intégralité des pendings. Pour ça, {@link GetPendingOrdersResult}.
 * L'app fusionne les deux listes (les ids se recoupent).
 */
export interface GetPositionsResult {
  positions: CtraderPosition[];
  orders: CtraderOrder[];
}

/**
 * JSON brut `get_positions` → {@link GetPositionsResult}.
 * Les positions passent par {@link mapPosition} ; les ordres sont pris tels quels.
 */
export function mapGetPositionsResult(raw: unknown): GetPositionsResult {
  const record = raw !== null && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const positions = Array.isArray(record.positions) ? record.positions.map(mapPosition) : [];
  const orders = Array.isArray(record.orders) ? (record.orders as CtraderOrder[]) : [];
  return { positions, orders };
}
