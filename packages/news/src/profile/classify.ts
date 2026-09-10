/**
 * Classe d'actif à partir de base/quote/nom.
 *
 * L'ordre des tests compte : un `US100` n'est **pas** du forex (pas deux
 * devises ISO), un `XAUUSD` n'est **pas** du forex non plus (base métal)
 * même si la quote est `USD`.
 */

import {
  CRYPTO_BASES,
  CRYPTO_KEYWORDS,
  ENERGY_BASES,
  ENERGY_KEYWORDS,
  INDEX_HOME,
  METAL_BASES,
  METAL_KEYWORDS,
} from "./aliases.ts";
import { isFfCurrency, normalizeSymbolName } from "./symbol.ts";
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
  if (METAL_BASES.has(base) || METAL_KEYWORDS.test(cleaned)) return "metal";
  if (ENERGY_BASES.has(base) || ENERGY_KEYWORDS.test(cleaned)) return "energy";
  if (CRYPTO_BASES.has(base) || CRYPTO_KEYWORDS.test(cleaned)) return "crypto";
  if (isFfCurrency(base) && isFfCurrency(quote) && base !== quote) return "forex";
  return "other";
}
