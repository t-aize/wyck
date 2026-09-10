/**
 * Compte : résultat de `get_balance`.
 *
 * Les montants (`balance`, `equity`, `freeMargin`) sont des entiers à l'échelle
 * {@link GetBalanceResult.moneyDigits} (ex. `moneyDigits = 2` → centimes).
 * L'affichage (division, locale) vit côté app (`formatMoney`).
 */

import { z } from "zod";

/** Payload `get_balance`. */
export const GetBalanceResultSchema = z.object({
  /** Entier à l'échelle `moneyDigits` (ex. moneyDigits=2 → centimes). */
  balance: z.number(),
  /** Même échelle que `balance`. */
  equity: z.number(),
  /** Même échelle que `balance`. */
  freeMargin: z.number(),
  balanceVersion: z.number(),
  /** Nombre de décimales de la devise de dépôt. */
  moneyDigits: z.number(),
  /** Asset de dépôt (cf. {@link CtraderAsset.assetId}). */
  depositAssetId: z.number(),
});

/** @see GetBalanceResultSchema */
export type GetBalanceResult = z.infer<typeof GetBalanceResultSchema>;
