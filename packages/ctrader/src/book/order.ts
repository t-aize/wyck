/**
 * Ordre tel que le book / l'historique le renvoie.
 *
 * Vérifié via `get_order_history` (10 ordres réels, XAUUSD). Les champs propres
 * aux ordres *en attente* (label, comment, timeInForce, statut) restent non
 * vérifiés : aucun pending observé lors de ce test.
 *
 * `expirationTimestamp` est repris malgré tout (même nom que dans
 * {@link CreateOrderParams} / {@link AmendOrderParams}) — sans lui, un amend
 * manuel ne peut pas le renvoyer et cTrader **l'efface silencieusement**.
 */

import { z } from "zod";
import { HistoricalOrderTypeSchema, TradeSideSchema } from "../protocol/enums.ts";

/** Schéma d'un ordre du book. Plus strict que {@link CtraderPositionSchema} : forme confirmée. */
export const CtraderOrderSchema = z.object({
  orderId: z.number(),
  symbolId: z.number(),
  /** Inclut `STOP_LOSS_TAKE_PROFIT` pour les protections auto d'une position. */
  orderType: HistoricalOrderTypeSchema,
  tradeSide: TradeSideSchema,
  /** 1/100 d'unité d'actif de base, comme {@link CreateOrderParams.volume}. */
  volume: z.number(),
  /** Prix affiché, présent sur LIMIT / STOP_LIMIT. */
  limitPrice: z.number().optional(),
  /** Prix affiché, présent sur STOP / STOP_LIMIT. */
  stopPrice: z.number().optional(),
  stopLoss: z.number().optional(),
  takeProfit: z.number().optional(),
  /** Epoch ms. Omis = GTC côté serveur après un amend qui ne le renvoie pas. */
  expirationTimestamp: z.number().optional(),
});

/** @see CtraderOrderSchema */
export type CtraderOrder = z.infer<typeof CtraderOrderSchema>;
