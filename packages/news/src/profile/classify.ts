/**
 * Classe d'actif à partir de base/quote/nom.
 *
 * L'ordre des tests compte : un `US100` n'est **pas** du forex (pas deux
 * devises ISO), un `XAUUSD` n'est **pas** du forex non plus (base métal)
 * même si la quote est `USD`. La classification se fait sur les tables de
 * bases / tickers, pas sur des regex de titre (celles-ci servent au filtrage
 * d'events, pas au ticker).
 */

import { CRYPTO_BASES, ENERGY_BASES, INDEX_HOME, METAL_BASES } from "./aliases.ts";
import { isIsoCurrency, normalizeSymbolName } from "./symbol.ts";
import type { AssetClass } from "./types.ts";

/**
 * Classe macro du symbole.
 *
 * @example
 * classifyAssetClass("EUR", "USD", "EURUSD")  // "forex"
 * classifyAssetClass("XAU", "USD", "XAUUSD")  // "metal"
 * classifyAssetClass("US100", "USD", "US100") // "index"
 */
export function classifyAssetClass(base: string, quote: string, symbolName: string): AssetClass {
  const cleaned = normalizeSymbolName(symbolName);
  if (INDEX_HOME[cleaned] !== undefined || INDEX_HOME[base] !== undefined) return "index";
  if (METAL_BASES.has(base)) return "metal";
  if (ENERGY_BASES.has(base)) return "energy";
  if (CRYPTO_BASES.has(base)) return "crypto";
  if (isIsoCurrency(base) && isIsoCurrency(quote) && base !== quote) return "forex";
  return "other";
}
