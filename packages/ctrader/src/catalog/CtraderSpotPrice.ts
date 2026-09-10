/**
 * Un tick spot (`get_spot_prices`).
 *
 * Tous les prix sont des entiers × 10⁵ (ex. `410177000` → `4101.77`). La
 * conversion d'affichage vit côté app (`PRICE_SCALE`).
 */
export interface CtraderSpotPrice {
  symbolId: number;
  /** Prix à l'échelle × 10⁵ (ex. 410177000 → 4101.77). */
  bid: number;
  ask: number;
  high: number;
  low: number;
  sessionClose: number;
  /** Epoch ms. */
  timestamp: number;
}
