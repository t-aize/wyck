/**
 * Construction du {@link NewsProfile} : pays ForexFactory + mots-clés de titre.
 *
 * C'est **le** point d'entrée du dossier `profile/`. L'app (et les tests)
 * n'ont en pratique besoin que de `newsProfile({ symbolName, base?, quote? })`.
 */

import {
  CRYPTO_KEYWORDS,
  ENERGY_KEYWORDS,
  INDEX_HOME,
  INDEX_KEYWORDS,
  METAL_KEYWORDS,
  NO_KEYWORDS,
} from "./aliases.ts";
import { classifyAssetClass } from "./classify.ts";
import { canonicalCurrency, inferBaseQuote, isFfCurrency, normalizeSymbolName } from "./symbol.ts";
import type { AssetClass, NewsProfile } from "./types.ts";

/**
 * Devises ForexFactory à écouter pour cette classe.
 *
 * - forex : base **et** quote (EURUSD → EUR + USD).
 * - indice : devise « home » (`GER40` → EUR) **et** USD (NFP / CPI / FOMC
 *   bougent DAX, FTSE, Nikkei autant que le Nasdaq).
 * - métal / crypto / énergie / other : devise de cotation, USD par défaut.
 */
function countriesFor(
  assetClass: AssetClass,
  base: string,
  quote: string,
  symbolName: string,
): string[] {
  const countries = new Set<string>();
  const cleaned = normalizeSymbolName(symbolName);

  const addIfFf = (code: string) => {
    if (isFfCurrency(code)) countries.add(canonicalCurrency(code));
  };

  switch (assetClass) {
    case "forex":
      addIfFf(base);
      addIfFf(quote);
      break;
    case "index": {
      const home = INDEX_HOME[cleaned] ?? INDEX_HOME[base] ?? (isFfCurrency(quote) ? quote : "USD");
      addIfFf(home);
      addIfFf("USD");
      break;
    }
    case "metal":
    case "crypto":
    case "energy":
    case "other":
      addIfFf(quote);
      if (countries.size === 0) countries.add("USD");
      break;
  }

  return [...countries];
}

/** Mots-clés de titre pour rattraper un event hors devise (or, bitcoin, pétrole…). */
function keywordsFor(assetClass: AssetClass): RegExp {
  switch (assetClass) {
    case "metal":
      return METAL_KEYWORDS;
    case "crypto":
      return CRYPTO_KEYWORDS;
    case "energy":
      return ENERGY_KEYWORDS;
    case "index":
      return INDEX_KEYWORDS;
    default:
      return NO_KEYWORDS;
  }
}

/**
 * Profil news d'un symbole.
 *
 * @param input.symbolName - Ticker cTrader (suffixe broker OK).
 * @param input.base - Nom d'asset cTrader si on l'a (`get_assets`).
 * @param input.quote - Idem quote. Sans les deux, on parse le ticker.
 *
 * @example
 * newsProfile({ symbolName: "EURUSD" }).countries // ["EUR", "USD"]
 * newsProfile({ symbolName: "US100" }).assetClass // "index"
 */
export function newsProfile(input: {
  symbolName: string;
  base?: string;
  quote?: string;
}): NewsProfile {
  const { base, quote } = inferBaseQuote(input.symbolName, input.base, input.quote);
  const assetClass = classifyAssetClass(base, quote, input.symbolName);
  return {
    symbolName: input.symbolName,
    assetClass,
    base,
    quote,
    countries: countriesFor(assetClass, base, quote, input.symbolName),
    keywords: keywordsFor(assetClass),
  };
}
