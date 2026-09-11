import type { CtraderSpotPrice } from "./CtraderSpotPrice.ts";

/** Enveloppe de `get_spot_prices`. */
export interface GetSpotPricesResult {
  prices: CtraderSpotPrice[];
}
