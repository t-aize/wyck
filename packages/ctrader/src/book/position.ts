/**
 * Position ouverte, projetée depuis un record permissif.
 *
 * Forme réelle confirmée (compte démo, XAUUSD) :
 * `{ positionId, symbolId, tradeSide, volume, entryPrice, stopLoss?, takeProfit?,
 * commission, swap }`. `volume` / `entryPrice` valent `0` sur le stub associé à
 * un ordre pending pas encore rempli (champ `position` de `create_order` LIMIT).
 * Pas de P&L latent ni d'`openTimestamp` observé sur cette forme.
 *
 * `volume` reste en **unités API** (1/100 d'unité d'actif de base). Le passage
 * en lots dépend du `lotSize` du symbole — connu seulement après `get_symbols`
 * / `get_assets`, côté app (`instrument/specs.ts`).
 *
 * Toujours {@link PermissiveRecordSchema} + `.transform()` : un schéma strict
 * qui se trompe planterait l'affichage des positions. L'UI détecte après coup
 * si les champs à haute confiance n'ont rien résolu (`isUnmapped`).
 */

import type { z } from "zod";
import { PermissiveRecordSchema, readNumber, readTradeSide } from "../protocol/record.ts";

/**
 * Record JSON → forme utile à l'UI. Chaque champ `undefined` si absent / du
 * mauvais type, jamais d'échec de parse.
 *
 * @example
 * CtraderPositionSchema.parse({ positionId: 7, tradeSide: "BUY", volume: 5000, entryPrice: 2000 })
 * // { id: 7, side: "BUY", volume: 5000, entry: 2000, … }
 */
export const CtraderPositionSchema = PermissiveRecordSchema.transform((position) => {
  return {
    id: readNumber(position, ["positionId"]),
    symbolId: readNumber(position, ["symbolId"]),
    side: readTradeSide(position),
    volume: readNumber(position, ["volume"]),
    entry: readNumber(position, ["entryPrice"]),
    stopLoss: readNumber(position, ["stopLoss"]),
    takeProfit: readNumber(position, ["takeProfit"]),
    swap: readNumber(position, ["swap"]),
  };
});

/** Position telle que l'app la manipule. Tous les champs peuvent être `undefined`. */
export type CtraderPosition = z.infer<typeof CtraderPositionSchema>;

/**
 * {@link CtraderPosition} dont `id` a été confirmé résolu — prérequis pour
 * `amend_position`.
 */
export type AmendablePosition = CtraderPosition & { id: number };

/**
 * {@link AmendablePosition} dont `volume` API a lui aussi été confirmé — prérequis
 * pour `close_position` (le serveur exige le volume à clôturer).
 */
export type ClosablePosition = AmendablePosition & { volume: number };
