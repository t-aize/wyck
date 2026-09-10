/**
 * Vocabulaire d'ordre du protocole cTrader.
 *
 * Schémas zod même si des Params les utilisent : ils doivent être validés côté
 * Results (ex. {@link CtraderOrderSchema}.`tradeSide`). Les Params empruntent
 * le `z.infer` ({@link OrderType}, {@link TradeSide}…) sans jamais appeler le
 * schéma — rien à valider à cette frontière, c'est l'app qui construit.
 */

import { z } from "zod";

/** Types d'ordre que l'on *envoie* à `create_order`. */
export const OrderTypeSchema = z.enum(["MARKET", "LIMIT", "STOP", "MARKET_RANGE", "STOP_LIMIT"]);
/** @see OrderTypeSchema */
export type OrderType = z.infer<typeof OrderTypeSchema>;

/** Sens d'un trade / d'une position. */
export const TradeSideSchema = z.enum(["BUY", "SELL"]);
/** @see TradeSideSchema */
export type TradeSide = z.infer<typeof TradeSideSchema>;

/** Validité temporelle d'un ordre (création). */
export const TimeInForceSchema = z.enum([
  "GOOD_TILL_CANCEL",
  "GOOD_TILL_DATE",
  "IMMEDIATE_OR_CANCEL",
]);
/** @see TimeInForceSchema */
export type TimeInForce = z.infer<typeof TimeInForceSchema>;

/**
 * Type d'ordre tel que *renvoyé* par le book / l'historique, en plus de
 * {@link OrderType} : le serveur y inclut les SL/TP auto-générés d'une position
 * (`STOP_LOSS_TAKE_PROFIT`).
 */
export const HistoricalOrderTypeSchema = z.union([
  OrderTypeSchema,
  z.literal("STOP_LOSS_TAKE_PROFIT"),
]);
/** @see HistoricalOrderTypeSchema */
export type HistoricalOrderType = z.infer<typeof HistoricalOrderTypeSchema>;
