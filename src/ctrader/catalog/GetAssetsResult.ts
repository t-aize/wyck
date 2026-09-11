import type { CtraderAsset } from "./CtraderAsset.ts";

/** Enveloppe de `get_assets`. */
export interface GetAssetsResult {
  assets: CtraderAsset[];
}
