/**
 * Résultat de `get_balance`.
 *
 * Les montants (`balance`, `equity`, `freeMargin`) sont des entiers à l'échelle
 * {@link GetBalanceResult.moneyDigits} (ex. `moneyDigits = 2` → centimes).
 * L'affichage (division, locale) vit côté app (`formatMoney`).
 */
export interface GetBalanceResult {
  /** Entier à l'échelle `moneyDigits` (ex. moneyDigits=2 → centimes). */
  balance: number;
  /** Même échelle que `balance`. */
  equity: number;
  /** Même échelle que `balance`. */
  freeMargin: number;
  balanceVersion: number;
  /** Nombre de décimales de la devise de dépôt. */
  moneyDigits: number;
  /** Asset de dépôt (cf. {@link CtraderAsset.assetId}). */
  depositAssetId: number;
}
