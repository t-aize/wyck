/**
 * Normalisation d'un ticker cTrader et inférence base/quote.
 *
 * Idéalement l'app passe `base`/`quote` issus de `get_assets` (fiable).
 * Le parsing du nom n'est qu'un fallback (broker qui n'a pas renvoyé les
 * assets, ou appel hors connexion).
 */

import { BROKER_SUFFIX, FF_CURRENCIES, QUOTE_ALIASES, QUOTE_CANDIDATES } from "./aliases.ts";

/**
 * Ticker en majuscules, suffixe broker retiré.
 *
 * @example
 * normalizeSymbolName("xauusd.r") // "XAUUSD"
 * normalizeSymbolName("US100_SB") // "US100"
 */
export function normalizeSymbolName(symbolName: string): string {
  return symbolName.trim().toUpperCase().replace(BROKER_SUFFIX, "");
}

/**
 * Canonise un code devise : `USDT`/`USDC` → `USD`, `CNH` → `CNY`, le reste
 * inchangé (uppercase).
 */
export function canonicalCurrency(code: string): string {
  const upper = code.toUpperCase();
  return QUOTE_ALIASES[upper] ?? upper;
}

/** `true` si le code (après canonisation) existe dans la colonne ForexFactory. */
export function isFfCurrency(code: string): boolean {
  return FF_CURRENCIES.has(canonicalCurrency(code));
}

/**
 * Déduit `{ base, quote }` d'un symbole.
 *
 * Ordre :
 * 1. Assets cTrader si les deux sont fournis (source de vérité).
 * 2. Suffixe quote connu (`EURUSD` → EUR/USD, `BTCUSDT` → BTC/USD).
 * 3. Découpage 6 lettres (filet pour une paire ISO+ISO).
 * 4. Ticker entier en base, quote `USD` (indices `US100`, `GER40`…).
 *
 * @param symbolName - Nom brut cTrader (suffixe OK).
 * @param baseFromAssets - `assets[symbol.baseAssetId].name`, si on l'a.
 * @param quoteFromAssets - idem quote.
 */
export function inferBaseQuote(
  symbolName: string,
  baseFromAssets?: string,
  quoteFromAssets?: string,
): { base: string; quote: string } {
  if (baseFromAssets && quoteFromAssets) {
    return {
      base: canonicalCurrency(baseFromAssets),
      quote: canonicalCurrency(quoteFromAssets),
    };
  }

  const cleaned = normalizeSymbolName(symbolName);
  for (const quote of QUOTE_CANDIDATES) {
    if (cleaned.endsWith(quote) && cleaned.length > quote.length) {
      return {
        base: cleaned.slice(0, -quote.length),
        quote: canonicalCurrency(quote),
      };
    }
  }
  if (cleaned.length === 6) {
    return { base: cleaned.slice(0, 3), quote: canonicalCurrency(cleaned.slice(3)) };
  }
  return { base: cleaned, quote: "USD" };
}
