/**
 * Spécifications d'un symbole cTrader : classe d'actif (via `src/news`), taille de lot,
 * digits, pip. `get_symbols` ne renvoie pas lotSize/digits — on les infère de la classe,
 * avec base/quote idéalement issus de `get_assets`.
 *
 * Volume API = lots × lotSize × 100 (cf. CreateOrderParams). Forex lotSize=100000,
 * métaux XAU=100 / XAG=5000, indices/crypto=1, énergie=1000 — valeurs cTrader
 * standard, le broker peut dévier.
 */

import type { CtraderAsset } from "../ctrader/catalog/CtraderAsset.ts";
import type { CtraderSymbol } from "../ctrader/catalog/CtraderSymbol.ts";
import { newsProfile } from "../news/profile/profile.ts";
import type { AssetClass, NewsProfile } from "../news/profile/types.ts";

export interface InstrumentSpecs {
  symbolId: number;
  symbolName: string;
  enabled: boolean;
  description: string;
  base: string;
  quote: string;
  assetClass: AssetClass;
  lotSize: number;
  digits: number;
  pipSize: number;
  news: NewsProfile;
}

export function lotSizeFor(assetClass: AssetClass, base: string): number {
  switch (assetClass) {
    case "forex":
      return 100_000;
    case "metal":
      return base === "XAG" || base === "SILVER" ? 5_000 : 100;
    case "index":
    case "crypto":
      return 1;
    case "energy":
      return 1_000;
    default:
      return 100_000;
  }
}

export function digitsFor(assetClass: AssetClass, base: string, quote: string): number {
  if (assetClass === "forex") return base === "JPY" || quote === "JPY" ? 3 : 5;
  return 2;
}

export function pipSizeFor(assetClass: AssetClass, base: string, quote: string): number {
  if (assetClass === "forex") return base === "JPY" || quote === "JPY" ? 0.01 : 0.0001;
  if (assetClass === "metal") return base === "XAG" || base === "SILVER" ? 0.01 : 0.1;
  if (assetClass === "index") return 1;
  if (assetClass === "crypto") return 1;
  if (assetClass === "energy") return 0.01;
  return 0.0001;
}

/** Pas de volume = 0.01 lot, converti en unités API. */
export function volumeStep(lotSize: number): number {
  return Math.max(1, Math.round(0.01 * lotSize * 100));
}

export function specsFromSymbol(
  symbol: CtraderSymbol,
  assetsById: ReadonlyMap<number, CtraderAsset>,
): InstrumentSpecs {
  const news = newsProfile({
    symbolName: symbol.symbolName,
    base: assetsById.get(symbol.baseAssetId)?.name,
    quote: assetsById.get(symbol.quoteAssetId)?.name,
  });
  return {
    symbolId: symbol.symbolId,
    symbolName: symbol.symbolName,
    enabled: symbol.enabled,
    description: symbol.description,
    base: news.base,
    quote: news.quote,
    assetClass: news.assetClass,
    lotSize: lotSizeFor(news.assetClass, news.base),
    digits: digitsFor(news.assetClass, news.base, news.quote),
    pipSize: pipSizeFor(news.assetClass, news.base, news.quote),
    news,
  };
}

export function buildCatalog(
  symbols: readonly CtraderSymbol[],
  assets: readonly CtraderAsset[],
): InstrumentSpecs[] {
  const assetsById = new Map(assets.map((asset) => [asset.assetId, asset]));
  return symbols
    .filter((symbol) => symbol.enabled)
    .map((symbol) => specsFromSymbol(symbol, assetsById));
}

export function findInstrument(
  catalog: readonly InstrumentSpecs[],
  name: string,
): InstrumentSpecs | undefined {
  const needle = name.trim().toUpperCase();
  const exact = catalog.find((item) => item.symbolName.toUpperCase() === needle);
  if (exact) return exact;
  const prefixed = catalog.filter((item) => item.symbolName.toUpperCase().startsWith(needle));
  return prefixed.length === 1 ? prefixed[0] : undefined;
}
