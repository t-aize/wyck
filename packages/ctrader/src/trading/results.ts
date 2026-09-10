/**
 * Résultats des 5 outils d'écriture (`create_order`, `amend_order`,
 * `cancel_order`, `amend_position`, `close_position`).
 *
 * Forme réelle confirmée (compte démo, XAUUSD), identique pour les cinq :
 * `{ orderId, positionId, executionType, order, position, deal? }`.
 *
 * - `executionType` : valeurs observées `ORDER_ACCEPTED` / `ORDER_REPLACED` /
 *   `ORDER_CANCELLED` / `ORDER_FILLED` — liste probablement non exhaustive
 *   (rejets, partiels jamais déclenchés pendant le test).
 * - `order` / `position` ne décrivent pas toujours l'action « principale » :
 *   `close_position` sur une position dont le SL/TP auto n'a jamais sauté
 *   peut renvoyer `ORDER_CANCELLED` pour *cet* ordre SL/TP, pas le MARKET
 *   qui a réellement clôturé.
 * - `deal` (souvent absent) : présent seulement si l'appel déclenche une
 *   exécution immédiate. Forme minimale distincte d'un deal d'historique :
 *   `{ dealId, volume, closePrice }` (pas de `symbolId` / `tradeSide`).
 *
 * Restent un {@link PermissiveRecordSchema} : l'app ne lit que succès / échec
 * de la Promise. Verrouiller n'apporterait aucun bénéfice pour le risque
 * (un `safeParse` qui échoue sur un cas non testé transformerait un ordre
 * *réussi* en échec apparent côté UI).
 */

import type { z } from "zod";
import { PermissiveRecordSchema } from "../protocol/record.ts";

const WriteResultSchema = PermissiveRecordSchema;

/** Résultat de `create_order` — record permissif, cf. en-tête du fichier. */
export const CreateOrderResultSchema = WriteResultSchema;
/** @see CreateOrderResultSchema */
export type CreateOrderResult = z.infer<typeof CreateOrderResultSchema>;

/** Résultat de `amend_order`. */
export const AmendOrderResultSchema = WriteResultSchema;
/** @see AmendOrderResultSchema */
export type AmendOrderResult = z.infer<typeof AmendOrderResultSchema>;

/** Résultat de `cancel_order`. */
export const CancelOrderResultSchema = WriteResultSchema;
/** @see CancelOrderResultSchema */
export type CancelOrderResult = z.infer<typeof CancelOrderResultSchema>;

/** Résultat de `amend_position`. */
export const AmendPositionResultSchema = WriteResultSchema;
/** @see AmendPositionResultSchema */
export type AmendPositionResult = z.infer<typeof AmendPositionResultSchema>;

/** Résultat de `close_position`. */
export const ClosePositionResultSchema = WriteResultSchema;
/** @see ClosePositionResultSchema */
export type ClosePositionResult = z.infer<typeof ClosePositionResultSchema>;
