/**
 * Lectures du book : positions ouvertes et ordres (attachés + pendings).
 *
 * Deux outils distincts :
 *
 * - `get_positions` — positions + souvent seulement les SL/TP attachés ;
 * - `get_pending_orders` — **tous** les pendings du compte, tous symboles.
 *
 * L'app fusionne les deux listes d'ordres (les ids se recoupent).
 */

import { z } from "zod";
import { CtraderOrderSchema } from "./order.ts";
import { CtraderPositionSchema } from "./position.ts";

/** Enveloppe `get_positions`. */
export const GetPositionsResultSchema = z.object({
  positions: z.array(CtraderPositionSchema),
  /** Souvent les SL/TP auto, pas l'intégralité des pendings — cf. {@link GetPendingOrdersResult}. */
  orders: z.array(CtraderOrderSchema),
});
/** @see GetPositionsResultSchema */
export type GetPositionsResult = z.infer<typeof GetPositionsResultSchema>;

/**
 * Enveloppe `get_pending_orders` : tous les ordres en attente du compte
 * (tous symboles), distinct de `get_positions.orders`.
 */
export const GetPendingOrdersResultSchema = z.object({
  orders: z.array(CtraderOrderSchema),
  /** Pagination serveur : `true` s'il reste une page. */
  hasMore: z.boolean(),
});
/** @see GetPendingOrdersResultSchema */
export type GetPendingOrdersResult = z.infer<typeof GetPendingOrdersResultSchema>;
