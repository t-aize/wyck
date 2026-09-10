/**
 * Un asset (devise, métal, indice…) référencé par `baseAssetId` / `quoteAssetId`
 * d'un {@link CtraderSymbol}.
 */
export interface CtraderAsset {
  assetId: number;
  /** Code court (`XAU`, `USD`, `EUR`). */
  name: string;
  displayName: string;
}
