/**
 * Une bougie OHLC (`get_trendbars`).
 *
 * Prix en entier × 10⁵, comme {@link CtraderSpotPrice}. `timestamp` = epoch ms
 * de l'**ouverture** de la bougie.
 */
export interface CtraderTrendbar {
  /** Epoch ms de l'ouverture. */
  timestamp: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}
