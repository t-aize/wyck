/**
 * Un symbole cTrader tel que `get_symbols` le renvoie.
 *
 * `symbolName` est le ticker **broker** (`XAUUSD`, `US100.r`…) : le suffixe est
 * conservé. La normalisation (classe d'actif, base/quote) vit dans `@aurum/news`.
 */
export interface CtraderSymbol {
  symbolId: number;
  /** Ticker broker (`XAUUSD`, `US100.r`…) — suffixe conservé. */
  symbolName: string;
  enabled: boolean;
  /** Asset de base (cf. {@link CtraderAsset.assetId}). */
  baseAssetId: number;
  /** Asset de cotation. */
  quoteAssetId: number;
  symbolCategoryId: number;
  description: string;
}
